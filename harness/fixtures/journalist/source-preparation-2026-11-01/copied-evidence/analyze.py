"""Reuse native receipt audit, then independently verify every copied evidence field."""
import copy, json, statistics, subprocess, sys
from collections import Counter
from pathlib import Path
from contract import validate, self_check

ROOT = Path(__file__).resolve().parent
BASE = ROOT.parent / 'worked-writer'
encode = lambda obj: json.dumps(obj, ensure_ascii=False, separators=(',', ':'))
self_check()
subprocess.run([sys.executable, str(ROOT.parent / 'analyze.py'), str(ROOT)], check=True, stdout=subprocess.DEVNULL)
assert (ROOT / 'writer.txt').read_bytes() == (BASE / 'writer.txt').read_bytes()
checked = []
for cohort in ['development', 'validation']:
    controls = list(map(json.loads, (ROOT / f'{cohort}-control.jsonl').read_text().splitlines()))
    assert (ROOT / f'{cohort}-control.jsonl').read_bytes() == (BASE / f'{cohort}-control.jsonl').read_bytes()
    sources = list(map(json.loads, (ROOT / f'{cohort}-sources.jsonl').read_text().splitlines()))
    originals = list(map(json.loads, (BASE / f'{cohort}-sources.jsonl').read_text().splitlines()))
    for c, original in zip(sources, originals, strict=True):
        assert {k:v for k,v in c.items() if k != 'schema_raw'} == {k:v for k,v in original.items() if k != 'schema_raw'}
        schema = json.loads(c['schema_raw']); old = json.loads(original['schema_raw'])
        props = schema['properties']['statements']['items']['properties']
        choices = props['about']['items']['enum']
        assert props['claim'].pop('enum') == choices
        assert schema['properties']['unspecified']['items'].pop('enum') == choices
        assert schema == old and list(props) == list(old['properties']['statements']['items']['properties'])
        path = ROOT / f'{cohort}-preparation' / c['name']
        if not path.exists(): continue
        note = json.loads((path / 'output.json').read_text())
        text = json.loads(c['user_prompt'])['headline']; validate(note, text)
        values = list(note['unspecified'])
        for s in note['statements']:
            assert list(s) == list(props)
            values += s['about'] + s['qualifications'] + [s['claim']]
            if s['speaker'] is not None: values.append(s['speaker'])
        assert all(value in choices and value in text for value in values)
        receipt = json.loads((path / 'receipt.json').read_text())['request']
        assert '"json_schema":' + c['schema_raw'] in receipt['request_body_raw']
        checked.append({'cohort':cohort, 'article_id':int(c['name']), 'all_fields_copied':True,
                        'statements':len(note['statements']),
                        'whole_headline_claim':any(s['claim'] == text for s in note['statements'])})
    for arm in ['verbatim', 'automatic']:
        file = ROOT / f'{cohort}-{arm}.jsonl'
        if not file.exists(): continue
        rows = list(map(json.loads, file.read_text().splitlines())); assert len(rows) == len(controls)
        for c, control in zip(rows, controls, strict=True):
            packet = json.loads(c['user_prompt'])
            for group in packet['fresh']:
                for source in group['reports']:
                    note = source.pop('preparation')
                    article = c['parts']['reports'][int(source['report_key'].split('_')[1]) - 1]
                    if arm == 'verbatim':
                        assert note == {'statements':[{'claim':source['headline']}], 'unspecified':[]}
                    else:
                        expected = json.loads((ROOT / f'{cohort}-preparation' / str(article['id']) / 'output.json').read_text())
                        assert encode(note) == encode(expected)
            assert encode(packet) == control['user_prompt']
            clean = copy.deepcopy(c); clean['user_prompt'] = control['user_prompt']; assert clean == control
summary = json.loads((ROOT / 'measurements.json').read_text())
summary['all_field_copy_checks'] = checked
summary['writer_unchanged'] = True
summary['verbatim_control_preparation_model_calls'] = 0
summary['historical_direct_writer_seconds'] = json.loads((BASE / 'measurements.json').read_text())['runs']['development-control']['total_ms'] / 1000
(ROOT / 'measurements.json').write_text(json.dumps(summary, indent=2) + '\n')
if (ROOT / 'review.json').exists() and (ROOT / 'selection.json').exists():
    review = json.loads((ROOT / 'review.json').read_text())
    selection = json.loads((ROOT / 'selection.json').read_text())
    assert len(review['source_reviews']) == 29 and len(review['writer_reviews']) == 12
    for arm in ['verbatim', 'automatic']:
        counts = Counter(r['assessment'] for r in review['writer_reviews'] if r['arm'] == arm)
        assert selection['counts'][arm] == {'clearly_faithful': counts['faithful'], 'clarity_concern': counts['clarity_concern'], 'failed': counts['unfaithful']}
    r = summary['runs']; total = r['development-preparation']['total_ms'] + r['development-automatic']['total_ms']
    comparison = {
        'new_calls': summary['calls'], 'new_call_wall_seconds': summary['total_wall_seconds'],
        'automatic_preparation_plus_writing_seconds': total / 1000,
        'automatic_ratio_to_same_session_verbatim': total / r['development-verbatim']['total_ms'],
        'automatic_ratio_to_historical_direct_writer': total / (summary['historical_direct_writer_seconds'] * 1000),
        'automatic_input_tokens': r['development-preparation']['input_tokens'] + r['development-automatic']['input_tokens'],
        'automatic_output_tokens': r['development-preparation']['output_tokens'] + r['development-automatic']['output_tokens'],
        'statements': sum(c['statements'] for c in checked if c['cohort'] == 'development'),
        'whole_headline_claim_sources': sum(c['whole_headline_claim'] for c in checked if c['cohort'] == 'development'),
        'all_text_field_copy_valid_sources': len(checked),
        'source_preparation_investigation_cumulative_calls': 174 + summary['calls'],
        'source_preparation_investigation_cumulative_call_seconds': round(942.639 + summary['total_wall_seconds'], 3),
        'combined_packet_median_seconds': statistics.median(c['total_ms'] for c in summary['combined_automatic'] if c['cohort'] == 'development') / 1000,
        'timing_limit': 'Sequential observed times with prefix caching; no interleaved repeats, full-workload throughput or independent semantic adjudication.'}
    (ROOT / 'comparison.json').write_text(json.dumps(comparison, indent=2) + '\n')
print(json.dumps({k:v for k,v in summary.items() if k not in ['records','all_field_copy_checks']}, indent=2))
