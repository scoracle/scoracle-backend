#!/usr/bin/env python3
"""Read-only headline controls: native decision endpoint or a pinned ModernBERT NLI head.

Export exact production requests with classifier_replay CASES.jsonl --requests-only.
Run: headline_model_bench.py native URL REQUESTS.jsonl OUTPUT.jsonl
  or headline_model_bench.py modernbert MODEL@REVISION REQUESTS.jsonl OUTPUT.jsonl
Synthetic labels are provisional diagnostics, not a sports-news qualification set.
"""
import argparse
import hashlib
import json
import math
import platform
import sys
import time
import urllib.request
from pathlib import Path

KEYS = {"subject", "emotion", "movement", "availability", "clickbait"}


def http(url, payload=None):
    request = urllib.request.Request(url, data=None if payload is None else json.dumps(payload).encode(),
                                     headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(request, timeout=120) as response:
        return json.load(response)


def counts(pairs):
    pairs = list(pairs)
    tp = sum(p and y for p, y in pairs)
    fp = sum(p and not y for p, y in pairs)
    fn = sum(not p and y for p, y in pairs)
    tn = len(pairs) - tp - fp - fn
    return dict(tp=tp, fp=fp, fn=fn, tn=tn,
                precision=tp / (tp + fp) if tp + fp else None,
                recall=tp / (tp + fn) if tp + fn else None,
                f1=2 * tp / (2 * tp + fp + fn) if 2 * tp + fp + fn else None)


def summarize(rows):
    good = [row for row in rows if "scores" in row]
    per_label = {}
    for key in sorted(KEYS):
        labeled = [row for row in good if key in row["expected"]]
        if labeled:
            per_label[key] = dict(
                at_50=counts((r["scores"][key] >= .5, r["expected"][key]) for r in labeled),
                brier=sum((r["scores"][key] - r["expected"][key]) ** 2 for r in labeled) / len(labeled))
    routing = {}
    for character, key, cutoff in [("journalist", "subject", .55), ("influencer", "emotion", .15), ("insider", "movement", .80)]:
        labeled = [r for r in good if {"subject", key} <= r["expected"].keys()]
        routing[character] = counts((r["scores"]["subject"] >= .55 and r["scores"][key] >= cutoff,
                                     r["expected"]["subject"] and r["expected"][key]) for r in labeled)
    latencies = sorted(r["seconds"] for r in good)
    return dict(completed=len(good), failed=len(rows)-len(good), per_label=per_label,
                routing=routing, median_seconds=latencies[len(latencies)//2] if latencies else None,
                p95_seconds=latencies[max(0, math.ceil(len(latencies)*.95)-1)] if latencies else None)


def native(endpoint):
    inventory = http(endpoint.rstrip("/") + "/v1/models")["data"]
    if len(inventory) != 1:
        raise ValueError("one native model required")
    model = inventory[0]["id"]
    if "@sha256:" not in model:
        raise ValueError("native benchmark needs fingerprinted weights")

    def score(case):
        response = http(endpoint.rstrip("/") + "/v1/systemone", case["request"])
        if response.get("model") != model or set(response.get("answers", {})) != KEYS:
            raise ValueError("model changed or incomplete native answer")
        scores = {}
        for key, answer in response["answers"].items():
            value = answer.get("noul")
            if answer.get("type") != "noul" or type(value) not in (int, float) or not math.isfinite(value) or not 0 <= value <= 1:
                raise ValueError("invalid native probability")
            scores[key] = value
        return scores, {"response": response}
    return score, dict(model=model, endpoint=endpoint, inventory=inventory)


def modernbert(spec):
    import torch
    from transformers import AutoModelForSequenceClassification, AutoTokenizer
    repo, revision = spec.rsplit("@", 1)
    if len(revision) != 40 or any(c not in "0123456789abcdef" for c in revision):
        raise ValueError("pin a full Hugging Face commit revision")
    torch.set_num_threads(4)
    tokenizer = AutoTokenizer.from_pretrained(repo, revision=revision)
    model = AutoModelForSequenceClassification.from_pretrained(repo, revision=revision,
        torch_dtype=torch.float32, attn_implementation="eager").to("cpu").eval()
    entailment = [int(i) for i, label in model.config.id2label.items() if label.lower() == "entailment"]
    if len(entailment) != 1:
        raise ValueError("model requires an explicit entailment class")
    limit = model.config.max_position_embeddings

    def score(case):
        subject = case["request"]["state"]["subject_meta"]["identity"]["name"]
        hypotheses = {
            "subject": f"The headline's reporting substantively involves {subject}, rather than merely making a passing historical comparison.",
            "emotion": f"The headline expresses feelings or emotional reactions about {subject}, such as joy, hope, pride, anger, frustration, fear, disappointment, grief or relief, rather than merely reporting a result.",
            "movement": f"The headline reports a current transfer, trade, loan, signing, departure, contract negotiation or coaching change involving {subject}, including rumours or denials, rather than historical background or fantasy trades.",
            "availability": f"The headline reports injury, illness, fitness, suspension, eligibility, rest, absence or return affecting whether {subject} or its players can participate.",
            "clickbait": "The headline uses sensationalism, exaggerated certainty, a misleading tease or withheld key information to entice clicks, rather than merely a surprising concrete claim or an attributed quotation."
        }
        keys = sorted(KEYS)
        premise = json.dumps(case["request"]["state"], ensure_ascii=False)
        inputs = tokenizer([premise]*len(keys), [hypotheses[k] for k in keys], padding=True,
                           truncation=False, return_tensors="pt")
        lengths = inputs["attention_mask"].sum(dim=1).tolist()
        if max(lengths) > limit:
            raise ValueError("complete NLI pair exceeds model context")
        with torch.inference_mode():
            probabilities = model(**inputs).logits.softmax(-1)[:, entailment[0]].tolist()
        return dict(zip(keys, probabilities)), dict(hypotheses=hypotheses, token_counts=lengths,
                                                   token_ids=inputs["input_ids"].tolist(), truncated=False)
    return score, dict(model=spec, backend="transformers-nli", torch=torch.__version__,
                       device="cpu", threads=4, platform=platform.platform(), id2label=model.config.id2label)


def self_test():
    assert counts([(True,True),(True,False),(False,True),(False,False)]) == dict(tp=1,fp=1,fn=1,tn=1,precision=.5,recall=.5,f1=.5)
    rows = [dict(scores=dict.fromkeys(KEYS,.9),expected=dict.fromkeys(KEYS,True),seconds=.01)]
    result = summarize(rows)
    assert result["routing"]["influencer"]["tp"] == 1
    assert result["per_label"]["emotion"]["at_50"]["fp"] == 0
    assert summarize(rows+[dict(error="failure")])["failed"] == 1
    print("headline benchmark self-check passed")


def main():
    if sys.argv[1:] == ["--self-test"]:
        self_test()
        return
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["native", "modernbert"])
    parser.add_argument("model")
    parser.add_argument("requests", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    cases = [json.loads(line) for line in args.requests.read_text().splitlines() if line.strip()]
    if not cases or len({c["id"] for c in cases}) != len(cases):
        raise ValueError("nonempty uniquely identified cases required")
    for case in cases:
        if set(case["request"]["questions"]) != KEYS or not set(case["expected"]) <= KEYS or any(type(v) is not bool for v in case["expected"].values()):
            raise ValueError("invalid headline spectrum case")
    output = args.output.open("x")
    score, identity = native(args.model) if args.mode == "native" else modernbert(args.model)
    # Exclude one warmup request; every measured case is still retained.
    score(cases[0])
    rows = []
    for case in cases:
        started = time.perf_counter()
        row = dict(id=case["id"],headline=case["headline"],expected=case["expected"],identity=identity,
                   request_sha256=hashlib.sha256(json.dumps(case["request"],sort_keys=True).encode()).hexdigest())
        try:
            scores, receipt = score(case)
            if set(scores) != KEYS or any(not math.isfinite(v) or not 0 <= v <= 1 for v in scores.values()):
                raise ValueError("invalid spectrum")
            row.update(scores=scores,receipt=receipt)
        except Exception as error:
            row["error"] = repr(error)
        row["seconds"] = time.perf_counter()-started
        rows.append(row)
        output.write(json.dumps(row,ensure_ascii=False)+"\n")
        output.flush()
    output.close()
    summary=dict(summarize(rows),identity=identity,input_sha256=hashlib.sha256(args.requests.read_bytes()).hexdigest(),
                 qualification="provisional diagnostic labels; no production promotion; hardware differs across endpoints")
    args.output.with_suffix(".summary.json").write_text(json.dumps(summary,indent=2)+"\n")
    print(json.dumps(summary,indent=2))
    if summary["failed"]:
        raise SystemExit("benchmark contains failed requests")


if __name__ == "__main__":
    main()
