#!/usr/bin/env python3
"""Score reviewed headline-read labels without exposing source text.

Usage: python3 harvest_headline_score.py REPLAY.json MANIFEST.json LABELS.csv
       development|holdout RELEVANT_COUNT IRRELEVANT_COUNT UNCLASSIFIED_COUNT

Review only development while choosing a policy. Run holdout once after the
policy is frozen. The output is an approximate, stratum-weighted comparison;
the packet is small and old body decisions were sampling strata, not gold.
"""

import csv
import json
import sys
from collections import Counter, defaultdict


THRESHOLDS = (0.0, 0.05, 0.10, 0.15, 0.20, 0.25, 0.30, 0.40, 0.50, 0.70, 0.90)
STRATA = ("relevant", "irrelevant", "unclassified")


def main():
    if len(sys.argv) != 8 or sys.argv[4] not in ("development", "holdout"):
        raise SystemExit(__doc__)
    replay_path, manifest_path, labels_path, split = sys.argv[1:5]
    population = dict(zip(STRATA, map(int, sys.argv[5:8])))
    if any(n <= 0 for n in population.values()):
        raise SystemExit("all population counts must be positive")

    replay = json.load(open(replay_path, encoding="utf-8"))
    manifest = json.load(open(manifest_path, encoding="utf-8"))
    with open(labels_path, newline="", encoding="utf-8") as handle:
        label_rows = list(csv.DictReader(handle))

    replay_counts = Counter()
    sample_counts = Counter()
    for row in replay["rows"]:
        p = row.get("p_relevant")
        if p is not None:
            replay_counts[(row["old_body_stratum_not_gold"], p >= 0.25)] += 1
            sample_counts[row["old_body_stratum_not_gold"]] += 1

    selected = {row["case_id"]: row for row in manifest if row["split"] == split}
    if len(selected) != 36:
        raise SystemExit(f"expected 36 {split} manifest cases, got {len(selected)}")
    if {row["case_id"] for row in label_rows} != set(selected):
        raise SystemExit("label sheet case IDs do not match selected manifest cases")

    labeled = []
    labeled_counts = Counter()
    unresolved = Counter()
    for row in label_rows:
        case = selected[row["case_id"]]
        answer = row["headline_should_read"].strip().lower()
        if answer not in ("yes", "no", "unsure", ""):
            raise SystemExit(f"invalid headline label for {row['case_id']}: {answer}")
        cell = (case["old_body_stratum_not_gold"], case["p_relevant"] >= 0.25)
        if answer in ("yes", "no"):
            labeled.append((case, answer == "yes"))
            labeled_counts[cell] += 1
        else:
            unresolved[cell] += 1

    if unresolved:
        print("Unresolved labels by sampling cell:", dict(sorted(unresolved.items())))
    if any(labeled_counts[cell] == 0 for cell in replay_counts):
        print("Every sampling cell needs at least one yes/no label before weighted scoring.")
        return

    def weight(case):
        stratum = case["old_body_stratum_not_gold"]
        cell = (stratum, case["p_relevant"] >= 0.25)
        return (
            population[stratum]
            * replay_counts[cell]
            / sample_counts[stratum]
            / labeled_counts[cell]
        )

    print(f"Split: {split}; reviewed yes/no: {len(labeled)}/36")
    print("Threshold | weighted recall | weighted precision | weighted read rate | reviewed TP/FP/FN/TN")
    for threshold in THRESHOLDS:
        weighted = defaultdict(float)
        raw = Counter()
        for case, truth in labeled:
            admitted = case["p_relevant"] >= threshold
            outcome = "TP" if truth and admitted else "FN" if truth else "FP" if admitted else "TN"
            weighted[outcome] += weight(case)
            raw[outcome] += 1
        recall_denom = weighted["TP"] + weighted["FN"]
        precision_denom = weighted["TP"] + weighted["FP"]
        total = sum(weighted.values())
        recall = weighted["TP"] / recall_denom if recall_denom else float("nan")
        precision = weighted["TP"] / precision_denom if precision_denom else float("nan")
        read_rate = precision_denom / total
        counts = "/".join(str(raw[k]) for k in ("TP", "FP", "FN", "TN"))
        print(f"{threshold:.2f} | {recall:.3f} | {precision:.3f} | {read_rate:.3f} | {counts}")
    print("Estimates are approximate: small enriched sample, unresolved labels excluded, article-only deduplication.")


if __name__ == "__main__":
    main()
