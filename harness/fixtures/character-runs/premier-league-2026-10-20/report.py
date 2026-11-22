"""Verify complete private replay receipts and print an aggregate JSON report."""
from collections import Counter
from pathlib import Path
import hashlib
import json
import statistics
import sys

run = Path(sys.argv[1])
root = run / "results"
assert (Path(__file__).with_name("replay.rs")).read_bytes() == (run / "driver.rs").read_bytes()
roster = json.loads((root / "roster.json").read_text())
summary = json.loads((root / "summary.json").read_text())
events = [json.loads(line) for line in (root / "events.jsonl").read_text().splitlines()]
stages = ["journalist", "influencer", "scout", "insider", "analyst", "oracle"]
expected = {(s["entity_type"], s["entity_id"], stage) for s in roster for stage in stages}
actual = {(e["entity_type"], e["entity_id"], e["stage"]) for e in events}
assert len(events) == len(actual) == len(expected) and actual == expected
assert summary["read_only"] and not summary["publishes"]
restored = json.loads((run / "monitor-restoration.json").read_text())
assert restored["exit_code"] == 0 and restored["stdout"].strip() == "active"

by_stage = {}
errors = Counter()
examples = {}
max_budget = native_responses = failed_responses = unknown_attempts = 0
for stage in stages:
    outcomes = Counter()
    latencies = []
    invocations = attempts = responses = input_tokens = output_tokens = 0
    for event in (e for e in events if e["stage"] == stage):
        receipt = json.loads((root / f"{event['entity_type']}-{event['entity_id']}" / f"{stage}.json").read_text())
        product = receipt.get("product", {})
        status = event["status"]
        if status == "generated" and (
            (stage == "journalist" and not product.get("narratives"))
            or (stage == "scout" and (product.get("abstained") or not product.get("body")))
        ):
            status = "abstained"
        event["outcome"] = status
        outcomes[status] += 1
        invocations += bool(receipt["calls"])
        if "error" in receipt:
            reason = receipt["error"].split(": ", 1)[-1]
            errors[(stage, reason)] += 1
            examples.setdefault((stage, reason), f"{event['entity_type']}-{event['entity_id']}")
        for call in receipt["calls"]:
            attempts += 1
            try:
                raw = json.loads(call.get("raw_response", "{}"))
            except json.JSONDecodeError:
                raw = {}
            inp = call.get("input_tokens", raw.get("tokens_evaluated", 0))
            out = call.get("output_tokens", raw.get("tokens_predicted", 0))
            if inp:
                responses += 1
                native_responses += 1
                input_tokens += inp
                output_tokens += out
                max_budget = max(max_budget, inp + call["reserved_output"])
                assert inp + call["reserved_output"] <= 10000
                failed_responses += bool(call.get("error"))
                wall_ms = call.get("provider_wall_ms")
                if wall_ms is None and raw.get("timings"):
                    wall_ms = raw["timings"].get("prompt_ms", 0) + raw["timings"].get("predicted_ms", 0)
                if wall_ms is not None:
                    latencies.append(wall_ms / 1000)
            elif not call.get("context_exceeded"):
                unknown_attempts += 1
    by_stage[stage] = {
        "outcomes": dict(outcomes), "invocations": invocations, "attempts": attempts, "native_responses": responses,
        "input_tokens": input_tokens, "output_tokens": output_tokens,
        "provider_seconds": round(sum(latencies), 2),
        "mean_provider_seconds": round(statistics.mean(latencies), 2) if latencies else None,
        "median_provider_seconds": round(statistics.median(latencies), 2) if latencies else None,
    }
trace = [json.loads(line) for line in (run / "slots.jsonl").read_text().splitlines()]
assert sum(s["native_responses"] for s in by_stage.values()) == summary["model_calls"]
assert len(events) == summary["stage_results"]
print(json.dumps({
    "complete": True,
    "driver_sha256": hashlib.sha256((run / "driver.rs").read_bytes()).hexdigest(),
    "clubs": sum(s["entity_type"] == "team" for s in roster),
    "players": sum(s["entity_type"] == "player" for s in roster),
    "timing": summary,
    "native_responses": native_responses,
    "incomplete_or_failed_native_responses": failed_responses,
    "unknown_transport_attempts": unknown_attempts,
    "max_admitted_tokens_including_output": max_budget,
    "peak_active_slots": max(len(s.get("active", [])) for s in trace),
    "outcomes": dict(Counter(e["outcome"] for e in events)),
    "by_stage": by_stage,
    "by_entity_type": {kind: dict(Counter(e["outcome"] for e in events if e["entity_type"] == kind)) for kind in ["team", "player"]},
    "top_errors": [{"stage": s, "reason": reason, "count": n, "example": examples[(s, reason)]} for (s, reason), n in errors.most_common(12)],
    "qualification": "Read-only creator/parser replay; excludes queue publication, write-time normalization, durable retries, unchanged-product reuse and factual accuracy grading.",
}, indent=2))
