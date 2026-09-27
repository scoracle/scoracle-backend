"""Build source-bound, story-grouped Laya supervision from Rust-prepared requests.

Labels are provisional annotations, not model predictions or Editor ground truth.
Only labelled fields enter training; unknown fields are omitted.
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path


def digest(value):
    return hashlib.sha256(json.dumps(value, ensure_ascii=False, sort_keys=True,
                                     separators=(",", ":")).encode()).hexdigest()


def split_for(group, seed):
    bucket = int(hashlib.sha256(f"{seed}:{group}".encode()).hexdigest()[:8], 16) % 100
    return "train" if bucket < 60 else "calibration" if bucket < 80 else "test"


def build(prepared, annotations, seed=20260924):
    sources = {r["article_id"]: r for r in prepared}
    if len(sources) != len(prepared):
        raise ValueError("duplicate source article IDs")
    seen = set()
    examples = []
    for label in annotations:
        aid = label["article_id"]
        if aid in seen:
            raise ValueError("duplicate annotation")
        seen.add(aid)
        request = sources[aid]["request"]
        if digest(request) != label["request_sha256"]:
            raise ValueError(f"source/question drift for {aid}; review labels again")
        group = label["story_group"]
        if not isinstance(group, str) or not group.strip():
            raise ValueError("story group required")
        targets = label["labels"]
        if not targets:
            continue  # Entirely uncertain example remains in the annotation review set.
        if set(targets) - {"entity", "content"}:
            raise ValueError("pilot only supervises entity and content")
        for qid, answer in targets.items():
            if answer not in request["questions"][qid]["criteria"]:
                raise ValueError(f"unknown option {qid}/{answer}")
        examples.append({"article_id": aid, "story_group": group,
                         "split": split_for(group, seed), "request": request,
                         "labels": targets, "label_source": label["label_source"],
                         "human_reviewed": label["human_reviewed"]})
    # Identical model inputs must not leak across story groups/splits.
    input_groups = {}
    for row in examples:
        key = digest(row["request"])
        previous = input_groups.setdefault(key, row["story_group"])
        if previous != row["story_group"]:
            raise ValueError("duplicate model input spans story groups")
    return examples


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--prepared", type=Path, required=True)
    p.add_argument("--labels", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--seed", type=int, default=20260924)
    args = p.parse_args()
    read = lambda path: [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    rows = build(read(args.prepared), read(args.labels), args.seed)
    with args.output.open("x") as f:
        for row in rows:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")
    print(json.dumps({split: {"articles": sum(r["split"] == split for r in rows),
        "labels": dict(Counter(f"{q}:{v}" for r in rows if r["split"] == split
                               for q, v in r["labels"].items()))}
        for split in ("train", "calibration", "test")}, indent=2))


if __name__ == "__main__":
    main()
