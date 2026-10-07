"""Read-only classifier bench; JSONL in/out, no database writes or routing.

python classifier_replay.py INPUT.jsonl OUTPUT.jsonl --model MODEL[@REVISION]
Requires the existing torch, transformers and huggingface_hub environment.
Source windows are inspected text, not explanations or attributed emotion claims.
"""
import argparse
import hashlib
import importlib.metadata
import json
import math
import platform
import time
from pathlib import Path

CONTRACT = "classifier-emotion-windows-v2"
SCHEMA_PATH = Path(__file__).resolve().parents[1] / "fixtures/classifier/vector-schema-v1.json"


def digest(value):
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def head_labels(schema, families):
    if not families or len(set(families)) != len(families):
        raise ValueError("unique vector families required")
    vectors = [schema["vectors"][family] for family in families]
    if len({vector["scope"] for vector in vectors}) != 1:
        raise ValueError("one head cannot mix document and target input scopes")
    return {label if families == ["emotion"] else f"{family}.{label}": (family, label)
            for family in families for label in schema["vectors"][family]["labels"]}


def validate_head(labels, expected):
    if len(labels) != len(set(labels)) or set(labels) != set(expected):
        raise ValueError("checkpoint does not provide the requested trained labels; new vectors need a trained head")


def model_text(item, text, scope):
    if scope == "document_window":
        return text
    target = item.get("target", {})
    if (not all(isinstance(target.get(key), str) and target[key].strip()
                for key in ("name", "sport", "entity_type"))
            or type(target.get("entity_id")) is not int or target["entity_id"] <= 0
            or target not in item.get("query_entities", [])):
        raise ValueError("target-conditioned head requires a canonical query target")
    return "TARGET (identity only, not evidence): " + json.dumps(target, ensure_ascii=False) + "\nSOURCE:\n" + text


def source_windows(text, windows):
    """Verify unchanged production windows cover every non-whitespace byte."""
    body = text.encode("utf-8")
    end_seen = 0
    for window in windows:
        start, end = window["start"], window["end"]
        if (not end_seen <= start < end <= len(body)
                or body[end_seen:start].decode().strip()
                or body[start:end].decode() != window["text"]):
            raise ValueError("invalid source window or non-whitespace coverage gap")
        end_seen = end
    if not windows or body[end_seen:].decode().strip():
        raise ValueError("windows did not cover the complete source")
    return [dict(window) for window in windows]


def measure(item, tokenizer, model, torch, args, metadata):
    started = time.perf_counter()
    windows = source_windows(item["body"], item["windows"])
    texts = [model_text(item, window["text"], metadata["input_scope"]) for window in windows]
    inputs = [tokenizer(text, truncation=False) for text in texts]
    if any(len(row["input_ids"]) > args.max_tokens for row in inputs):
        raise ValueError("source window exceeds the declared token limit")
    for offset in range(0, len(inputs), args.batch_size):
        batch = tokenizer.pad(inputs[offset:offset + args.batch_size], padding=True,
                              return_tensors="pt").to(args.device)
        with torch.inference_mode():
            logits = model(**batch).logits.float()
            scores = logits.sigmoid()
        for i, (raw, values) in enumerate(zip(logits.cpu().tolist(), scores.cpu().tolist(), strict=True), offset):
            if not all(math.isfinite(value) for value in raw + values):
                raise ValueError("nonfinite model output")
            grouped = {}
            for label, value in zip(metadata["labels"], values, strict=True):
                family, name = metadata["label_mapping"][label]
                grouped.setdefault(family, {})[name] = value
            windows[i].update(model_text=texts[i], input_token_ids=inputs[i]["input_ids"],
                              raw_logits=dict(zip(metadata["labels"], raw, strict=True)), scores=grouped)
    return {"status": "measured", "windows": windows,
            "coverage": {"input_tokens": sum(len(row["input_ids"]) for row in inputs), "window_count": len(windows),
                         "max_tokens": args.max_tokens, "truncated": False,
                         "strategy": "production-100-word-1200-byte-windows"},
            "elapsed_ms": (time.perf_counter() - started) * 1000}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--model", action="append", required=True)
    parser.add_argument("--device", choices=["cpu", "cuda", "mps"], default="cpu")
    parser.add_argument("--threads", type=int, default=2)
    parser.add_argument("--batch-size", type=int, default=8)
    parser.add_argument("--max-tokens", type=int, default=256)
    parser.add_argument("--schema", type=Path, default=SCHEMA_PATH)
    parser.add_argument("--vector-family", action="append",
                        help="Trained head family; repeat for a head sharing one input scope. Default: emotion.")
    args = parser.parse_args()
    if min(args.threads, args.batch_size, args.max_tokens) < 1:
        parser.error("threads, batch size and token limit must be positive")
    schema = json.loads(args.schema.read_text())
    families = args.vector_family or ["emotion"]
    try:
        expected = head_labels(schema, families)
    except (KeyError, ValueError) as error:
        parser.error(str(error))
    if any(candidate.partition("@")[0] == "laya" for candidate in args.model):
        parser.error("Laya is excluded from the launch bank; archived Phase 1 scripts reproduce its comparison")
    scope = schema["vectors"][families[0]]["scope"]
    items = [json.loads(line) for line in args.input.read_text().splitlines() if line.strip()]
    if not items or len({item["article_id"] for item in items}) != len(items):
        parser.error("nonempty input with unique canonical article IDs required")
    for item in items:
        if not isinstance(item["body"], str) or not item["body"].strip():
            parser.error("each item requires nonblank retained body text")
        source_windows(item["body"], item["windows"])
        model_text(item, item["windows"][0]["text"], scope)
    import torch
    from huggingface_hub import HfApi
    from transformers import AutoModelForSequenceClassification, AutoTokenizer

    torch.set_num_threads(args.threads)
    failures = 0
    with args.output.open("x") as output:
        for candidate in args.model:
            model_id, _, revision = candidate.partition("@")
            metadata = {"model": model_id, "device": args.device, "threads": args.threads,
                        "batch_size": args.batch_size, "platform": platform.platform(),
                        "vector_schema_version": schema["version"],
                        "vector_schema_sha256": digest(json.dumps(schema, sort_keys=True)),
                        "vector_families": families, "input_scope": scope, "label_mapping": expected,
                        "runtime": {name: importlib.metadata.version(name) for name in
                                    ("torch", "transformers", "huggingface_hub")}}
            loaded = time.perf_counter()
            model = None
            try:
                if Path(model_id).is_dir():
                    if not revision:
                        raise ValueError("local trained checkpoint requires a declared revision")
                    with (Path(model_id) / "model.safetensors").open("rb") as weights:
                        metadata["weights_sha256"] = hashlib.file_digest(weights, "sha256").hexdigest()
                else:
                    revision = HfApi().model_info(model_id, revision=revision or "main").sha
                metadata["revision"] = revision
                tokenizer = AutoTokenizer.from_pretrained(model_id, revision=revision, trust_remote_code=False)
                model = AutoModelForSequenceClassification.from_pretrained(
                    model_id, revision=revision, trust_remote_code=False, use_safetensors=True,
                    attn_implementation="eager").to(args.device).eval()
                if model.config.problem_type != "multi_label_classification":
                    raise ValueError("bench requires an explicitly declared multi-label sigmoid head")
                if args.max_tokens > min(tokenizer.model_max_length, model.config.max_position_embeddings):
                    raise ValueError("requested window exceeds model/tokenizer capacity")
                labels = [model.config.id2label[i] for i in range(model.config.num_labels)]
                validate_head(labels, expected)
                metadata.update(labels=labels, tokenizer_sha256=digest(tokenizer.backend_tokenizer.to_str()),
                                parameter_count=sum(p.numel() for p in model.parameters()),
                                dtype=str(next(model.parameters()).dtype), activation="sigmoid")
                metadata.update(label_schema_sha256=digest(json.dumps(sorted(expected))), calibrated=False,
                                load_ms=(time.perf_counter() - loaded) * 1000)
                load_error = None
            except Exception as error:
                load_error = f"{type(error).__name__}: {error}"
            for item in items:
                record = {"contract": CONTRACT if families == ["emotion"] else "classifier-spectrum-windows-v1",
                          "article_id": item["article_id"],
                          "body_sha256": digest(item["body"]), "measured_at": time.time(),
                          "source": {key: value for key, value in item.items() if key not in ("body", "windows")},
                          "provenance": metadata}
                try:
                    if load_error:
                        raise ValueError(load_error)
                    record.update(measure(item, tokenizer, model, torch, args, metadata))
                except Exception as error:
                    record.update(status="error", error=f"{type(error).__name__}: {error}")
                    failures += 1
                output.write(json.dumps(record, ensure_ascii=False, allow_nan=False) + "\n")
                output.flush()
                print(f"{model_id} article={item['article_id']} status={record['status']} "
                      f"ms={record.get('elapsed_ms', 0):.1f}", flush=True)
            if model is not None:
                del model
                if args.device == "cuda":
                    torch.cuda.empty_cache()
    raise SystemExit(1 if failures else 0)


if __name__ == "__main__":
    main()
