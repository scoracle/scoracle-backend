"""Replay complete frozen native requests with prompt caching; no runtime configuration changes."""
import json
from pathlib import Path
import time
import urllib.request

ROOT = Path(__file__).resolve().parent
OUT = ROOT / 'v5-cache-probe'
OUT.mkdir()
URL = 'http://127.0.0.1:8081'


def call(path, body=None):
    req = urllib.request.Request(URL + path, data=None if body is None else json.dumps(body).encode(),
                                 headers={'Content-Type': 'application/json'})
    with urllib.request.urlopen(req, timeout=180) as response:
        return json.load(response)


def identity():
    model = call('/v1/models')['data'][0]
    return {key: model.get(key) for key in ('id', 'aliases', 'meta', 'architecture')}


serving = identity()
assert serving['id'] == 'Qwen3.5-4B-Q4_K_M'
for line in (ROOT / 'v5-qualification/results.jsonl').read_text().splitlines():
    old = json.loads(line)
    source = ROOT / 'v5-qualification' / (old['name'] + '-' + old['variant'])
    receipt = json.loads((source / 'receipt.json').read_text())
    request = receipt['request'].copy()
    assert request['cache_prompt'] is False and request['temperature'] == 0
    request['cache_prompt'] = True
    path = OUT / old['name']
    path.mkdir()
    (path / 'request.json').write_text(json.dumps(request))
    started = time.monotonic()
    response = call('/completion', request)
    elapsed = round((time.monotonic() - started) * 1000)
    (path / 'receipt.json').write_text(json.dumps(response))
    (path / 'output.json').write_text(response.get('content', ''))
    complete = (response.get('stop') is True and response.get('stop_type') == 'eos'
                and response.get('truncated') is False
                and response.get('tokens_evaluated') == len(request['prompt']))
    try:
        same = json.loads(response['content']) == json.loads((source / 'output.json').read_text())
    except (ValueError, KeyError):
        same = False
    record = {'name': old['name'], 'complete_input_and_output': complete,
              'same_parsed_output_as_uncached': same, 'wall_ms': elapsed,
              'tokens_evaluated': response.get('tokens_evaluated'),
              'output_tokens': response.get('tokens_predicted'), 'timings': response.get('timings'),
              'production_writes': False}
    with (OUT / 'results.jsonl').open('a') as output:
        output.write(json.dumps(record) + '\n')
    print(json.dumps(record), flush=True)
assert identity() == serving
