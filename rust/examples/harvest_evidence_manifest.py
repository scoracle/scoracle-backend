"""Reduce a local cohort trace to source-text-free, deterministic audit metadata.

Usage: python3 examples/harvest_evidence_manifest.py TRACE.json OUTPUT.json
The output is an evidence index, not a source archive or a labeled benchmark.
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path


def digest(value):
    return hashlib.sha256(value.encode()).hexdigest()


def reduce_trace(raw):
    trace = json.loads(raw)
    rows, providers = [], {}
    for row in trace['articles']:
        item = {key: row[key] for key in ('article_id', 'entity', 'feed_rank', 'outcome')}
        packet = row.get('packet')
        if packet:
            for key in ('contract_version', 'question_set_versions', 'policy_version',
                        'body_hash', 'input_hashes', 'character_tags', 'signals'):
                item[key] = packet[key]
            item['headline_sha256'] = digest(packet['headline'])
            item['url_sha256'] = digest(packet['url'])
            item['hypothesis'] = packet['hypothesis']
            for key in ('model_input_excerpt', 'excerpt'):
                excerpt = packet.get(key)
                item[key] = None if excerpt is None else {
                    **{k: excerpt[k] for k in ('start', 'end', 'selection')},
                    'text_sha256': digest(excerpt['text']),
                    'bytes': len(excerpt['text'].encode()),
                }
            item['provider_ids'] = {}
            for stage, provenance in packet['provenance'].items():
                if provenance is None:
                    continue
                provider = {k: v for k, v in provenance.items()
                            if k not in ('coverage', 'inference_ms')}
                provider_id = digest(json.dumps(provider, sort_keys=True, separators=(',', ':')))
                providers[provider_id] = provider
                item['provider_ids'][stage] = provider_id
            item['storage_verification_reported_by_trace'] = row.get('storage')
        else:
            if row['outcome'] not in ('publisher_fetch_error', 'classification_error'):
                raise ValueError('unrecognized packet-free outcome')
            # Error text can contain URLs/source material; retain its hash only.
            item['error_sha256'] = digest(row.get('error', ''))
        rows.append(item)
    counts = dict(sorted(Counter(r['outcome'] for r in rows).items()))
    if len(rows) != trace['canonical_candidate_count'] or counts != trace['outcomes']:
        raise ValueError('cohort accounting mismatch')
    return {
        'schema_version': 'harvester-evidence-index-v1',
        'trace_sha256': hashlib.sha256(raw).hexdigest(),
        'cohort_contract': trace['cohort_contract'],
        'limitations': [
            'No publisher text retained; source replay still requires an authorized source archive.',
            'Predictions are uncalibrated distributions, not annotations or gold labels.',
            'Storage verification is a historical trace assertion, not independently replayed here.',
            'Pre-deduplication rows are absent; duplicate provenance cannot be reconstructed.',
        ],
        'canonical_candidates': len(rows), 'outcomes': counts,
        'all_four_assignments': sum(len(r.get('character_tags', [])) == 4 for r in rows),
        'accepted_context_bytes': sum((r.get('excerpt') or {}).get('bytes', 0)
                                      for r in rows if r['outcome'] == 'accepted'),
        'elapsed_ms': trace['elapsed_ms'], 'providers': providers, 'articles': rows,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('trace', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    result = reduce_trace(args.trace.read_bytes())
    with args.output.open('x') as output:
        json.dump(result, output, indent=2, sort_keys=True, ensure_ascii=False)
        output.write('\n')


if __name__ == '__main__':
    main()
