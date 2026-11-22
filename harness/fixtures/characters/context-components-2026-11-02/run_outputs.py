"""One local generation per case; only Journalist, Insider and Influencer."""
import hashlib
import json
import os
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parent
REPO = ROOT.parents[2]

if __name__ == '__main__':
    binary = REPO / 'target/debug/examples/character_outputs'
    cases = sorted((ROOT / 'cases').glob('*.json'))
    assert len(cases) == 7
    for path in cases:
        fixture = json.loads(path.read_text())
        assert fixture['task'] in ('narratives', 'transfer', 'vibe')
        assert fixture['temperature'] == 0
    assert not (ROOT / 'outputs').exists(), 'preserve previous receipts'
    (ROOT / 'binary.json').write_text(json.dumps(dict(binary=str(binary),
        sha256=hashlib.sha256(binary.read_bytes()).hexdigest()), indent=2) + '\n')
    subprocess.run([str(binary), str(ROOT / 'outputs'), 'http://127.0.0.1:8081',
        'Qwen3.5-4B-Q4_K_M', *map(str, cases)], check=True,
        env={**os.environ, 'VOICE_NUM_CTX': '10240'})
