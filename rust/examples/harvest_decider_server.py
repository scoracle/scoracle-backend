"""Loopback-only Decider adapter; serves the same Rust replay contract as Laya.

Use a pinned local checkpoint and HF_HUB_OFFLINE=1. No generation or remote API.
"""
import argparse
import hashlib
import importlib.metadata
import json
from pathlib import Path
import threading
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--model-dir", required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--device", choices=["cpu", "mps"], default="cpu")
    parser.add_argument("--threads", type=int, default=2)
    parser.add_argument("--port", type=int, default=8020)
    args = parser.parse_args()
    import torch
    from decider.infer import Decider
    from decider.systemone import render_state
    from fastapi import FastAPI, HTTPException
    import uvicorn

    torch.set_num_threads(args.threads)
    agent = Decider(args.model_dir, device=args.device, use_graphs=False)
    lock = threading.Lock()
    app = FastAPI()
    model_dir = Path(args.model_dir)
    manifest = {}
    for path in model_dir.iterdir():
        if path.is_file():
            with path.open("rb") as f:
                manifest[path.name] = hashlib.file_digest(f, "sha256").hexdigest()
    provenance = {
        "model": "Mapika/" + model_dir.name, "revision": args.revision, "files": manifest,
        "runtime": {name: importlib.metadata.version(name) for name in ["decider-ai", "torch", "transformers"]},
        "config": json.loads((model_dir / "decider_config.json").read_text()),
        "threads": args.threads, "device": args.device,
        "dtype": str(next(agent.m.parameters()).dtype), "adapter": "harvest-decider-v1",
    }

    @app.get("/health")
    def health():
        return {"ok": True, "device": args.device, "provenance": provenance}

    @app.post("/v1/systemone")
    def decide(payload: dict):
        state, questions = payload.get("state"), payload.get("questions")
        if not isinstance(state, str) or not isinstance(questions, dict) or not 1 <= len(questions) <= 16:
            raise HTTPException(422, "state string and 1..16 questions required")
        with lock:
            state_tokens = len(agent.m.tok.encode(render_state(state), add_special_tokens=False))
            if state_tokens > 2048:
                raise HTTPException(422, "opening exceeds trial's 2048-token state budget")
            start = time.perf_counter()
            result = agent.system_one(state, questions, independent=True, max_state_tokens=2048, max_fwd_tokens=4096)
            if args.device == "mps":
                torch.mps.synchronize()
            result["provenance"] = dict(provenance,
                coverage={qid: {"state_tokens": state_tokens, "available": 2048, "truncated": False} for qid in questions},
                inference_ms=(time.perf_counter()-start)*1000)
            return result

    uvicorn.run(app, host="127.0.0.1", port=args.port, log_level="warning")


if __name__ == "__main__":
    main()
