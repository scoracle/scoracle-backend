"""Score source-bound synthetic route fixtures against context_harvest JSONL.

These labels are provisional development expectations, not human-reviewed gold.
Incomplete/error runs fail instead of turning inference failures into negatives.
"""
import argparse
from collections import Counter
import hashlib
import json


DESTINATIONS = {
    "scoracle.character.narrative": "journalist",
    "scoracle.character.vibe": "influencer",
    "scoracle.character.transfers": "insider",
    "scoracle.character.rating": "scout",
}


def read_rows(path, key):
    rows = {}
    with open(path) as stream:
        for line in stream:
            row = json.loads(line)
            identity = key(row)
            if identity in rows:
                raise ValueError(f"duplicate article {identity}")
            rows[identity] = row
    return rows


def score(fixtures, replay):
    if not fixtures or fixtures.keys() != replay.keys():
        raise ValueError("replay must cover every fixture exactly once")
    totals = Counter(TP=0, FP=0, FN=0, TN=0)
    per_route = {name: totals.copy() for name in DESTINATIONS.values()}
    mismatches = []
    rejected = []
    revisions = set()
    for identity, fixture in fixtures.items():
        row = replay[identity]
        if row.get("disposition") not in ("accept", "reject"):
            raise ValueError(f"article {identity}: incomplete/error replay")
        article, context = fixture["article"], row["context"]
        if context["contract_version"] != "harvest-context-v7":
            raise ValueError("requires the v7 scalar predicate contract")
        for source, saved in [("title", "headline"), ("source", "source"),
                              ("url", "url"), ("hypothesis", "hypothesis")]:
            if article[source] != context[saved]:
                raise ValueError(f"article {identity}: {source} changed")
        digest = hashlib.sha256(article["body"].encode()).hexdigest()
        if digest != context["body_sha256"]:
            raise ValueError(f"article {identity}: source body changed")
        labels = fixture["routes"]
        if set(labels) != set(per_route) or any(type(v) is not bool for v in labels.values()):
            raise ValueError(f"article {identity}: incomplete route labels")
        predicted = {DESTINATIONS[key] for key in context["recommended_characters"]}
        expected = {key for key, value in labels.items() if value}
        revisions.add(context["model_revision"])
        if row["disposition"] == "reject":
            if predicted:
                raise ValueError(f"article {identity}: rejected headline has routes")
            rejected.append(fixture["case_id"])
        for route, label in labels.items():
            metric = ("T" if (route in predicted) == label else "F") + ("P" if route in predicted else "N")
            totals[metric] += 1
            per_route[route][metric] += 1
        if expected != predicted:
            mismatches.append({"case_id": fixture["case_id"],
                               "extra": sorted(predicted - expected),
                               "missing": sorted(expected - predicted)})
    return {"label_status": "synthetic provisional development expectations",
            "articles": len(fixtures), "model_revisions": sorted(revisions),
            "headline_rejections": rejected, "route_pairs": dict(totals),
            "per_route": per_route, "mismatches": mismatches}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fixtures")
    parser.add_argument("replay")
    args = parser.parse_args()
    fixtures = read_rows(args.fixtures, lambda row: row["article"]["article_id"])
    replay = read_rows(args.replay, lambda row: row["article_id"])
    print(json.dumps(score(fixtures, replay), indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
