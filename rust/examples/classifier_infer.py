"""Offline qualification using the installed local llama.cpp GPU runner; no database writes.

classifier_infer.py REQUESTS.jsonl REPLIES.jsonl --model OLLAMA_TAG [...]
Reuses installed Ollama models and their exact templates/tokenizer. Native classifier_qualify decodes the retained replies.
"""
import argparse
import hashlib
import json
import re
import subprocess
import time
from pathlib import Path
from urllib.error import URLError
from urllib.request import Request, urlopen


def call(url, path, payload=None):
    request = Request(url + path, data=None if payload is None else json.dumps(payload).encode(),
                      headers={"Content-Type": "application/json"})
    with urlopen(request, timeout=600) as response:
        return json.load(response)


def proposal_schema(request):
    contract = request["output_contract"]
    quote = {"type": "object", "additionalProperties": False, "required": ["quote", "occurrence"],
             "properties": {"quote": {"type": "string", "minLength": 1},
                            "occurrence": {"type": "integer", "minimum": 0}}}
    spans = {"anyOf": [{"type": "null"}, {"type": "array", "minItems": 1, "items": quote}]}
    properties = {"evidence": quote, "target_evidence": spans,
                  "candidate_dimensions": {"type": "array", "items": {
                      "type": "string", "enum": list(contract["emotion_labels"])}},
                  "qualifiers": {"type": "object", "additionalProperties": False,
                                 "required": list(contract["claims"][0]["qualifiers"]),
                                 "properties": {key: spans for key in contract["claims"][0]["qualifiers"]}}}
    properties.update({key: {"type": "string", "enum": contract["claims"][0][key]}
                       for key in ("target_relation", "kind", "time_scope")})
    return {"type": "object", "additionalProperties": False,
            "required": ["complete_source_review", "extraction_usable", "claims"],
            "properties": {"complete_source_review": {"type": "boolean"},
                           "extraction_usable": {"type": "boolean"},
                           "claims": {"type": "array", "items": {
                               "type": "object", "additionalProperties": False,
                               "required": list(properties), "properties": properties}}}}


def request_hash(request):
    return hashlib.sha256(json.dumps(request, ensure_ascii=False, sort_keys=True,
                                     separators=(",", ":")).encode()).hexdigest()


def budget(tokens, output, context):
    if not tokens or min(output, context) < 1 or len(tokens) + output > context:
        raise ValueError(f"full input ({len(tokens)}) + reserved output ({output}) exceeds context ({context})")


def complete(response):
    if (response.get("stop") is not True or response.get("stop_type") != "eos"
            or response.get("truncated") is not False):
        raise ValueError("incomplete generation or context truncation")
    return response["content"]


def render(template, messages, renderer):
    if "{%" in template or "{%-" in template:
        from transformers.utils.chat_template_utils import _compile_jinja_template
        return _compile_jinja_template(template).render(messages=messages, tools=None, xml_tools=None,
            python_tools=None, add_generation_prompt=True, enable_thinking=False)
    legacy_system = ".System" in template
    system = messages[0]["content"] if legacy_system else ""
    selected = messages[1:] if legacy_system else messages
    data = {"System": system, "Messages": [{"Role": m["role"], "Content": m["content"],
            "Thinking": "", "ToolCalls": []} for m in selected], "Tools": [],
            "IsThinkSet": True, "Think": False, "Prompt": "", "Response": ""}
    result = subprocess.run([renderer], input=json.dumps({"template": template, "data": data}),
                            text=True, capture_output=True, check=True)
    return result.stdout


def runner_url(ollama, tag, model_path, context):
    call(ollama, "/api/generate", {"model": tag, "keep_alive": "10m", "options": {
        "num_ctx": context, "num_gpu": 99}})
    loaded = next(row for row in call(ollama, "/api/ps")["models"] if row["name"] == tag)
    if loaded["size_vram"] != loaded["size"]:
        raise ValueError("checkpoint is not fully resident on the GPU")
    processes = subprocess.run(["ps", "-eo", "args"], capture_output=True, text=True, check=True).stdout
    matching = [line for line in processes.splitlines()
                if "llama-server --model " + model_path + " " in line]
    if len(matching) != 1:
        raise ValueError("unique model-bound local runner not found")
    port = re.search(r"--port (\d+)", matching[0]).group(1)
    return "http://127.0.0.1:" + port, loaded, matching[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("requests", type=Path)
    parser.add_argument("replies", type=Path)
    parser.add_argument("--model", action="append", required=True)
    parser.add_argument("--renderer", default="./classifier-render")
    parser.add_argument("--ollama", default="http://127.0.0.1:11434")
    parser.add_argument("--context", type=int, default=8192)
    parser.add_argument("--output-tokens", type=int, default=3072)
    args = parser.parse_args()
    requests = [json.loads(line) for line in args.requests.read_text().splitlines() if line.strip()]
    if not requests or len({(row["article_id"], json.dumps(row["target"], sort_keys=True))
                            for row in requests}) != len(requests):
        parser.error("unique source/target requests required")
    for row in requests:
        if request_hash(row["request"]) != row["request_sha256"]:
            parser.error("native qualification request hash mismatch")
    inventory = {row["name"]: row for row in call(args.ollama, "/api/tags")["models"]}
    if len(set(args.model)) != len(args.model) or any(model not in inventory for model in args.model):
        parser.error("unique exact installed model tags required")
    failures = 0
    with args.replies.open("x") as output:
        for tag in args.model:
            show = call(args.ollama, "/api/show", {"model": tag})
            model_path = re.search(r"^FROM (.+)$", show["modelfile"], re.M).group(1)
            contexts = [value for key, value in show["model_info"].items() if key.endswith(".context_length")]
            if not contexts or args.context > min(contexts):
                parser.error("requested context exceeds the checkpoint's native limit")
            try:
                url, loaded, command = runner_url(args.ollama, tag, model_path, args.context)
                resolved = call(args.ollama, "/api/show", {"model": tag})
                if resolved["template"] != show["template"] or re.search(
                        r"^FROM (.+)$", resolved["modelfile"], re.M).group(1) != model_path:
                    raise ValueError("checkpoint weights or template changed during setup")
                # Freeze the resolved manifest after loading; Ollama can normalize metadata on load.
                initial_inventory = inventory[tag]
                inventory[tag] = next(row for row in call(args.ollama, "/api/tags")["models"] if row["name"] == tag)
                props = call(url, "/props")
                context = props["default_generation_settings"]["n_ctx"]
                if loaded["digest"] != inventory[tag]["digest"] or context != args.context:
                    raise ValueError(f"model identity or context differs from the requested run: "
                        f"digest={loaded['digest']} expected={inventory[tag]['digest']} "
                        f"context={context} requested={args.context}")
                provenance = {"initial_inventory": initial_inventory, "inventory": inventory[tag], "gguf_blob": model_path,
                    "weights_identity": "installed Ollama manifest and GGUF SHA256 filename; protected weights not rehashed",
                    "server_command": command, "context": context, "loaded": loaded,
                    "template": show["template"], "production_eligible": False}
                for row in requests:
                    receipt = {key: row[key] for key in ("article_id", "target", "request_sha256")}
                    receipt.update(model=tag, revision=inventory[tag]["digest"], raw_response="",
                        model_provenance=provenance, native_request=row["request"], inference_status="error",
                        preflight=None, response=None)
                    started = time.perf_counter()
                    try:
                        native = row["request"]
                        user = json.dumps({"input": native["input"], "output_contract": native["output_contract"]},
                                          ensure_ascii=False)
                        messages = [{"role": "system", "content": native["system"]},
                                    {"role": "user", "content": user}]
                        schema = proposal_schema(native)
                        prompt = render(show["template"], messages, args.renderer)
                        if native["system"] not in prompt or user not in prompt:
                            raise ValueError("chat template omitted or changed source/system input")
                        tokens = call(url, "/tokenize", {"content": prompt, "add_special": True,
                                                       "parse_special": True})["tokens"]
                        receipt["preflight"] = {"rendered_prompt": prompt, "input_token_ids": tokens,
                            "input_tokens": len(tokens), "reserved_output_tokens": args.output_tokens,
                            "context": context, "complete_source": True, "thinking": False, "schema": schema}
                        budget(tokens, args.output_tokens, context)
                        payload = {"prompt": tokens, "json_schema": schema, "stream": False,
                            "cache_prompt": False, "n_predict": args.output_tokens, "temperature": 0, "seed": 42}
                        receipt["generation_request"] = payload
                        response = call(url, "/completion", payload)
                        receipt["response"] = response
                        receipt["raw_response"] = response.get("content", "")
                        complete(response)
                        if response["tokens_evaluated"] != len(tokens):
                            raise ValueError("generated prompt token count differs from exact preflight")
                        receipt["inference_status"] = "complete"
                    except Exception as error:
                        receipt["inference_error"] = f"{type(error).__name__}: {error}"
                        failures += 1
                    receipt["elapsed_ms"] = (time.perf_counter() - started) * 1000
                    output.write(json.dumps(receipt, ensure_ascii=False, allow_nan=False) + "\n")
                    output.flush()
                    print(f"{tag} article={row['article_id']} {receipt['inference_status']} ms={receipt['elapsed_ms']:.0f}", flush=True)
            finally:
                call(args.ollama, "/api/generate", {"model": tag, "keep_alive": 0})
    raise SystemExit(1 if failures else 0)


if __name__ == "__main__":
    main()
