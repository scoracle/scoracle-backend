"""Evaluate retained decisions against frozen source-only checks; AI provisional, not gold.

REFERENCE.jsonl OUTPUT.json RECEIPTS.jsonl [...]
Only the eight fresh controls have exhaustive diagnostic predicate expectations.
"""
import argparse
import hashlib
import json
from pathlib import Path


def expected(reference):
    direct = [c for c in reference['checks'] if c['relation'] == 'direct_subject']
    result = {}
    for scope in ('current', 'historical'):
        result[scope + '_emotion'] = any('emotion' in c['kind'] and scope in c.get('time', []) for c in direct)
    for kind in ('conditional_emotion', 'emotion_denial', 'report_denial', 'withdrawn'):
        result[kind] = any(kind in c['kind'] for c in direct)
    return result


def scores(row):
    if row['status'] != 'complete':
        return {}, None
    if 'answers' in row:
        answers = row['answers']
        return {key.split('.', 1)[1]: value['noul'] for key, value in answers.items()
                if key.startswith('diagnostic.')}, answers['extraction_usable']['noul'] >= .5
    response = row['response']
    return {a['label']: a['confidence'] for a in response['diagnostic']}, response['extraction_usable']['label'] == 'true'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('reference', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('receipts', type=Path, nargs='+')
    args = parser.parse_args()
    references = {r['article_id']: r for r in map(json.loads, args.reference.read_text().splitlines())}
    report = {'production_eligible': False, 'review_status': 'ai_provisional',
        'reference_sha256': hashlib.sha256(args.reference.read_bytes()).hexdigest(),
        'threshold': .5, 'models': {}, 'cases': [], 'limitations': [
            'Frozen source-only AI provisional expectations, without independent human adjudication.',
            'Only eight fresh controls support exhaustive six-predicate checks; this is not overall accuracy.',
            'Threshold 0.5 is a diagnostic convention, not Scoracle calibration or an admission policy.',
            'Document predicates do not qualify exact claims, speakers, event times or evidence spans.',
            'Possible real-source emotion and unknown acquisition usability remain unevaluated.',
            'No correctness claims for the full 51-label spectrum or coarse ordinal anchors.']}
    for path in args.receipts:
        rows = list(map(json.loads, path.read_text().splitlines()))
        assert len({r['article_id'] for r in rows}) == len(rows), 'duplicate canonical source'
        summary = {'receipt_sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
        for cohort in ('fresh_control', 'fresh_real', 'existing_diagnostic'):
            cases = []
            for row in rows:
                reference = references[row['article_id']]
                assert row['body_sha256'] == reference['body_sha256'] and row['target'] == reference['target'], 'source/target drift'
                if reference['cohort'] != cohort:
                    continue
                measured, usable = scores(row)
                checks = []
                if cohort == 'fresh_control':
                    for name, value in expected(reference).items():
                        score = measured.get(name)
                        checks.append({'predicate': name, 'expected': value, 'score': score,
                                       'correct': score is not None and (score >= .5) == value})
                cases.append({'article_id': row['article_id'], 'model': path.stem, 'cohort': cohort,
                    'usable_correct': None if reference['usable'] is None else usable is reference['usable'],
                    'diagnostics': checks, 'no_current_target_emotion_correct':
                        None if cohort != 'fresh_real' or reference['emotion'] != 'none' else
                        measured.get('current_emotion') is not None and measured['current_emotion'] < .5})
            diag = [c for case in cases for c in case['diagnostics']]
            usability = [c['usable_correct'] for c in cases if c['usable_correct'] is not None]
            no_emotion = [c['no_current_target_emotion_correct'] for c in cases if c['no_current_target_emotion_correct'] is not None]
            summary[cohort] = {'cases': len(cases), 'diagnostic_correct': sum(c['correct'] for c in diag),
                'diagnostic_evaluated': len(diag), 'usable_correct': sum(usability), 'usable_evaluated': len(usability),
                'no_current_target_emotion_correct': sum(no_emotion), 'no_current_target_emotion_evaluated': len(no_emotion)}
            summary[cohort]['always_absent_correct'] = sum(not c['expected'] for c in diag)
            summary[cohort]['confusion'] = {key: 0 for key in ('tp', 'fp', 'tn', 'fn', 'failed')}
            for c in diag:
                key = 'failed' if c['score'] is None else (
                    ('tp' if c['expected'] else 'fp') if c['score'] >= .5 else ('fn' if c['expected'] else 'tn'))
                summary[cohort]['confusion'][key] += 1
            report['cases'].extend(cases)
        report['models'][path.stem] = summary
    with args.output.open('x') as f:
        json.dump(report, f, indent=2, ensure_ascii=False, allow_nan=False)
        f.write('\n')
    print(json.dumps(report['models'], indent=2))


if __name__ == '__main__':
    main()
