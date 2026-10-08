"""Score frozen source-only checkpoints separately from native contract acceptance.

classifier_accuracy.py REFERENCE.jsonl NATIVE_RECEIPTS.jsonl OUTPUT.json
These focused AI provisional checks are not an independently adjudicated gold set.
"""
import argparse
import hashlib
import json
from pathlib import Path


def overlap(claim, check):
    span = claim['evidence']
    return max(0, min(span['end'], check['end']) - max(span['start'], check['start'])) >= .8 * (check['end'] - check['start'])


def matches(claim, check):
    if not overlap(claim, check) or claim['target_relation'] != check['relation'] or claim['kind'] not in check['kind']:
        return False
    if 'time' in check and claim['time_scope'] not in check['time']:
        return False
    qualifiers = claim['qualifiers']
    if 'speaker' in check and not any(check['speaker'] in span['quote'] for span in qualifiers.get('speaker') or []):
        return False
    if 'qualifier' in check and not qualifiers.get(check['qualifier']):
        return False
    return True


def assess(receipt, reference):
    source = receipt['source']
    if (hashlib.sha256(source['body'].encode()).hexdigest() != reference['body_sha256']
            or receipt['target'] != reference['target']):
        raise ValueError('evaluation source/target drift')
    try:
        proposed = json.loads(receipt['raw_response'])
    except (ValueError, TypeError):
        proposed = None
    bound = receipt.get('qualification')
    usable = proposed.get('extraction_usable') if isinstance(proposed, dict) else None
    result = {'article_id': receipt['article_id'], 'model': receipt['model'], 'cohort': reference['cohort'],
              'native_status': receipt['status'], 'native_error': receipt.get('error'),
              'inference_status': receipt['model_provenance'].get('inference_status'),
              'usability_expected': reference['usable'], 'usability_proposed': usable,
              'usability_correct': None if reference['usable'] is None else usable is reference['usable'],
              'checkpoints': [], 'no_target_emotion_expected': reference['emotion'] == 'none',
              'no_target_emotion_correct': None, 'review_status': 'ai_provisional', 'production_eligible': False}
    if bound:
        for check in reference['checks']:
            result['checkpoints'].append({'expected_kind': check['kind'], 'expected_relation': check['relation'],
                                         'passed': any(matches(claim, check) for claim in bound['claims'])})
        if result['no_target_emotion_expected']:
            result['no_target_emotion_correct'] = not any(claim['target_relation'] == 'direct_subject'
                and claim['kind'] in ('emotion', 'emotion_denial', 'conditional_emotion') for claim in bound['claims'])
    else:
        result['checkpoints'] = [{'expected_kind': check['kind'], 'expected_relation': check['relation'],
                                 'passed': False} for check in reference['checks']]
        # An explicit unusable acquisition is a retained source disposition, not usable evidence.
        if reference['usable'] is False and isinstance(proposed, dict) and usable is False:
            result['no_target_emotion_correct'] = proposed.get('claims') == []
        elif result['no_target_emotion_expected']:
            result['no_target_emotion_correct'] = False
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('reference', type=Path)
    parser.add_argument('receipts', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    references = {row['article_id']: row for row in map(json.loads, args.reference.read_text().splitlines())}
    receipts = list(map(json.loads, args.receipts.read_text().splitlines()))
    identities = {(row['model'], row['article_id'], json.dumps(row['target'], sort_keys=True)) for row in receipts}
    if len(identities) != len(receipts):
        parser.error('duplicate evaluation receipt')
    cases = [assess(row, references[row['article_id']]) for row in receipts]
    models = {}
    for name in dict.fromkeys(row['model'] for row in cases):
        models[name] = {}
        for cohort in ('fresh_real', 'fresh_control', 'existing_diagnostic'):
            rows = [row for row in cases if row['model'] == name and row['cohort'] == cohort]
            checkpoints = [check for row in rows for check in row['checkpoints']]
            usability = [row['usability_correct'] for row in rows if row['usability_correct'] is not None]
            no_emotion = [row['no_target_emotion_correct'] for row in rows if row['no_target_emotion_expected']]
            models[name][cohort] = {'cases': len(rows),
                'native_accepted': sum(row['native_status'] == 'source_bound_provisional' for row in rows),
                'inference_complete': sum(row['inference_status'] == 'complete' for row in rows),
                'usability': {'correct': sum(value is True for value in usability), 'evaluated': len(usability)},
                'focused_checkpoints': {'passed': sum(check['passed'] for check in checkpoints), 'evaluated': len(checkpoints)},
                'no_target_emotion': {'correct': sum(value is True for value in no_emotion), 'evaluated': len(no_emotion)}}
    result = {'contract': 'classifier-focused-accuracy-v1', 'production_eligible': False,
        'reference_sha256': hashlib.sha256(args.reference.read_bytes()).hexdigest(),
        'receipts_sha256': hashlib.sha256(args.receipts.read_bytes()).hexdigest(), 'models': models, 'cases': cases,
        'limitations': ['AI provisional source-only reference; no independent human adjudication.',
                        'Focused checkpoint coverage is not exhaustive claim precision/recall or factual verification.',
                        'Old controls are diagnostics; kept separate from fresh sources and controls.',
                        'Checkpoint matching requires 80% literal anchor coverage and the frozen relationship/kind/qualifiers.',
                        'Contract rejection counts as a checkpoint failure; unusable acquisitions remain explicit dispositions.']}
    with args.output.open('x') as output:
        json.dump(result, output, indent=2, ensure_ascii=False, allow_nan=False)
        output.write('\n')
    print(json.dumps(models, indent=2))


if __name__ == '__main__':
    main()
