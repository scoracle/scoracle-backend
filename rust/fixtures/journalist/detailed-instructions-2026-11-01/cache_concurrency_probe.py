"""Four-slot prefix-reuse comparison; the warm-up inputs differ from the measured inputs."""
from concurrent.futures import ThreadPoolExecutor
import json
from pathlib import Path
import time
import urllib.request

ROOT = Path(__file__).resolve().parent
OUT = ROOT / 'v5-cache-concurrency'
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


def cases(run):
    return [ROOT / run / (r['name'] + '-' + r['variant']) for r in
            map(json.loads, (ROOT / run / 'results.jsonl').read_text().splitlines())]


serving = identity()
assert serving['id'] == 'Qwen3.5-4B-Q4_K_M'
slots = call('/slots')
assert len(slots) == 4
physical = min(s['n_ctx'] for s in slots)
targets = cases('v5-workload') + [cases('v5-development')[0]]
primers = cases('v5-qualification')[:4]
assert set(targets).isdisjoint(primers)


def run_one(source, mode, cached):
    path = OUT / mode / source.name
    path.mkdir(parents=True)
    request = json.loads((source / 'receipt.json').read_text())['request'].copy()
    request['cache_prompt'] = cached
    assert len(request['prompt']) + request['n_predict'] <= min(7900, physical)
    (path / 'request.json').write_text(json.dumps(request))
    started = time.monotonic()
    response = call('/completion', request)
    elapsed = round((time.monotonic() - started) * 1000)
    (path / 'receipt.json').write_text(json.dumps(response))
    (path / 'output.json').write_text(response.get('content', ''))
    complete = (response.get('stop') is True and response.get('stop_type') == 'eos'
                and response.get('truncated') is False
                and response.get('tokens_evaluated') == len(request['prompt']))
    record = {'case': source.name, 'mode': mode, 'complete_input_and_output': complete,
              'wall_ms': elapsed, 'slot': response.get('id_slot'), 'timings': response.get('timings')}
    if mode == 'cached':
        baseline = OUT / 'uncached' / source.name / 'output.json'
        try:
            record['same_parsed_output_as_uncached'] = json.loads(response['content']) == json.loads(baseline.read_text())
        except (ValueError, KeyError):
            record['same_parsed_output_as_uncached'] = False
    return record


for mode, selected, cached in [('uncached', targets, False), ('primer', primers, True), ('cached', targets, True)]:
    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=4) as executor:
        records = list(executor.map(lambda source: run_one(source, mode, cached), selected))
    batch = {'mode': mode, 'batch_wall_ms': round((time.monotonic() - started) * 1000), 'requests': records}
    with (OUT / 'results.jsonl').open('a') as output:
        output.write(json.dumps(batch) + '\n')
    print(json.dumps(batch), flush=True)
    assert len({r['slot'] for r in records}) == 4, 'four distinct slots were not exercised'
    assert all(r['complete_input_and_output'] for r in records)
assert identity() == serving
