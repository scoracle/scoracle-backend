"""Prepare or validate blind, source-bound review packets; never supplies gold labels.

python classifier_review.py WINDOWED_SOURCE.jsonl REVIEW.jsonl [--validate]
Positive labels require exact evidence within that unit's model-visible window.
Use null for unknown; inspect the whole source for extraction and boundary problems.
"""
import argparse
import json
import math
from pathlib import Path

from classifier_replay import SCHEMA_PATH, digest, head_labels, model_text, source_windows


def validate_review_state(status, reviewer, adjudicator, completed, split):
    if status not in ("pending", "ai_provisional", "reviewed", "adjudicated"):
        raise ValueError("unknown review status")
    if status == "pending":
        if completed or reviewer is not None or adjudicator is not None:
            raise ValueError("pending row has labels or reviewer")
    elif not isinstance(reviewer, str) or not reviewer.strip() or not completed:
        raise ValueError("reviewed row needs a named reviewer and decisions")
    if status == "adjudicated":
        if not isinstance(adjudicator, str) or not adjudicator.strip() or adjudicator == reviewer:
            raise ValueError("independent adjudicator required")
    elif adjudicator is not None:
        raise ValueError("premature adjudicator")
    if status == "ai_provisional" and (reviewer != "codex-ai-provisional" or split != "unassigned"):
        raise ValueError("AI labels cannot enter a train/evaluation split")
    if split != "unassigned" and status != "adjudicated":
        raise ValueError("train/evaluation split needs adjudicated review")


def prepare(item, schema, previously_measured=()):
    windows = source_windows(item["body"], item["windows"])
    units = []
    for window in windows:
        for scope in ("document_window", "target_window"):
            families = [name for name, vector in schema["vectors"].items() if vector["scope"] == scope]
            targets = [None] if scope == "document_window" else item["query_entities"]
            for target in targets:
                units.append({"scope": scope, "target": target,
                              "start": window["start"], "end": window["end"],
                              "model_text": model_text(dict(item, target=target), window["text"], scope),
                              "labels": {f"{family}.{label}": None for family in families
                                         for label in schema["vectors"][family]["labels"]},
                              "ordinal_annotations": {name: None for name, vector in schema["ordinal_vectors"].items()
                                                      if vector["scope"] == scope},
                              "review_status": "pending", "reviewer": None, "adjudicator": None,
                              "evidence": [], "notes": None})
    return {"contract": "classifier-review-v2", "article_id": item["article_id"],
            "body_sha256": digest(item["body"]), "schema_version": schema["version"],
            "schema_sha256": digest(json.dumps(schema, sort_keys=True)),
            "source": {key: value for key, value in item.items() if key not in ("body", "windows")},
            "body": item["body"], "windows": windows,
            "previously_measured": item["article_id"] in previously_measured,
            "split": None, "syndication_group": None,
            "extraction_review": {"reviewer": None, "usable": None, "problems": [], "notes": None},
            "units": units}


def validate(record, item, schema, previously_measured=()):
    expected = prepare(item, schema, previously_measured)
    for key in ("contract", "article_id", "body_sha256", "schema_version", "schema_sha256",
                "source", "body", "windows", "previously_measured"):
        if record[key] != expected[key]:
            raise ValueError(f"retained source or schema changed: {key}")
    if record["split"] not in (None, "train", "dev", "test"):
        raise ValueError("split must be train, dev, test or unassigned")
    if record["split"] == "test" and record["previously_measured"]:
        raise ValueError("previously measured Phase 1 source cannot be a fresh held-out test item")
    if record["split"] is not None and (not isinstance(record["syndication_group"], str)
            or not record["syndication_group"].strip()):
        raise ValueError("assign a syndication group before splitting")
    extraction = record["extraction_review"]
    if extraction["usable"] is not None and (type(extraction["usable"]) is not bool
            or not isinstance(extraction["reviewer"], str) or not extraction["reviewer"].strip()):
        raise ValueError("extraction usability requires a named reviewer and a boolean")
    if len(record["units"]) != len(expected["units"]):
        raise ValueError("review units omitted or duplicated")
    body = item["body"].encode()
    reviewed = 0
    for unit, original in zip(record["units"], expected["units"], strict=True):
        for key in ("scope", "target", "start", "end", "model_text"):
            if unit[key] != original[key]:
                raise ValueError(f"model-visible review unit changed: {key}")
        if set(unit["labels"]) != set(original["labels"]):
            raise ValueError("review label schema changed")
        if set(unit["ordinal_annotations"]) != set(original["ordinal_annotations"]):
            raise ValueError("ordinal annotation schema changed")
        for name, value in unit["ordinal_annotations"].items():
            vector = schema["ordinal_vectors"][name]
            if value is not None and (type(value) not in (int, float) or not math.isfinite(value)
                    or not vector["minimum"] <= value <= vector["maximum"]):
                raise ValueError("ordinal annotation is outside its declared scale")
        if any(value is not None and (type(value) is not int or value not in (0, 1))
               for value in unit["labels"].values()):
            raise ValueError("review labels must be null, 0 or 1")
        completed = sum(value is not None for value in unit["labels"].values()) + sum(
            value is not None for value in unit["ordinal_annotations"].values())
        validate_review_state(unit["review_status"], unit["reviewer"], unit["adjudicator"],
                              completed, (record["split"] or "unassigned") if completed else "unassigned")
        if unit["reviewer"] is None:
            if (any(value is not None for value in unit["labels"].values())
                    or any(value is not None for value in unit["ordinal_annotations"].values()) or unit["evidence"]):
                raise ValueError("labels or evidence require a named reviewer")
            continue
        if not isinstance(unit["reviewer"], str) or not unit["reviewer"].strip():
            raise ValueError("named reviewer required")
        reviewed += 1
        supported = set()
        for evidence in unit["evidence"]:
            start, end = evidence["start"], evidence["end"]
            if (type(start) is not int or type(end) is not int
                    or not unit["start"] <= start < end <= unit["end"]
                    or body[start:end].decode() != evidence["quote"]):
                raise ValueError("evidence is not an exact model-visible source span")
            if (unit["labels"].get(evidence["label"]) != 1
                    and unit["ordinal_annotations"].get(evidence["label"]) is None):
                raise ValueError("evidence must support a reviewed positive label")
            if not isinstance(evidence.get("qualifiers"), dict) or set(evidence["qualifiers"]) != set(schema["qualifiers"]):
                raise ValueError("retain speaker, target, time and claim qualifiers; use null for unknown")
            supported.add(evidence["label"])
        if any(value == 1 and label not in supported for label, value in unit["labels"].items()):
            raise ValueError("positive label lacks exact supporting evidence")
        if any(value is not None and label not in supported for label, value in unit["ordinal_annotations"].items()):
            raise ValueError("ordinal annotation lacks exact supporting evidence")
    return reviewed


def load_review(source, review, schema, prior):
    items = [json.loads(line) for line in source.read_text().splitlines() if line.strip()]
    records = [json.loads(line) for line in review.read_text().splitlines() if line.strip()]
    if not items or len({item["article_id"] for item in items}) != len(items) or len(records) != len(items):
        raise ValueError("unique canonical sources and matching review packets required")
    groups, bodies = {}, {}
    for record, item in zip(records, items, strict=True):
        validate(record, item, schema, prior)
        split = record["split"]
        if split is not None:
            for ledger, key in ((groups, record["syndication_group"]), (bodies, record["body_sha256"])):
                if ledger.setdefault(key, split) != split:
                    raise ValueError("syndicated or identical sources leak across splits")
    return records


def training_rows(records, schema, families):
    mapping = head_labels(schema, families)
    scope = schema["vectors"][families[0]]["scope"]
    result = {split: [] for split in ("train", "dev", "test")}
    for record in records:
        if record["split"] is None:
            continue
        if record["extraction_review"]["usable"] is not True:
            raise ValueError("split source needs reviewed usable extraction")
        for unit in record["units"]:
            if unit["scope"] != scope:
                continue
            labels = [unit["labels"][f"{family}.{label}"] for family, label in mapping.values()]
            if all(label is None for label in labels):
                continue
            if unit["review_status"] != "adjudicated":
                raise ValueError("training/evaluation requires independently adjudicated labels")
            result[record["split"]].append({"text": unit["model_text"], "labels": labels})
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("review", type=Path)
    parser.add_argument("--validate", action="store_true")
    parser.add_argument("--schema", type=Path, default=SCHEMA_PATH)
    parser.add_argument("--prior-manifest", type=Path, default=SCHEMA_PATH.with_name("corpus-manifest-2026-10-07.jsonl"))
    args = parser.parse_args()
    schema = json.loads(args.schema.read_text())
    prior = {row["article_id"] for row in map(json.loads, args.prior_manifest.read_text().splitlines())}
    if args.validate:
        records = load_review(args.source, args.review, schema, prior)
        reviewed = sum(unit["review_status"] != "pending" for record in records for unit in record["units"])
        print(f"Validated {len(records)} source packets; {reviewed} units have a named reviewer. This does not verify review independence or accuracy.")
    else:
        items = [json.loads(line) for line in args.source.read_text().splitlines() if line.strip()]
        if not items or len({item["article_id"] for item in items}) != len(items):
            parser.error("nonempty canonical source IDs must be unique")
        with args.review.open("x") as output:
            for item in items:
                record = prepare(item, schema, prior)
                validate(record, item, schema, prior)
                output.write(json.dumps(record, ensure_ascii=False, allow_nan=False) + "\n")
        print(f"Prepared {len(items)} unreviewed source packets; no labels supplied.")


if __name__ == "__main__":
    main()
