"""Aggregate Laya replay agreement with source-hash-matched AI provisional labels.

This is a triage metric, not human-grounded precision or recall. Inputs stay local;
only counts and ratios are printed. Re-acquired source bodies with changed hashes
are excluded rather than treating labels on different text as valid comparisons.
"""
import argparse
from collections import Counter
import json
from pathlib import Path

PLUGIN = {
    'entity': None,
    'journalist': 'scoracle.character.narrative',
    'influencer': 'scoracle.character.vibe',
    'insider': 'scoracle.character.transfers',
    'scout': 'scoracle.character.rating',
}


def rows(path):
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def evaluate(replay, annotations):
    by_key = {(row['article_id'], row['query_entity']): row for row in annotations}
    if len(by_key) != len(annotations):
        raise ValueError('duplicate annotation key')
    counts = Counter()
    confusion = {dimension: Counter() for dimension in PLUGIN}
    for row in replay:
        if row.get('state') != 'classified':
            counts['not_classified'] += 1
            continue
        counts['classified'] += 1
        key = (row['article_id'], row['query_entity']['name'])
        annotation = by_key.get(key)
        if annotation is None:
            counts['missing_annotation'] += 1
            continue
        if annotation['body_sha256'] != row['body_sha256']:
            counts['source_hash_mismatch'] += 1
            continue
        counts['source_hash_matched'] += 1
        if annotation['review_status'] != 'ai_provisional':
            counts['not_provisionally_labelled'] += 1
            continue
        for dimension, plugin_id in PLUGIN.items():
            label = annotation['labels'][dimension]
            if label is None:
                continue
            actual = label['useful']
            predicted = (row['entity_choice'] == 'relevant') if plugin_id is None else (
                plugin_id in row['advisory_characters'])
            confusion[dimension][('tp' if actual else 'fp') if predicted else
                                 ('fn' if actual else 'tn')] += 1
    dimensions = {}
    for dimension, matrix in confusion.items():
        tp, fp, fn, tn = (matrix[key] for key in ('tp', 'fp', 'fn', 'tn'))
        dimensions[dimension] = {
            'labelled': tp + fp + fn + tn,
            'positive_labels': tp + fn,
            'tp': tp, 'fp': fp, 'fn': fn, 'tn': tn,
            'precision_proxy': round(tp / (tp + fp), 3) if tp + fp else None,
            'recall_proxy': round(tp / (tp + fn), 3) if tp + fn else None,
        }
    return {
        'scope': 'historical replay vs AI provisional labels; not human gold or production-nightly accuracy',
        'source_binding': dict(counts),
        'dimensions': dimensions,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--replay', type=Path, required=True)
    parser.add_argument('--annotations', type=Path, required=True)
    args = parser.parse_args()
    replay = json.loads(args.replay.read_text())['rows']
    print(json.dumps(evaluate(replay, rows(args.annotations)), indent=2, sort_keys=True))


if __name__ == '__main__':
    main()
