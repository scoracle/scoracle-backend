#!/usr/bin/env python3
"""Offline native decision replay; no database access or production routing changes.

python3 examples/relevance_calibration.py ENDPOINT BANK.json CASES.jsonl RESULTS.jsonl [CONCURRENCY]
First snapshot subject metadata with the relevance_meta Rust example. Cases require id,
group (source article), kind, sport, subject, subject_reference, meta, body and four boolean
expected labels. Keep publisher text private. Run --self-test for the scoring checks.
"""
import concurrent.futures
import hashlib
import json
import math
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

KEYS = {"emotion", "movement", "availability", "subject"}


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def decode(response, model):
    if response.get("model") != model or set(response.get("answers", {})) != KEYS:
        raise ValueError("model changed or incomplete answers")
    scores = {}
    for key, answer in response["answers"].items():
        value = answer.get("noul")
        if (answer.get("type") != "noul" or type(value) not in (float, int)
                or not math.isfinite(value) or not 0 <= value <= 1):
            raise ValueError(f"invalid {key} probability")
        scores[key] = value
    return scores


def confusion(pairs):
    pairs = list(pairs)
    tp = sum(predicted and expected for predicted, expected in pairs)
    fp = sum(predicted and not expected for predicted, expected in pairs)
    fn = sum(not predicted and expected for predicted, expected in pairs)
    tn = len(pairs) - tp - fp - fn
    return dict(tp=tp, fp=fp, fn=fn, tn=tn, errors=fp + fn,
                precision=tp / (tp + fp) if tp + fp else None,
                recall=tp / (tp + fn) if tp + fn else None)


def tune(rows):
    """Equal cost for each false admission and miss; ties stay closest to 50%."""
    if not rows or any("error" in row for row in rows):
        raise ValueError("cannot tune an empty or failed replay")
    return {key: min((i / 100 for i in range(10, 95, 5)), key=lambda cutoff: (
        sum((row["scores"][key] >= cutoff) != row["expected"][key] for row in rows),
        abs(round(cutoff * 100) - 50), cutoff)) for key in sorted(KEYS)}


def admitted(values):
    return values["subject"] and any(values[key] for key in KEYS - {"subject"})


def request(bank, case):
    questions = {key: {**question, "instructions": (
        "Use subject metadata only to identify the entity. Judge evidence from the article only. "
        + question["instructions"].replace("{subject}", case["subject_reference"]))}
        for key, question in bank["questions"].items()}
    return {"state": {"subject_meta": case["meta"],
                      "article": {"sport": case["sport"], "body": case["body"]}},
            "questions": questions}


def metrics(rows, cutoffs):
    if not rows or set(cutoffs) != KEYS or any("error" in row for row in rows):
        raise ValueError("metrics require a complete successful replay and four cutoffs")
    decisions = [{key: row["scores"][key] >= cutoffs[key] for key in KEYS} for row in rows]
    per_topic = {key: confusion((decision[key], row["expected"][key])
                               for row, decision in zip(rows, decisions)) for key in sorted(KEYS)}
    per_character = {character: confusion(
        (decision["subject"] and decision[topic], row["expected"]["subject"] and row["expected"][topic])
        for row, decision in zip(rows, decisions)) for character, topic in (
            ("journalist", "subject"), ("influencer", "emotion"),
            ("insider", "movement"), ("scout", "availability"))}
    return dict(cases=len(rows), source_groups=len({row["group"] for row in rows}),
                labels=len(rows) * len(KEYS), per_topic=per_topic, per_character=per_character,
                label_errors=sum(counts["errors"] for counts in per_topic.values()),
                brier=sum((row["scores"][key] - row["expected"][key]) ** 2
                          for row in rows for key in KEYS) / (len(rows) * len(KEYS)),
                gate=confusion((admitted(decision), admitted(row["expected"]))
                               for row, decision in zip(rows, decisions)))


def replay(endpoint, bank_path, cases_path, results_path, concurrency):
    bank = json.loads(Path(bank_path).read_text())
    cases = [json.loads(line) for line in Path(cases_path).read_text().splitlines() if line.strip()]
    if (concurrency < 1 or set(bank["questions"]) != KEYS or not cases
            or len({case["id"] for case in cases}) != len(cases)):
        raise ValueError("positive concurrency, four questions and unique nonempty cases required")
    for case in cases:
        if (set(case["expected"]) != KEYS or any(type(v) is not bool for v in case["expected"].values())
                or any(not isinstance(case[k], str) or not case[k].strip()
                       for k in ("id", "group", "kind", "sport", "subject", "subject_reference", "body"))
                or case["meta"]["identity"]["name"] != case["subject"]
                or case["meta"]["identity"]["sport"] != case["sport"]):
            raise ValueError(f"invalid case {case.get('id')}")
    endpoint = endpoint.rstrip("/")
    with urllib.request.urlopen(endpoint + "/v1/models", timeout=30) as response:
        inventory = json.load(response)
    if len(inventory["data"]) != 1:
        raise ValueError("one fingerprinted model required")
    model = inventory["data"][0]["id"]
    name, fingerprint = model.rsplit("@sha256:", 1)
    if not name or len(fingerprint) != 64 or any(c not in "0123456789abcdef" for c in fingerprint):
        raise ValueError("invalid model fingerprint")

    def one(case):
        payload = request(bank, case)
        result = {key: case[key] for key in ("id", "group", "kind", "subject", "expected")}
        result.update(model=model, bank_version=bank["version"], request_sha256=digest(payload),
                      context_version="meta.rs-v1", meta_sha256=digest(case["meta"]),
                      body_sha256=hashlib.sha256(case["body"].encode()).hexdigest())
        started = time.monotonic()
        raw = ""
        try:
            # Match production's serde_json key order; state serialization affects native scores.
            http_request = urllib.request.Request(endpoint + "/v1/systemone", json.dumps(payload, sort_keys=True).encode(),
                                                  {"Content-Type": "application/json"})
            with urllib.request.urlopen(http_request, timeout=120) as response:
                raw = response.read().decode()
            result["response"] = json.loads(raw)
            result["scores"] = decode(result["response"], model)
        except Exception as error:
            if isinstance(error, urllib.error.HTTPError):
                raw = error.read().decode(errors="replace")
            result.update(error=str(error), raw_response=raw)
        result["seconds"] = time.monotonic() - started
        return result

    started = time.monotonic()
    failures = 0
    # Exclusive creation prevents overwriting an earlier benchmark receipt.
    with Path(results_path).open("x") as output, concurrent.futures.ThreadPoolExecutor(concurrency) as pool:
        for result in pool.map(one, cases):
            failures += "error" in result
            output.write(json.dumps(result, sort_keys=True) + "\n")
            output.flush()
    seconds = time.monotonic() - started
    print(json.dumps(dict(model=model, bank=bank["version"], cases=len(cases), failures=failures,
                          concurrency=concurrency, seconds=seconds,
                          successful_requests_per_second=(len(cases) - failures) / seconds,
                          inventory=inventory)), file=sys.stderr)
    return int(failures > 0)


def self_test():
    model = "test@sha256:" + "0" * 64
    response = {"model": model, "answers": {key: {"type": "noul", "noul": .8} for key in KEYS}}
    assert set(decode(response, model)) == KEYS
    for wrong in [None, True, "0.8", -0.1, 1.1, float("nan")]:
        invalid = json.loads(json.dumps(response))
        invalid["answers"]["subject"]["noul"] = wrong
        try:
            decode(invalid, model)
        except ValueError:
            pass
        else:
            raise AssertionError(f"accepted {wrong}")
    for invalid in [{"model": model, "answers": {}}, {**response, "model": "changed"}]:
        try:
            decode(invalid, model)
        except ValueError:
            pass
        else:
            raise AssertionError("accepted incomplete or changed model")
    rows = [dict(group=str(i), scores=dict.fromkeys(KEYS, score), expected=dict.fromkeys(KEYS, expected))
            for i, (score, expected) in enumerate([(.2, False), (.4, True), (.8, True)])]
    cutoffs = tune(rows)
    assert all(value == .4 for value in cutoffs.values())
    assert metrics(rows, cutoffs)["label_errors"] == 0
    assert metrics(rows, dict.fromkeys(KEYS, .5))["gate"]["fn"] == 1
    assert not admitted(dict(emotion=True, movement=True, availability=True, subject=False))
    cutoffs = dict(subject=.55, emotion=.15, movement=.8, availability=.9)
    neutral = dict(subject=True, emotion=False, movement=False, availability=False)
    routes = metrics([
        dict(group="neutral", scores=dict(subject=.55, emotion=0, movement=0, availability=0),
             expected=neutral),
        dict(group="unrelated", scores=dict(subject=.549, emotion=1, movement=1, availability=1),
             expected=dict.fromkeys(KEYS, False)),
        dict(group="boundary", scores=cutoffs, expected=dict.fromkeys(KEYS, True)),
    ], cutoffs)["per_character"]
    assert routes["journalist"]["tp"] == 2
    assert all(counts["tp"] == 1 for role, counts in routes.items() if role != "journalist")
    assert all(counts["errors"] == 0 for counts in routes.values())
    assert confusion([(True, False), (False, True)])["errors"] == 2
    assert confusion([(False, False)])["precision"] is None
    case = dict(subject_reference="Cedar FC, the FOOTBALL team", sport="FOOTBALL", body="A neutral match report.",
                meta={"identity": {"name": "Cedar FC"}, "identity_record": "Captain injured; striker signed."})
    bank = {"questions": {"subject": {"instructions": "Is this article about {subject}?", "type": "noul"}}}
    payload = request(bank, case)
    assert payload["state"]["article"]["body"] == "A neutral match report."
    assert "injured" not in payload["state"]["article"]["body"]
    assert "Cedar FC, the FOOTBALL team" in payload["questions"]["subject"]["instructions"]
    print("relevance calibration self-test passed")


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        self_test()
    elif len(sys.argv) in (5, 6):
        raise SystemExit(replay(*sys.argv[1:5], int(sys.argv[5]) if len(sys.argv) == 6 else 1))
    else:
        raise SystemExit(__doc__)
