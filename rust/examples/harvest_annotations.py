"""Prepare local Harvester review material and validate human annotations.

The review packet includes licensed source excerpts and belongs in ignored local
storage. Predictions remain in the separate trace/index, never in reviewer input.
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import subprocess

DIMENSIONS = ('entity', 'journalist', 'influencer', 'insider', 'scout')
SPLITS = ('unassigned', 'train', 'calibration', 'test', 'temporal_holdout')


def sha(text):
    return hashlib.sha256(text.encode()).hexdigest()


def read_jsonl(path):
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def load_bound(trace_path, index_path, queue_path):
    raw = trace_path.read_bytes()
    index = json.loads(index_path.read_text())
    require(hashlib.sha256(raw).hexdigest() == index['trace_sha256'],
            'trace differs from durable evidence index')
    trace = json.loads(raw)
    require(trace['canonical_candidate_count'] == index['canonical_candidates'],
            'trace/index candidate count differs')
    rows = read_jsonl(queue_path)
    expected = {(r['article_id'], r['entity']): r for r in trace['articles']}
    require(len(expected) == len(trace['articles']) == len(rows),
            'duplicate or missing candidate rows')
    durable = {(r['article_id'], r['entity']): r for r in index['articles']}
    require(set(durable) == set(expected), 'trace/index candidate keys differ')
    return rows, expected, durable


def validate_evidence_span(span, headline, packet):
    require(isinstance(span, dict) and set(span) == {'field', 'start', 'end'},
            'evidence span must contain field, start, end')
    start, end = span['start'], span['end']
    require(type(start) is int and type(end) is int and 0 <= start < end,
            'invalid evidence byte range')
    if span['field'] == 'headline':
        candidate = headline.encode()
        require(end <= len(candidate), 'headline span exceeds source')
        candidate[start:end].decode('utf-8')
        return
    require(span['field'] == 'body', 'evidence field must be headline or body')
    for key in ('model_input_excerpt', 'excerpt'):
        excerpt = packet.get(key)
        if excerpt and excerpt['start'] <= start and end <= excerpt['end']:
            candidate = excerpt['text'].encode()
            candidate[start - excerpt['start']:end - excerpt['start']].decode('utf-8')
            return
    raise ValueError('body evidence span is outside retained publisher openings')


def validate(rows, trace_rows, index_rows):
    seen, groups, counts = set(), {}, Counter()
    for row in rows:
        key = (row['article_id'], row['query_entity'])
        require(key in trace_rows and key in index_rows and key not in seen,
                f'unknown or duplicate annotation {key}')
        seen.add(key)
        source, durable = trace_rows[key], index_rows[key]
        acquired = source['outcome'] in ('accepted', 'rejected')
        require(row['schema_version'] == 'harvester-annotation-v1', f'{key}: version')
        require(row['source_state'] == ('materialized' if acquired else 'acquisition_error'),
                f'{key}: source state disagrees with trace')
        require(row['body_sha256'] == durable.get('body_hash') and
                row['headline_sha256'] == durable.get('headline_sha256'),
                f'{key}: source hashes disagree with trace')
        require(row['split'] in SPLITS, f'{key}: unknown split')
        require(row['review_status'] in ('pending', 'ai_provisional', 'reviewed', 'adjudicated'),
                f'{key}: unknown review status')
        require(set(row['labels']) == set(DIMENSIONS), f'{key}: missing label dimension')
        packet = source.get('packet')
        if packet:
            require(sha(packet['headline']) == row['headline_sha256'],
                    f'{key}: headline hash mismatch')
            for field in ('model_input_excerpt', 'excerpt'):
                excerpt = packet[field]
                if excerpt:
                    retained = durable[field]
                    require(sha(excerpt['text']) == retained['text_sha256'] and
                            len(excerpt['text'].encode()) == retained['bytes'],
                            f'{key}: retained opening hash mismatch')
        else:
            require(not acquired, f'{key}: missing packet')
        labels = row['labels']
        completed = sum(value is not None for value in labels.values())
        if not acquired:
            require(completed == 0 and row['review_status'] == 'pending' and
                    row['split'] == 'unassigned',
                    f'{key}: acquisition failure cannot be labeled')
        if row['review_status'] == 'pending':
            require(completed == 0 and row['annotator'] is None and row['adjudicator'] is None,
                    f'{key}: pending row has labels or reviewer')
        else:
            require(acquired and isinstance(row['annotator'], str) and row['annotator'].strip(),
                    f'{key}: reviewed row needs annotator')
            require(completed > 0, f'{key}: reviewed row has no decisions')
        if row['review_status'] == 'adjudicated':
            require(completed == 5, f'{key}: adjudication needs five independent labels')
            require(isinstance(row['adjudicator'], str) and row['adjudicator'].strip() and
                    row['adjudicator'] != row['annotator'],
                    f'{key}: independent adjudicator required')
        else:
            require(row['adjudicator'] is None, f'{key}: premature adjudicator')
        if row['review_status'] == 'ai_provisional':
            require(row['annotator'] == 'codex-ai-provisional' and
                    row['split'] == 'unassigned',
                    f'{key}: AI labels cannot enter a train/evaluation split')
        for name, value in labels.items():
            if value is None:
                continue
            require(isinstance(value, dict) and set(value) == {'useful', 'reason', 'evidence'},
                    f'{key}/{name}: label shape')
            require(type(value['useful']) is bool and isinstance(value['reason'], str) and
                    value['reason'].strip(), f'{key}/{name}: reason and boolean required')
            evidence = value['evidence']
            require(isinstance(evidence, list) and (not value['useful'] or evidence),
                    f'{key}/{name}: positive label needs a source span')
            for span in evidence:
                validate_evidence_span(span, packet['headline'], packet)
        group = row['story_group']
        if group is not None:
            require(isinstance(group, str) and group.strip(), f'{key}: empty story group')
            prior = groups.setdefault(group, row['split'])
            require(prior == row['split'], f'{key}: story group crosses splits')
        if row['split'] != 'unassigned':
            require(group is not None and row['review_status'] == 'adjudicated',
                    f'{key}: train/evaluation split needs adjudicated group')
            if row['split'] == 'temporal_holdout':
                require(row['cohort_date'] > '2026-09-26',
                        f'{key}: temporal holdout must be a later cohort')
        counts[row['review_status']] += 1
    require(seen == set(trace_rows), 'annotation queue omits candidates')
    return dict(sorted(counts.items()))


def review_rows(rows, trace_rows):
    output = []
    for row in rows:
        source = trace_rows[(row['article_id'], row['query_entity'])]
        packet = source.get('packet')
        view = {'article_id': row['article_id'], 'query_entity': row['query_entity'],
                'source_state': row['source_state'], 'body_sha256': row['body_sha256'],
                'headline_sha256': row['headline_sha256'],
                'review_status': row['review_status'], 'labels': row['labels']}
        if packet:
            view['headline'] = packet['headline']
            view['publisher_openings'] = [packet[k] for k in ('model_input_excerpt', 'excerpt')
                                          if packet.get(k)]
            view['evidence_scope'] = 'retained publisher openings only; not a complete body'
        output.append(view)
    return output


def verify_retained_bodies(candidates, trace_rows):
    """Verify complete source bytes where the pre-replay export retained a body."""
    checked = 0
    for candidate in candidates:
        source = trace_rows[(candidate['article_id'], candidate['hypothesis']['name'])]
        body = candidate.get('body') or ''
        if not body:
            continue
        packet = source.get('packet')
        require(packet is not None, f"{candidate['article_id']}: body but no packet")
        require(sha(body) == packet['body_hash'], f"{candidate['article_id']}: body hash")
        require(candidate['title'] == packet['headline'], f"{candidate['article_id']}: headline")
        raw = body.encode()
        for field in ('model_input_excerpt', 'excerpt'):
            excerpt = packet[field]
            if excerpt:
                require(raw[excerpt['start']:excerpt['end']] == excerpt['text'].encode(),
                        f"{candidate['article_id']}: {field} byte range")
        checked += 1
    return checked


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--trace', type=Path, required=True)
    parser.add_argument('--index', type=Path, required=True)
    parser.add_argument('--queue', type=Path, required=True)
    parser.add_argument('--candidates', type=Path,
                        help='Optional local pre-replay export to verify retained full bodies')
    parser.add_argument('--review-output', type=Path,
                        help='Path ignored by Git; includes licensed publisher excerpts')
    args = parser.parse_args()
    rows, trace_rows, index_rows = load_bound(args.trace, args.index, args.queue)
    counts = validate(rows, trace_rows, index_rows)
    verified_bodies = (verify_retained_bodies(read_jsonl(args.candidates), trace_rows)
                       if args.candidates else None)
    if args.review_output:
        output = args.review_output.resolve()
        ignored = subprocess.run(['git', 'check-ignore', '-q', str(output)],
                                 check=False, capture_output=True)
        require(ignored.returncode == 0, 'review output must be ignored by Git')
        with output.open('x') as writer:
            for row in review_rows(rows, trace_rows):
                writer.write(json.dumps(row, ensure_ascii=False, sort_keys=True) + '\n')
    print(json.dumps({'candidates': len(rows), 'review_status': counts,
                      'full_bodies_verified': verified_bodies,
                      'trainable_gold': sum(r['review_status'] == 'adjudicated' and
                                            r['split'] != 'unassigned' for r in rows)}))


if __name__ == '__main__':
    main()
