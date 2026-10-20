"""Check the retained token budget and complete-corpus replay, without a server."""
import json
from pathlib import Path
root = Path(__file__).resolve().parent
load = lambda name: json.loads((root / f"{name}.json").read_text())
assert load("run")["database_read_only"] and not load("run")["publishes"]
base = load("journalist-baseline")
assert len(base["sources"]) == 61 and len(base["selected"]) == 58
assert len(base["dispositions"]) == 3
assert all(d["reason"] == "already_reported_exact_text" for d in base["dispositions"])
assert load("journalist-output")["product"]["scope"] == "current_complete_assignment"
budget = load("input-budget")
assert budget["effective_request_cap"] == 7900
for row in budget["calls"]:
    call = load(row["character"] + "-output")["calls"][row["attempt"] - 1]
    assert row["input_tokens"] == len(call["request"]["prompt"])
    assert row["input_tokens"] == call["input_tokens"]
    assert row["reserved_output_tokens"] == call["request"]["n_predict"]
    assert row["total_budgeted_tokens"] == row["input_tokens"] + row["reserved_output_tokens"]
    assert row["total_budgeted_tokens"] <= 7900 < 8000
    assert call["request"]["model"] == "Qwen3.5-4B-Q4_K_M"
assert max(r["total_budgeted_tokens"] for r in budget["calls"]) == 6736
print("PASS: all 58 unique Chelsea headlines retained; seven Qwen calls budgeted below 8k, including output.")

memory = load("memory-budget")["twenty_history_stress"]
assert memory["synthetic"] and memory["memories"] == 20 and memory["retained_memory_bytes"] <= 6000
assert memory["input_tokens"] + memory["reserved_output_tokens"] == memory["total_tokens"] == 7656
assert memory["total_tokens"] <= 7900 < 8000

packet = load("memory-stress-packet")
assert len(packet["fresh"]) == 58 and len(packet["memories"]) == 20
assert all(set(m) == {"headline", "publisher", "published_at"} for m in packet["memories"])
assert load("memory-stress-provenance")["synthetic"]
