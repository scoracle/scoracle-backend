#!/usr/bin/env python3
"""Plain evidence-reading comparison; no database, publication, schema or judge.

python3 rust/examples/plain_context_compare.py --check
python3 rust/examples/plain_context_compare.py OUTPUT.jsonl --base-url http://localhost:11434
"""
import argparse
import json
import time
from pathlib import Path

from influencer_tool_compare import chat

FIXTURES = Path(__file__).resolve().parents[1] / "fixtures/influencer/plain-context-3b"
MODELS = ["alibayram/smollm3:latest", "granite4.2:3b", "ministral-3:3b"]
SYSTEM = (
    "Answer the question using only the supplied reports and history. "
    "Keep speakers, dates and qualifications attached to their statements. "
    "If an answer about the target is not established, say so. Give a short answer."
)


def request(packet, model):
    return {
        "model": model,
        "messages": [
            {"role": "system", "content": SYSTEM},
            {"role": "user", "content": json.dumps(packet, indent=2)},
        ],
        "stream": False,
        "think": False,
        "options": {"temperature": 0, "top_p": 1, "seed": 42, "num_ctx": 4096, "num_predict": 2048},
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", nargs="?", type=Path)
    parser.add_argument("--base-url", default="http://localhost:11434")
    parser.add_argument("--models", nargs="+", default=MODELS)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    base = json.loads((FIXTURES / "packet.json").read_text())
    cases = json.loads((FIXTURES / "cases.json").read_text())
    if args.check:
        for case in cases:
            packet = {**base, **{k: case[k] for k in ("reports", "history") if k in case}}
            a, b = (request(packet, model) for model in MODELS[:2])
            assert a.pop("model") != b.pop("model") and a == b
            assert json.loads(a["messages"][1]["content"]) == packet
            assert set(packet) == {"question", "target", "reporting_period", "reports", "history"}
            assert not any(k in a for k in ("format", "tools"))
        print("Five packets checked: identical model inputs; review criteria never sent.")
        return
    if args.output is None:
        parser.error("output is required unless --check is used")
    with args.output.open("x") as out:
        for model in args.models:
            for case in cases:
                packet = {**base, **{k: case[k] for k in ("reports", "history") if k in case}}
                req = request(packet, model)
                row = {"model": model, "case": case["name"], "synthetic": True,
                       "review": case["review"], "request_body": json.dumps(req, separators=(",", ":"))}
                started = time.perf_counter()
                try:
                    row.update(chat(args.base_url.rstrip("/"), req))
                    response = row["response"]
                    row["complete"] = (response.get("done") is True
                        and response.get("done_reason") == "stop"
                        and bool(response.get("message", {}).get("content", "").strip()))
                except Exception as error:
                    row.update(error=str(error), complete=False,
                               wall_ms=(time.perf_counter() - started) * 1000)
                    if hasattr(error, "read"):
                        row["raw_response_body"] = error.read().decode()
                out.write(json.dumps(row) + "\n")
                out.flush()
                print(f"{model} {case['name']} complete={row['complete']}", flush=True)


if __name__ == "__main__":
    main()
