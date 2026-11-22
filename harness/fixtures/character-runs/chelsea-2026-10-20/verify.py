"""Verify saved replay provenance; semantic findings remain manual review."""
from pathlib import Path
import json

root = Path(__file__).resolve().parent

def load(name):
    return json.loads((root / f"{name}.json").read_text())

run = load("run")
assert run["database_read_only"] is True and run["publishes"] is False
assert run["subject"]["entity_id"] == 18 and run["subject"]["name"] == "Chelsea"
assert run["runtime_source"] == "49d27ca98bf280ec3718ba5d85c06dc8b381e569"
for character in ("journalist", "influencer", "scout", "insider", "analyst"):
    output = load(f"{character}-output")
    assert output["calls"], character
    expected = "Qwen3.5-4B-Q4_K_M" if character == "influencer" else "alibayram/smollm3"
    for call in output["calls"]:
        assert call["request"]["model"] == expected
        assert call["input_tokens"] > 0 and call["output_tokens"] > 0
        assert call["wall_ms"] > 0
        json.loads(call["response"])
    parts = load(f"{character}-input")
    assert "postgres://" not in json.dumps(parts) and "postgresql://" not in json.dumps(parts)

baseline = load("journalist-baseline")
assert len(baseline["sources"]) == 43 and not baseline["selected"]
assert sum(d["reason"] == "daily_package_exceeds_context_budget" for d in baseline["dispositions"]) == 42
journalist = load("journalist-input")
assert journalist["scope"] == "newest_bounded_debug_subset_not_publishable_daily_run"
assert len(journalist["reports"]) == 24 and len(journalist["prompt"].encode()) == 22681
assert {r["id"] for r in journalist["reports"]} <= {r["article_id"] for r in baseline["sources"]}

influencer = load("influencer-output")
assert json.loads(influencer["calls"][0]["response"]) == {"score": None, "headline": None, "body": None}
wire = json.loads(influencer["calls"][0]["request"]["request_body_raw"])
assert list(wire["json_schema"]["oneOf"][0]["properties"]) == ["score", "headline", "body"]

scout = load("scout-output")
assert len(scout["calls"]) == 2
assert "Output correction:" in scout["calls"][1]["request"].get("rendered_prompt", "") or "Output correction:" in scout["calls"][1]["raw_response_body"]
analyst = load("analyst-input")
assert analyst["scope"] == "fresh_replay_peer_outputs"
assert analyst["components"]["scout_body"] == scout["product"]["body"]
assert "influencer_body" not in analyst["components"]
insider = load("insider-output")
assert "counterparty was not co-mentioned" in insider["product"]["error"]
assert load("insider-input")["parts"]["reports"][0]["co_mentions"] == []
print("PASS: five stages, exact routes/schema order, retained corpus, fresh peer inputs, abstention and rejection receipts; no publication.")
