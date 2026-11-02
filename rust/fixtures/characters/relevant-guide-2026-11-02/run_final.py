"""One generation per frozen case after the documented guide refinement."""
import hashlib
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent
REPO = ROOT.parents[2]
binary = REPO / 'target/debug/examples/character_outputs'

if __name__ == '__main__':
    arm = 'followup' if sys.argv[1:] == ['followup'] else 'final'
    root = ROOT / arm
    root.mkdir()
    (root / 'run.json').write_text(json.dumps(dict(
        model='Qwen3.5-4B-Q4_K_M', temperature=0, thinking=False, concurrency=1, retries=0,
        database_access=False, publishes=False, binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
        design='Refined concise guides on the same 12 development cases. Baseline from batches 1/2 retained.'
    ), indent=2)+'\n')
    with (root / 'results.jsonl').open('w') as ledger:
        for case in json.loads((ROOT / 'selection.json').read_text()):
            if arm == 'followup' and case['task'] not in ['rating','vibe','oracle','momentum']:
                continue
            target = root / case['name']
            subprocess.run([str(binary),str(target),'http://127.0.0.1:8081','Qwen3.5-4B-Q4_K_M',str(ROOT/'cases/candidate'/(case['name']+'.json'))], check=True)
            row = json.loads((target/'01/result.json').read_text());row.update(arm=arm,case_name=case['name'])
            ledger.write(json.dumps(row,ensure_ascii=False)+'\n');ledger.flush()
