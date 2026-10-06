#!/usr/bin/env python3
"""Compare acquisition routes using exact evidence from a retained tool replay.

This measures local inference, not live DB latency or source eligibility. Both
routes receive the same evidence. Native dispatch must first request read_source
with empty arguments. No retry, output repair, publication or database mutation.

python3 examples/influencer_tool_compare.py --output /private/tmp/compare.jsonl \
    --temperature 1 --repeats 3 /private/tmp/granite-used-source-replay.json \
    /private/tmp/granite-used-source-replay-70146.json
Validate the output with `cargo run --example influencer_replay -- --validate FILE`.
Exact requests and replies contain publisher evidence; keep the output private.
"""
import argparse
import copy
import hashlib
import json
import os
import time
import urllib.request
from pathlib import Path


def chat(base_url, request):
    wire = json.dumps(request, separators=(",", ":"))
    http = urllib.request.Request(
        base_url + "/api/chat", wire.encode(), {"Content-Type": "application/json"}
    )
    started = time.perf_counter()
    with urllib.request.urlopen(http, timeout=180) as response:
        raw = response.read().decode()
    return {
        "request_body": wire, "raw_response_body": raw,
        "response": json.loads(raw), "wall_ms": (time.perf_counter() - started) * 1000,
    }


def requests(capture, temperature, seed):
    first = json.loads(capture["first"]["request_body"])
    final = json.loads(capture["second"]["request_body"])
    evidence = json.dumps(capture["tool_result"], separators=(",", ":"))
    assert final["messages"][:2] == first["messages"]
    assert json.loads(final["messages"][-1]["content"]) == capture["tool_result"]
    for request in (first, final):
        request["options"].update(temperature=temperature, top_p=0.95, seed=seed)
    prepared = copy.deepcopy(final)
    prepared["messages"] = copy.deepcopy(first["messages"])
    task = prepared["messages"][0]["content"]
    read_step = "Read the assigned source with read_source, then describe"
    assert task.startswith(read_step), "capture does not use the pilot task"
    prepared["messages"][0]["content"] = task.replace(read_step, "Describe", 1)
    prepared["messages"][1]["content"] += "\nSource result:\n" + evidence
    assert not prepared.get("tools") and not final.get("tools")
    assert prepared["format"] == final["format"]
    assert prepared["options"] == final["options"] == first["options"]
    return first, final, prepared, evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("captures", nargs="+", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--temperature", required=True, type=float)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--base-url", default="http://127.0.0.1:11434")
    args = parser.parse_args()
    if args.repeats < 1:
        parser.error("repeats must be positive")
    # Fail on an existing output; retained failed responses must not be overwritten.
    with os.fdopen(os.open(args.output, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "w") as out:
        for repeat in range(args.repeats):
            for path in args.captures:
                capture = json.loads(path.read_text())
                for route in (["native", "prepared"] if repeat % 2 == 0 else ["prepared", "native"]):
                    first, final, prepared, evidence = requests(capture, args.temperature, 42 + repeat)
                    calls = []
                    error = None
                    reads = 0
                    started = time.perf_counter()
                    if route == "native":
                        calls.append(chat(args.base_url, first))
                        response = calls[-1]["response"]
                        message = response.get("message", {})
                        tool_calls = message.get("tool_calls", [])
                        if (
                            response.get("done") is not True
                            or response.get("done_reason") != "stop"
                            or message.get("role") != "assistant"
                            or len(tool_calls) != 1
                            or tool_calls[0].get("function", {}).get("name") != "read_source"
                            or tool_calls[0]["function"].get("arguments") != {}
                        ):
                            error = "invalid_first_call"
                        else:
                            reads = 1
                            final["messages"] = first["messages"] + [message, {"role": "tool", "tool_name": "read_source", "content": evidence}]
                            calls.append(chat(args.base_url, final))
                    else:
                        reads = 1
                        calls.append(chat(args.base_url, prepared))
                    row = {
                        "key": f"{path.stem}/{route}/{repeat}", "route": route,
                        "seed": 42 + repeat, "temperature": args.temperature,
                        "input_capture": str(path), "evidence_mode": "retained replay; no live DB read",
                        "evidence_sha256": hashlib.sha256(evidence.encode()).hexdigest(),
                        "evidence_deliveries": reads, "error": error, "calls": calls,
                        "response": calls[-1]["response"],
                        "wall_ms": (time.perf_counter() - started) * 1000,
                        "prompt_tokens": sum(c["response"].get("prompt_eval_count", 0) for c in calls),
                        "output_tokens": sum(c["response"].get("eval_count", 0) for c in calls),
                    }
                    out.write(json.dumps(row) + "\n")
                    out.flush()
                    print(json.dumps({k: row[k] for k in ("key", "error", "wall_ms", "prompt_tokens", "output_tokens")}), flush=True)


if __name__ == "__main__":
    main()
