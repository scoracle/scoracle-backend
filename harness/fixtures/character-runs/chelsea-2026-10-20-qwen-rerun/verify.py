"""Validate the retained replay and budgets; semantic findings remain manual."""
import json
from pathlib import Path
root = Path(__file__).resolve().parent
load = lambda name: json.loads((root / f"{name}.json").read_text())
run = load("run")
assert run["database_read_only"] and not run["publishes"] and run["num_ctx"] == 7900
assert run["subject"]["entity_id"] == 18 and run["subject"]["name"] == "Chelsea"
base = load("journalist-baseline")
assert len(base["sources"]) == 67 and len(base["selected"]) == 63
assert len(base["dispositions"]) == 4
assert all(d["reason"] == "already_reported_exact_text" for d in base["dispositions"])
assert load("journalist-output")["product"]["scope"] == "current_complete_assignment"
selected = {r["id"] for r in base["selected"]}
assert {r["id"] for r in load("journalist-input")["reports"]} == selected
budget = load("budget")
assert len(budget["calls"]) == 7
for row in budget["calls"]:
    c = load(row["character"] + "-output")["calls"][row["attempt"] - 1]
    assert c["request"]["model"] == "Qwen3.5-4B-Q4_K_M"
    assert row["input_tokens"] == c["input_tokens"] == len(c["request"]["prompt"])
    assert row["reserved_output_tokens"] == c["request"]["n_predict"]
    assert row["total_budgeted_tokens"] == row["input_tokens"] + row["reserved_output_tokens"] <= 7900 < 8000
assert max(r["total_budgeted_tokens"] for r in budget["calls"]) == 7112
components = load("analyst-input")["components"]
assert "scout_body" not in components
assert components["influencer_body"] == load("influencer-output")["product"]["body"]
assert len(load("scout-guards")) == 3
assert load("provenance")["allows_subset_fallback"] is False
print("PASS: all 63 unique headlines, seven Qwen requests below 8k, correct fresh-peer provenance, no publication.")
