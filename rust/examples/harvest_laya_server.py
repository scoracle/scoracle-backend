"""Local-only classification adapter for the Harvester worker and replay.

Requires a previously downloaded checkpoint directory. No article data leaves this
process. Run with HF_HUB_OFFLINE=1 after caching the encoder config, if required.
"""
import argparse
import hashlib
import importlib.metadata
import json
from pathlib import Path
import threading
import time


def complete_question_tokens(tokenizer, question, options):
    """Unshortened SDK frame, including the empty state's closing separator.

    Compare this with build_sequence before reporting complete input coverage:
    the SDK can truncate instructions and individual options as well as state.
    """
    def tokens(text):
        return tokenizer(text.replace(tokenizer.mask_token, " "),
                         add_special_tokens=False)["input_ids"]

    ids = [tokenizer.cls_token_id]
    ids += tokens(f"{question['t']} question: {question['ins']}")
    ids.append(tokenizer.sep_token_id)
    for option in options:
        ids.append(tokenizer.mask_token_id)
        ids += tokens(" " + option)
    return ids + [tokenizer.sep_token_id, tokenizer.sep_token_id]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model-dir", required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--device", choices=["cpu", "mps", "cuda"], default="cpu")
    parser.add_argument("--threads", type=int, default=2)
    parser.add_argument("--port", type=int, default=8019)
    args = parser.parse_args()
    import torch
    import laya
    from laya.common import build_sequence, render_options, serialize_state
    from fastapi import FastAPI, HTTPException
    import uvicorn

    torch.set_num_threads(args.threads)
    agent = laya.load(args.model_dir, device=args.device)
    lock = threading.Lock()
    app = FastAPI()
    model_dir = Path(args.model_dir)
    manifest = {}
    for path in model_dir.rglob("*"):
        if path.is_file() and ".cache" not in path.parts:
            with path.open("rb") as f:
                manifest[str(path.relative_to(model_dir))] = hashlib.file_digest(f, "sha256").hexdigest()
    provenance = {
        "model": "laya/" + model_dir.name,
        "revision": args.revision, "files": manifest,
        "runtime": {name: importlib.metadata.version(name) for name in ["laya", "torch", "transformers"]},
        "config": agent.cfg, "threads": args.threads,
        "adapter": "harvest-laya-v2",
    }

    @app.get("/health")
    def health():
        return {"ok": True, "device": str(agent.device), "provenance": provenance}

    @app.post("/v1/systemone")
    def decide(payload: dict):
        state, questions = payload.get("state"), payload.get("questions")
        if not isinstance(state, str) or not isinstance(questions, dict) or not questions:
            raise HTTPException(422, "state string and nonempty questions required")
        with lock:
            state_ids = agent.tok(serialize_state(state).replace(agent.tok.mask_token, " "), add_special_tokens=False)["input_ids"]
            coverage = {}
            for qid, question in questions.items():
                agent._check_question(qid, question)
                q = agent._to_internal(question)
                # Build the exact prompt without state to calculate its real remaining room.
                ids, _ = build_sequence(agent.tok, "", q, max_len=agent.cfg["max_len"],
                                        head_max_len=agent.cfg["head_max_len"], state_ids=[])
                expected = complete_question_tokens(agent.tok, q, render_options(q))
                if ids != expected:
                    raise HTTPException(422, f"{qid}: question or criteria would truncate")
                room = agent.cfg["max_len"] - len(ids)
                if len(state_ids) > room:
                    raise HTTPException(422, f"{qid}: input would truncate ({len(state_ids)} state tokens, {room} available)")
                coverage[qid] = {"state_tokens": len(state_ids), "question_tokens": len(ids),
                                 "available": room, "truncated": False}
            start = time.perf_counter()
            result = agent.predict(state, questions)
            result["provenance"] = dict(provenance, device=str(agent.device),
                                        dtype=str(agent.dtype), coverage=coverage,
                                        inference_ms=(time.perf_counter()-start)*1000)
            return result

    uvicorn.run(app, host="127.0.0.1", port=args.port, log_level="warning")


if __name__ == "__main__":
    main()
