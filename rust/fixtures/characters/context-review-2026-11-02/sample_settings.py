"""Replay four verified requests with Qwen's published non-thinking sampler settings."""
import copy
import json
import pathlib
import subprocess
import sys
import time
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent
URL = 'http://127.0.0.1:8081'
MODEL = 'Qwen3.5-4B-Q4_K_M'
SETTINGS = dict(temperature=0.7, top_p=0.8, top_k=20, min_p=0.0,
                presence_penalty=1.5, repeat_penalty=1.0, seed=42)


def call(path, body=None):
    request = urllib.request.Request(URL + path,
        data=None if body is None else json.dumps(body).encode(),
        headers={'Content-Type': 'application/json'})
    with urllib.request.urlopen(request, timeout=300) as response:
        return json.load(response)


def save(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')


def stable_inventory(value):
    result = copy.deepcopy(value)
    for model in result['data']:
        model.pop('created', None)  # llama.cpp reports request time here.
    return result


def main(output_name='sampled', offset=0):
    root = ROOT / output_name
    root.mkdir()
    before, props, slots = call('/v1/models'), call('/props'), call('/slots')
    assert len(before['data']) == 1 and before['data'][0]['id'] == MODEL
    save(root / 'model-before.json', before)
    save(root / 'props.json', props)
    save(root / 'run.json', dict(settings=SETTINGS, retries=0, concurrency=1,
        database_access=False, publishes=False,
        design='Four worked-example requests; only sampler parameters differ. One fixed seed is diagnostic, not a stability estimate.',
        source='https://huggingface.co/Qwen/Qwen3.5-4B'))
    cases = sorted((ROOT / 'worked').glob('*/prepared.json'))[offset:4]
    assert len(cases) == 4 - offset
    with (root / 'results.jsonl').open('w') as ledger:
        for path in cases:
            original = path.parent
            destination = root / original.name
            destination.mkdir()
            prepared = json.loads(path.read_text())
            coverage = prepared['coverage']
            assert all(props[k] == coverage['provider'][k] for k in ['model_path', 'chat_template', 'build_info'])
            request = copy.deepcopy(prepared['request'])
            assert request['prompt'] == coverage['token_ids']
            assert len(request['prompt']) + request['n_predict'] <= min(7900, *(s['n_ctx'] for s in slots))
            assert (original / 'system.txt').read_text() in coverage['rendered_prompt']
            assert (original / 'user.txt').read_text() in coverage['rendered_prompt']
            request.update(SETTINGS)
            assert {k:v for k,v in request.items() if k not in SETTINGS} == {
                k:v for k,v in prepared['request'].items() if k not in SETTINGS}
            save(destination / 'request.json', request)
            started = time.monotonic()
            response = call('/completion', request)
            save(destination / 'response.json', response)
            assert response['stop'] and response['stop_type'] == 'eos' and response['truncated'] is False
            assert response['tokens_evaluated'] == len(request['prompt'])
            (destination / 'output.json').write_text(response['content'])
            after_models = call('/v1/models')
            save(destination / 'model-after.json', after_models)
            assert stable_inventory(after_models) == stable_inventory(before)
            after_props = call('/props')
            assert all(after_props[k] == props[k] for k in ['model_path', 'chat_template', 'build_info'])
            verdict = json.loads(subprocess.check_output([
                str(ROOT.parents[2] / 'target/release/examples/character_prompt_trial'), '--validate',
                str(original / 'fixture.json'), str(destination / 'output.json')], text=True))
            result = dict(name=original.name, arm='sampled', wall_ms=round((time.monotonic()-started)*1000),
                input_tokens=response['tokens_evaluated'], output_tokens=response['tokens_predicted'],
                status='accepted' if verdict['accepted'] else 'rejected', parser_detail=verdict['parser_detail'])
            save(destination / 'result.json', result)
            ledger.write(json.dumps(result) + '\n')
            ledger.flush()
            print(result['name'], result['status'], flush=True)
    save(root / 'model-after.json', call('/v1/models'))


if __name__ == '__main__':
    assert stable_inventory({'data':[{'id':'a','created':1}]}) == stable_inventory({'data':[{'id':'a','created':2}]})
    main(*([sys.argv[1], int(sys.argv[2])] if len(sys.argv) == 3 else []))
