"""Verify saved replay receipts and summarize measurements; no inference or DB access."""
import hashlib
import json
import statistics
from pathlib import Path

ROOT = Path(__file__).resolve().parent
manifest = json.loads((ROOT/'manifest.json').read_text())
system = (ROOT/'instructions.txt').read_text()
assert hashlib.sha256(system.encode()).hexdigest() == manifest['instructions_sha256']
identities = [json.loads((ROOT/f'model-{phase}.json').read_text()) for phase in ['before', 'after']]
for identity in identities:
    # llama-server emits the response time as created; compare actual model metadata.
    for model in identity['data']:
        model.pop('created', None)
assert identities[0] == identities[1]
records, pairs = [], {}
for slug in ['ac-milan', 'chicago-bears', 'la-clippers']:
    cases = {c['name']: c for c in map(json.loads, (ROOT/f'{slug}-cases.jsonl').read_text().splitlines())}
    results = list(map(json.loads, (ROOT/f'{slug}-runs/results.jsonl').read_text().splitlines()))
    assert len(cases) == len(results) == 8
    for result in results:
        case = cases[result['name']]
        path = ROOT/f'{slug}-runs'/f"{result['name']}-{result['variant']}"
        assembled = json.loads((path/'input.json').read_text())
        receipt = json.loads((path/'receipt.json').read_text())
        request, raw = receipt['request'], json.loads(receipt['raw_response'])
        output = json.loads((path/'output.json').read_text())
        packet = json.loads(assembled['user_prompt'])
        size = len(case['parts']['reports'])
        assert assembled['parts'] == case['parts'] and assembled['system'] == system
        assert request['temperature'] == 0 and request['cache_prompt'] is True
        assert assembled['thinking'] is False and assembled['num_ctx'] == 7900
        assert request['n_predict'] == assembled['num_predict'] == (900 if size == 1 else 2100)
        assert raw['stop'] is True and raw['stop_type'] == 'eos' and raw['truncated'] is False
        assert raw['tokens_evaluated'] == len(request['prompt']) == result['input_tokens']
        assert result['input_tokens'] + request['n_predict'] <= assembled['num_ctx']
        assert result['output_tokens'] == raw['tokens_predicted'] < request['n_predict']
        assert json.loads(raw['content']) == output and len(packet['fresh']) == size
        anchor_entries = [i for i,n in enumerate(output['narratives']) if 'report_1' in n['report_keys']]
        record = {**result, 'subject': case['parts']['subject']['name'], 'report_count': size,
                  'anchor_cited_in_entries': anchor_entries, 'entry_count': len(output['narratives']),
                  'reserved_output': request['n_predict'], 'full_input_evaluated_and_eos': True,
                  'native_timings': raw['timings']}
        records.append(record)
        pairs.setdefault((slug,size), []).append((request, output))
assert len(records) == 24 and len(pairs) == 12
repeats = []
for (slug,size), values in pairs.items():
    assert len(values) == 2 and values[0][0] == values[1][0]
    repeats.append({'subject': slug, 'report_count': size, 'identical_requests': True,
                    'identical_parsed_output': values[0][1] == values[1][1]})
summary = {'runs': len(records), 'parser_and_expect_accepted': sum(r['accepted'] for r in records),
           'all_receipts_complete': True, 'repeats': repeats, 'by_size': [], 'records': records}
for size in [1,4,12,40]:
    group = [r for r in records if r['report_count'] == size]
    summary['by_size'].append({'report_count': size, 'runs': len(group),
        'parser_and_expect_accepted': sum(r['accepted'] for r in group),
        'anchor_cited_first': sum(r['anchor_cited_in_entries'] == [0] for r in group),
        'wall_ms_median': statistics.median(r['wall_ms'] for r in group),
        'wall_ms_range': [min(r['wall_ms'] for r in group),max(r['wall_ms'] for r in group)],
        'input_tokens_range': [min(r['input_tokens'] for r in group),max(r['input_tokens'] for r in group)],
        'output_tokens_range': [min(r['output_tokens'] for r in group),max(r['output_tokens'] for r in group)]})
(ROOT/'measurements.json').write_text(json.dumps(summary,ensure_ascii=False,indent=2)+'\n')
print(json.dumps({k:v for k,v in summary.items() if k != 'records'},ensure_ascii=False,indent=2))
