#!/usr/bin/env python3
"""Screen local GGUFs on frozen synthetic emotion cases; no database or publication.

python3 examples/emotion_model_screen.py CASES.json MODELS.json OUTPUT_DIR [--repeats 2]
python3 examples/emotion_model_screen.py CASES.json --check
All models use the same llama-server build and no thinking.
"""
import argparse
import hashlib
import json
import statistics
import subprocess
import time
import urllib.request
from pathlib import Path

from plain_context_compare import SYSTEM


SCHEMA = {"oneOf": [
    {"type": "object", "additionalProperties": False, "required": ["score", "headline", "body"],
     "properties": {"score": {"type": "integer", "minimum": 0, "maximum": 100},
                    "headline": {"type": "string", "minLength": 1, "maxLength": 140},
                    "body": {"type": "string", "minLength": 1, "maxLength": 1200}}},
    {"type": "object", "additionalProperties": False, "required": ["score", "headline", "body"],
     "properties": {key: {"type": "null"} for key in ("score", "headline", "body")}}
]}


def http(base, path, payload=None):
    data = None if payload is None else json.dumps(payload).encode()
    request = urllib.request.Request(base + path, data, {"Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=180) as response:
        return json.load(response)


def assess(case, content):
    if case["mode"] == "plain":
        return {"nonempty": bool(content.strip())}
    try:
        value = json.loads(content)
    except (ValueError, TypeError):
        return {"valid_contract": False, "expected_decision": False}
    valid = isinstance(value, dict) and set(value) == {"score", "headline", "body"}
    abstained = valid and all(v is None for v in value.values())
    valid = valid and (abstained or (
        type(value["score"]) is int and 0 <= value["score"] <= 100
        and isinstance(value["headline"], str) and 0 < len(value["headline"]) <= 140
        and "\n" not in value["headline"] and "\r" not in value["headline"]
        and isinstance(value["body"], str) and 0 < len(value["body"]) <= 1200))
    expected = case["expected"]
    decision = valid and (abstained if expected == "abstain" else not abstained and (
        value["score"] > 50 if expected == "positive" else
        value["score"] < 50 if expected == "negative" else 35 <= value["score"] <= 65))
    return {"valid_contract": bool(valid), "expected_decision": bool(decision),
            "abstained": bool(abstained), "parsed": value}


def template_request(case):
    messages = [{"role": "user", "content": json.dumps(case["packet"], ensure_ascii=False)}]
    system = case.get("system", SYSTEM if case["mode"] == "plain" else None)
    if system:
        messages.insert(0, {"role": "system", "content": system})
    return {"messages": messages, "add_generation_prompt": True,
            "chat_template_kwargs": {"enable_thinking": False}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cases", type=Path)
    parser.add_argument("models", nargs="?", type=Path)
    parser.add_argument("output", nargs="?", type=Path)
    parser.add_argument("--repeats", type=int, default=2)
    parser.add_argument("--port", type=int, default=18190)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--constrain-json", action="store_true")
    args = parser.parse_args()
    cases = json.loads(args.cases.read_text())
    assert len({(c["mode"], c["name"]) for c in cases}) == len(cases)
    assert args.repeats > 0
    for case in cases:
        req = template_request(case)
        assert "review" not in req and "expected" not in req
        assert json.loads(req["messages"][-1]["content"]) == case["packet"]
    null = '{"score":null,"headline":null,"body":null}'
    assert assess({"mode": "json", "expected": "abstain"}, null)["expected_decision"]
    for bad in ['{}', '[]', '```json\n' + null + '\n```',
                '{"score":true,"headline":"x","body":"x"}',
                '{"score":null,"headline":"x","body":null}']:
        assert not assess({"mode": "json", "expected": "abstain"}, bad)["valid_contract"]
    if args.check:
        print(f"{len(cases)} cases checked; review labels excluded; contract checks passed.")
        return
    if args.models is None or args.output is None:
        parser.error("models and output required unless --check is used")
    models = json.loads(args.models.read_text())
    args.output.mkdir(exist_ok=False)
    (args.output / "cases.json").write_bytes(args.cases.read_bytes())
    (args.output / "models.json").write_bytes(args.models.read_bytes())
    (args.output / "runner.py").write_bytes(Path(__file__).read_bytes())
    version = subprocess.run(["llama-server", "--version"], capture_output=True, text=True, check=True)
    (args.output / "runtime.json").write_text(json.dumps({
        "version": version.stdout + version.stderr, "repeats": args.repeats,
        "cases_sha256": hashlib.sha256(args.cases.read_bytes()).hexdigest(),
        "runner_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "sampling": {"temperature": 0, "top_p": 1, "seed": 42},
        "thinking": False, "grammar_constrained": args.constrain_json, "cache_prompt": False,
        "scope": "Synthetic screening; score-band checks are not factual correctness."}, indent=2))
    base = f"http://127.0.0.1:{args.port}"
    # Refuse to attach to or replace an unrelated server.
    try:
        http(base, "/health")
    except urllib.error.URLError:
        pass
    else:
        raise RuntimeError("benchmark port already occupied")
    summary = {}
    with (args.output / "results.jsonl").open("x") as out:
        for model in models:
            name = model["name"]
            path = Path(model["path"])
            actual = hashlib.file_digest(path.open("rb"), "sha256").hexdigest()
            assert actual == model["sha256"], (name, "model hash mismatch")
            command = ["llama-server", "--model", str(path), "--alias", name,
                       "--host", "127.0.0.1", "--port", str(args.port),
                       "--ctx-size", "4096", "--parallel", "1", "--n-gpu-layers", "99",
                       "--flash-attn", "on", "--cache-ram", "0", "--jinja",
                       "--reasoning", "off", "--chat-template-kwargs", '{"enable_thinking":false}']
            (args.output / (name + "-command.json")).write_text(json.dumps(command))
            rows = []
            with (args.output / (name + "-server.log")).open("x") as log:
                process = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT)
                try:
                    for _ in range(120):
                        if process.poll() is not None:
                            raise RuntimeError(f"{name}: server exited; see log")
                        try:
                            if http(base, "/health").get("status") == "ok":
                                break
                        except (urllib.error.URLError, TimeoutError):
                            pass
                        time.sleep(1)
                    else:
                        raise TimeoutError(name + " server startup")
                    props = http(base, "/props")
                    (args.output / (name + "-props.json")).write_text(json.dumps(props, indent=2))
                    warm = http(base, "/apply-template", {"messages": [{"role": "user", "content": "Reply only OK."}], "chat_template_kwargs": {"enable_thinking": False}})
                    http(base, "/completion", {"prompt": warm["prompt"], "n_predict": 16, "temperature": 0, "cache_prompt": False})
                    for repeat in range(args.repeats):
                        for case in cases:
                            template = template_request(case)
                            prompt = http(base, "/apply-template", template)["prompt"]
                            request = {"prompt": prompt, "n_predict": 600 if case["mode"] == "json" else 2048,
                                       "temperature": 0, "top_p": 1, "seed": 42, "cache_prompt": False}
                            if args.constrain_json and case["mode"] == "json":
                                request["json_schema"] = SCHEMA
                            row = {"model": name, "case": case["name"], "mode": case["mode"],
                                   "repeat": repeat, "template_request": template, "request": request}
                            started = time.perf_counter()
                            try:
                                response = http(base, "/completion", request)
                                row.update(response=response, wall_ms=(time.perf_counter()-started)*1000)
                                row["complete"] = response.get("stop_type") == "eos" and not response.get("truncated", False)
                                row["assessment"] = assess(case, response.get("content", ""))
                            except Exception as error:
                                row.update(error=str(error), complete=False, wall_ms=(time.perf_counter()-started)*1000)
                            out.write(json.dumps(row, ensure_ascii=False) + "\n")
                            out.flush()
                            rows.append(row)
                            print(name, repeat, case["mode"], case["name"], row["complete"], round(row["wall_ms"]), flush=True)
                finally:
                    process.terminate()
                    try:
                        process.wait(timeout=15)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
            good = [r for r in rows if r["complete"]]
            js = [r for r in rows if r["mode"] == "json"]
            summary[name] = {"calls": len(rows), "complete": len(good),
                             "median_ms": statistics.median(r["wall_ms"] for r in good) if good else None,
                             "json_calls": len(js),
                             "valid_contract": sum(r.get("assessment", {}).get("valid_contract", False) for r in js),
                             "expected_decision": sum(r.get("assessment", {}).get("expected_decision", False) for r in js)}
            (args.output / "summary.json").write_text(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
