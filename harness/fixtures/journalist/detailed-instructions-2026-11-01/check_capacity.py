"""Tokenize frozen production-loader packets with saved instructions; no inference or DB."""
import json
from pathlib import Path
import urllib.request
import sys

ROOT = Path(__file__).resolve().parent
URL = 'http://127.0.0.1:8081'


def request(path, body=None):
    req = urllib.request.Request(URL + path, data=None if body is None else json.dumps(body).encode(),
                                 headers={'Content-Type': 'application/json'})
    with urllib.request.urlopen(req, timeout=60) as response:
        return json.load(response)


assert len(sys.argv) == 3, 'INSTRUCTIONS.txt NEW_OUTPUT.json'
system = Path(sys.argv[1]).read_text().strip()
def serving_identity():
    model = request('/v1/models')['data'][0]
    return {key: model.get(key) for key in ('id', 'aliases', 'meta', 'architecture')}


models = serving_identity()
assert models['id'] == 'Qwen3.5-4B-Q4_K_M'
physical = min(slot['n_ctx'] for slot in request('/slots'))
rows = []
for line in (ROOT.parent / 'assembly-2026-11-01/live-capacity/results.jsonl').read_text().splitlines():
    old = json.loads(line)
    body = old['input']
    body['messages'][0]['content'] = system
    rendered = request('/apply-template', body)['prompt']
    assert all(m['content'] in rendered for m in body['messages'])
    tokens = request('/tokenize', {'content': rendered, 'add_special': True, 'parse_special': True})['tokens']
    reserved = old['reserved_output']
    rows.append({'subject': old['subject'], 'selected_reports': old['selected'],
                 'input_tokens': len(tokens), 'reserved_output': reserved, 'total': len(tokens) + reserved,
                 'default_limit': 7900, 'configured_limit': 10000, 'physical_limit': physical,
                 'fits_default': len(tokens) + reserved <= min(7900, physical),
                 'fits_configured': len(tokens) + reserved <= min(10000, physical), 'inference_calls': 0})
assert serving_identity() == models
with Path(sys.argv[2]).open('x') as output:
    json.dump(rows, output, indent=2)
print(json.dumps({'packets': len(rows), 'min_total': min(r['total'] for r in rows),
                  'max_total': max(r['total'] for r in rows),
                  'all_fit_default': all(r['fits_default'] for r in rows),
                  'all_fit_configured': all(r['fits_configured'] for r in rows)}))
