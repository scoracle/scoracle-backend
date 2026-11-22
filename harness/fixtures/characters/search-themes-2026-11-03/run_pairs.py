"""Mac-only paired replay. Raw receipts stay in the supplied private directory."""
import json
import os
from pathlib import Path
import subprocess
import sys
import urllib.request

root, baseline, candidate = map(Path, sys.argv[1:])
assert root.is_absolute() and 'scoracle-worker/experiments/' in str(root)
assert json.load(urllib.request.urlopen('http://127.0.0.1:8081/health', timeout=5))['status'] == 'ok'
paths = sorted((root / 'baseline-cases').glob('*.json'))
assert len(paths) == 18
output = root / 'paired'
output.mkdir()
results = []
for index, path in enumerate(paths):
    other = root / 'candidate-cases' / path.name
    assert json.loads(path.read_text())['parts'] == json.loads(other.read_text())['parts']
    variants = [('baseline', baseline, path), ('candidate', candidate, other)]
    if index % 2:
        variants.reverse()
    for variant, binary, fixture in variants:
        run = output / f'{path.stem}-{variant}'
        print(f'{index + 1}/18 {variant} {path.stem}', flush=True)
        # Exactly one invocation per case and variant; no retries or database writes.
        subprocess.run([str(binary), str(run), 'http://127.0.0.1:8081',
                        'Qwen3.5-4B-Q4_K_M', str(fixture)],
                       env={**os.environ, 'VOICE_NUM_CTX': '10000'}, check=True)
        record = json.loads((run / '01/result.json').read_text())
        record['variant'] = variant
        record['receipt_dir'] = str(run / '01')
        results.append(record)
        (output / 'summary.json').write_text(json.dumps(results, indent=2) + '\n')
print(f'Finished {len(results)} paired generations', flush=True)
