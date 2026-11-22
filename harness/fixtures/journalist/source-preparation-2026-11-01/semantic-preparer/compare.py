"""Audit the actual schema wire order and compare all costs without re-running inference."""
import hashlib, json
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
encode = lambda value: json.dumps(value, ensure_ascii=False, separators=(',', ':'))
baseline = ROOT / 'worked-writer'
base_cost = json.loads((baseline / 'measurements.json').read_text())
direct_ms = base_cost['runs']['development-control']['total_ms']
summary = {'comparisons': {}, 'historical_direct_writer_seconds': direct_ms / 1000,
           'timing_limit': 'Sequential observed call times with prefix caching; historical controls were not rerun or interleaved. Not a daily-capacity benchmark.'}
for name in ['worked-writer', 'semantic-preparer', 'claim-first']:
    root = ROOT / name
    for file, digest in json.loads((root / 'manifest.json').read_text())['sha256'].items():
        assert hashlib.sha256((root / file).read_bytes()).hexdigest() == digest
    assert (root / 'writer.txt').read_bytes() == (baseline / 'writer.txt').read_bytes()
    for file in ['development-control.jsonl', 'validation-control.jsonl']:
        assert (root / file).read_bytes() == (baseline / file).read_bytes()
    rows = list(map(json.loads, (root / 'development-sources.jsonl').read_text().splitlines()))
    originals = list(map(json.loads, (baseline / 'development-sources.jsonl').read_text().splitlines()))
    counts = []
    for case, original in zip(rows, originals, strict=True):
        assert {k: v for k, v in case.items() if k != 'schema_raw'} == {k: v for k, v in original.items() if k != 'schema_raw'}
        assert json.loads(case['schema_raw']) == json.loads(original['schema_raw'])
        path = root / 'development-preparation' / case['name']
        request = json.loads((path / 'receipt.json').read_text())['request']
        wire = json.loads(request['request_body_raw'])
        props = json.loads(case['schema_raw'])['properties']['statements']['items']['properties']
        assert '"json_schema":' + case['schema_raw'] in request['request_body_raw']
        assert list(wire['json_schema']['properties']['statements']['items']['properties']) == list(props)
        output = json.loads((path / 'output.json').read_text())
        assert all(list(s) == list(props) for s in output['statements'])
        counts.append(len(output['statements']))
    for case in map(json.loads, (root / 'development-automatic.jsonl').read_text().splitlines()):
        controls = list(map(json.loads, (root / 'development-control.jsonl').read_text().splitlines()))
        original = next(c for c in controls if c['name'] == case['name'])
        packet = json.loads(case['user_prompt'])
        for group in packet['fresh']:
            for source in group['reports']:
                article = case['parts']['reports'][int(source['report_key'].split('_')[1]) - 1]
                note = json.loads((root / 'development-preparation' / str(article['id']) / 'output.json').read_text())
                assert encode(source.pop('preparation')) == encode(note)
        assert encode(packet) == original['user_prompt']
    measurements = json.loads((root / 'measurements.json').read_text())
    prep = measurements['runs']['development-preparation']
    writer = measurements['runs']['development-automatic']
    review = json.loads((root / 'review.json').read_text())
    writers = [r for r in review['writer_reviews'] if r['arm'] == 'automatic']
    assert len(rows) == len(counts) == 29 and len(writers) == 6
    total = prep['total_ms'] + writer['total_ms']
    summary['comparisons'][name] = {
        'writer_assessments': dict(Counter(r['assessment'] for r in writers)),
        'statements': sum(counts), 'sources_with_multiple_statements': sum(n > 1 for n in counts),
        'preparation_seconds': prep['total_ms'] / 1000, 'writer_seconds': writer['total_ms'] / 1000,
        'combined_seconds': total / 1000, 'ratio_to_historical_direct_writer': total / direct_ms,
        'input_tokens': prep['input_tokens'] + writer['input_tokens'],
        'output_tokens': prep['output_tokens'] + writer['output_tokens'],
        'preparation_output_tokens': prep['output_tokens'], 'actual_schema_order_verified': True}
assert (ROOT / 'claim-first/preparer.txt').read_bytes() == (ROOT / 'semantic-preparer/preparer.txt').read_bytes()
summary['new_calls'] = 70
summary['new_call_wall_seconds'] = round(sum(summary['comparisons'][n]['combined_seconds'] for n in ['semantic-preparer', 'claim-first']), 3)
summary['cumulative_source_preparation_calls'] = 104 + summary['new_calls']
summary['cumulative_source_preparation_call_wall_seconds'] = round(606.892 + summary['new_call_wall_seconds'], 3)
for name in ['semantic-preparer', 'claim-first']:
    assert not json.loads((ROOT / name / 'selection.json').read_text())['qualifies_for_followup']
    assert not (ROOT / name / 'validation-preparation').exists()
    assert not (ROOT / name / 'repeat-automatic').exists()
(ROOT / 'semantic-preparer/comparison.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary, indent=2))
