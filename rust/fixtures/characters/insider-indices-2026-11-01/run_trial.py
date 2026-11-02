"""One paired, counterbalanced sequential trial. No database access or publication."""
import hashlib
import json
import pathlib
import subprocess
import time
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent
REPO = ROOT.parents[2]
MODEL = 'Qwen3.5-4B-Q4_K_M'
URL = 'http://127.0.0.1:8081'
BINARIES = {'control': pathlib.Path('/private/tmp/scoracle-insider-unindexed-outputs'),
            'indexed': REPO / 'target/release/examples/character_outputs'}


def get(path):
    with urllib.request.urlopen(URL + path, timeout=10) as response:
        return json.load(response)


def main():
    root = ROOT / 'trial'
    root.mkdir()
    before, props = get('/v1/models'), get('/props')
    assert MODEL in [m['id'] for m in before['data']]
    (root / 'model-before.json').write_text(json.dumps(before, indent=2) + '\n')
    (root / 'server-props.json').write_text(json.dumps(props, indent=2) + '\n')
    (root / 'run.json').write_text(json.dumps(dict(model=MODEL, url=URL, temperature=0,
        concurrency=1, retries=0, database_access=False, publishes=False,
        binaries={arm: dict(path=str(p), sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for arm, p in BINARIES.items()},
        design='Twelve frozen cases, both arms once, alternating control-first and indexed-first. Only report_index input fields differ.'), indent=2) + '\n')
    cases = json.loads((ROOT / 'case-provenance.json').read_text())
    assert len(cases) == 12
    started = time.monotonic()
    with (root / 'results.jsonl').open('w') as ledger:
        for index, case in enumerate(cases):
            for arm in (['control', 'indexed'] if index % 2 == 0 else ['indexed', 'control']):
                target = root / arm / case['case']
                target.parent.mkdir(exist_ok=True)
                fixture = ROOT / 'cases' / arm / (case['case'] + '.json')
                subprocess.run([str(BINARIES[arm]), str(target), URL, MODEL, str(fixture)], check=True)
                result = json.loads((target / '01/result.json').read_text())
                result.update(arm=arm, subject=case['subject'], order=len(list(root.glob('*/*/01/result.json'))))
                ledger.write(json.dumps(result, ensure_ascii=False) + '\n')
                ledger.flush()
    after = get('/v1/models')
    assert before['data'][0]['id'] == after['data'][0]['id']
    (root / 'model-after.json').write_text(json.dumps(after, indent=2) + '\n')
    (root / 'elapsed.json').write_text(json.dumps(dict(seconds=time.monotonic() - started)) + '\n')


if __name__ == '__main__':
    main()
