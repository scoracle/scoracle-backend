"""Read-only compact emotion bench; JSONL in/out, no database writes or routing.

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
LABELS = "admiration amusement anger annoyance approval caring confusion curiosity desire disappointment disapproval disgust embarrassment excitement fear gratitude grief joy love nervousness optimism pride realization relief remorse sadness surprise neutral".split()


def laya_questions():
    return {label: {"type": "noul", "instructions": (
        "The text contains no expressed emotion." if label == "neutral" else
        f"The text expresses {label}, including emotion attributed to a quoted speaker."),
        "criteria": ({"false": "Emotion is expressed", "true": "No emotion is expressed"}
                     if label == "neutral" else {"false": "Not expressed", "true": "Expressed"})}
            for label in LABELS}


def digest(value):
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


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
    inputs = [tokenizer(window["text"], truncation=False) for window in windows]
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
            windows[i].update(input_token_ids=inputs[i]["input_ids"],
                              raw_logits=dict(zip(metadata["labels"], raw, strict=True)),
                              scores={"emotion": dict(zip(metadata["labels"], values, strict=True))})
    return {"status": "measured", "windows": windows,
            "coverage": {"input_tokens": sum(len(row["input_ids"]) for row in inputs), "window_count": len(windows),
                         "max_tokens": args.max_tokens, "truncated": False,
                         "strategy": "production-100-word-1200-byte-windows"},
            "elapsed_ms": (time.perf_counter() - started) * 1000}


def laya_input(agent, text, questions):
    from harvest_laya_server import complete_question_tokens
    from laya.common import build_sequence, render_options, serialize_state
    if agent.tok.mask_token in text:
        raise ValueError("Laya would replace a literal mask token in the source")
    state_ids = agent.tok(serialize_state(text), add_special_tokens=False)["input_ids"]
    for key, question in questions.items():
        agent._check_question(key, question)
        internal = agent._to_internal(question)
        frame, _ = build_sequence(agent.tok, "", internal, max_len=agent.cfg["max_len"],
                                  head_max_len=agent.cfg["head_max_len"], state_ids=[])
        if frame != complete_question_tokens(agent.tok, internal, render_options(internal)):
            raise ValueError("Laya would truncate a question")
        if len(state_ids) + len(frame) > agent.cfg["max_len"]:
            raise ValueError("Laya would truncate the source")
    return state_ids


def measure_laya(item, agent):
    started = time.perf_counter()
    windows = source_windows(item["body"], item["windows"])
    tokens = 0
    for window in windows:
        text = window["text"]
        questions = laya_questions()
        state_ids = laya_input(agent, text, questions)
        response = agent.predict(text, questions)
        values = {label: response["answers"][label]["noul"] for label in LABELS}
        if set(response["answers"]) != set(LABELS) or not all(
                math.isfinite(value) and 0 <= value <= 1 for value in values.values()):
            raise ValueError("invalid Laya score vector")
        window.update(input_token_ids=state_ids, raw_logits=None, scores={"emotion": values},
                      request={"state": text, "questions": questions}, raw_response=response)
        tokens += len(state_ids)
    return {"status": "measured", "windows": windows,
            "coverage": {"input_tokens": tokens, "window_count": len(windows),
                         "max_tokens": agent.cfg["max_len"], "truncated": False,
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
    parser.add_argument("--laya-model-dir", type=Path)
    args = parser.parse_args()
    if min(args.threads, args.batch_size, args.max_tokens) < 1:
        parser.error("threads, batch size and token limit must be positive")
    items = [json.loads(line) for line in args.input.read_text().splitlines() if line.strip()]
    if not items or len({item["article_id"] for item in items}) != len(items):
        parser.error("nonempty input with unique canonical article IDs required")
    for item in items:
        if not isinstance(item["body"], str) or not item["body"].strip():
            parser.error("each item requires nonblank retained body text")
        source_windows(item["body"], item["windows"])
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
                        "runtime": {name: importlib.metadata.version(name) for name in
                                    ("torch", "transformers", "huggingface_hub")}}
            loaded = time.perf_counter()
            model = None
            try:
                if model_id == "laya":
                    import laya
                    if not args.laya_model_dir or not revision:
                        raise ValueError("Laya requires a local checkpoint and revision")
                    model = laya.load(str(args.laya_model_dir), device=args.device)
                    weights = args.laya_model_dir / "model.safetensors"
                    with weights.open("rb") as stream:
                        metadata["weights_sha256"] = hashlib.file_digest(stream, "sha256").hexdigest()
                    metadata.update(revision=revision, labels=LABELS, sdk_config=model.cfg,
                                    questions=laya_questions(), activation="sdk-noul-scores",
                                    parameter_count=sum(p.numel() for p in model.model.parameters()),
                                    dtype=str(model.dtype),
                                    runtime=dict(metadata["runtime"], laya=importlib.metadata.version("laya")))
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
                    if set(labels) != set(LABELS):
                        raise ValueError("bench requires the same 28 named emotion labels")
                    metadata.update(labels=labels, tokenizer_sha256=digest(tokenizer.backend_tokenizer.to_str()),
                                    parameter_count=sum(p.numel() for p in model.parameters()),
                                    dtype=str(next(model.parameters()).dtype), activation="sigmoid")
                metadata.update(label_schema_sha256=digest(json.dumps(sorted(LABELS))), calibrated=False,
                                load_ms=(time.perf_counter() - loaded) * 1000)
                load_error = None
            except Exception as error:
                load_error = f"{type(error).__name__}: {error}"
            for item in items:
                record = {"contract": CONTRACT, "article_id": item["article_id"],
                          "body_sha256": digest(item["body"]), "measured_at": time.time(),
                          "source": {key: value for key, value in item.items() if key not in ("body", "windows")},
                          "provenance": metadata}
                try:
                    if load_error:
                        raise ValueError(load_error)
                    record.update(measure_laya(item, model) if model_id == "laya" else
                                  measure(item, tokenizer, model, torch, args, metadata))
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
