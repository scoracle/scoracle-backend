"""Only Journalist, Insider and Influencer; one generation per fixture, no retries."""
import hashlib
import json
import pathlib
import subprocess

ROOT=pathlib.Path(__file__).resolve().parent
REPO=ROOT.parents[2]

if __name__=='__main__':
    binary=REPO/'target/debug/examples/character_outputs'
    cases=sorted((ROOT/'cases').glob('*.json'))
    assert all(json.loads(p.read_text())['task'] in ['narratives','transfer','vibe'] for p in cases)
    (ROOT/'binary.json').write_text(json.dumps(dict(binary=str(binary),sha256=hashlib.sha256(binary.read_bytes()).hexdigest()),indent=2)+'\n')
    subprocess.run([str(binary),str(ROOT/'outputs'),'http://127.0.0.1:8081','Qwen3.5-4B-Q4_K_M',*[str(p) for p in cases]],check=True)
