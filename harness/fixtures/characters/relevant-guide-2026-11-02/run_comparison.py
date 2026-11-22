"""Paired development replay, sequential requests, no retries or publication."""
import hashlib
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent
REPO = ROOT.parents[2]
BASE = ROOT.parent / 'context-guide-2026-11-02'
BINS = {'baseline': pathlib.Path('/private/tmp/scoracle-6a87e76d-character_outputs'),
        'candidate': REPO / 'target/debug/examples/character_outputs'}

if __name__ == '__main__':
    # Batch 1: one case per role. Batch 2: remaining types plus dated comparisons.
    batch = int(sys.argv[1])
    selected = [0, 2, 4, 6, 7, 9] if batch == 1 else [1, 3, 5, 8, 10, 11]
    root = ROOT / f'batch-{batch}'
    root.mkdir()
    (root / 'run.json').write_text(json.dumps(dict(
        starting_commit='6a87e76db7ef4b8c64bcd6ac4514efd42278bca0',
        model='Qwen3.5-4B-Q4_K_M', temperature=0, thinking=False, concurrency=1, retries=0,
        database_access=False, publishes=False,
        design='Same evidence; alternating arm order. Development cases, not held-out accuracy.',
        binaries={a:dict(path=str(p), sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for a,p in BINS.items()}
    ), indent=2) + '\n')
    cases = json.loads((ROOT / 'selection.json').read_text())
    with (root / 'results.jsonl').open('w') as ledger:
        for i, index in enumerate(selected):
            case = cases[index]
            for arm in (['baseline','candidate'] if i % 2 == 0 else ['candidate','baseline']):
                source = BASE / 'cases/refined' if arm == 'baseline' else ROOT / 'cases/candidate'
                target = root / arm / case['name']
                target.parent.mkdir(exist_ok=True)
                subprocess.run([str(BINS[arm]),str(target),'http://127.0.0.1:8081','Qwen3.5-4B-Q4_K_M',str(source / (case['name']+'.json'))], check=True)
                row = json.loads((target / '01/result.json').read_text())
                row.update(arm=arm, case_name=case['name'])
                ledger.write(json.dumps(row,ensure_ascii=False)+'\n'); ledger.flush()
