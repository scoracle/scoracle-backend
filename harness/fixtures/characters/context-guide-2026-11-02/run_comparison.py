"""One complete local generation per case/arm; no database access or publication."""
import hashlib
import json
import pathlib
import subprocess
import time

ROOT=pathlib.Path(__file__).resolve().parent
REPO=ROOT.parents[2]
BINARIES={'control':pathlib.Path('/private/tmp/scoracle-before-context-guide-character_outputs'),
          'candidate':REPO/'target/release/examples/character_outputs'}


def main():
    root=ROOT/'trial';root.mkdir()
    cases=json.loads((ROOT/'selection.json').read_text())
    (root/'run.json').write_text(json.dumps(dict(model='Qwen3.5-4B-Q4_K_M',temperature=0,
        concurrency=1,retries=0,database_access=False,publishes=False,
        design='Two cases per character; paired same fresh data, alternating arm order. One dated-comparison case is synthetic.',
        binaries={arm:dict(path=str(p),sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for arm,p in BINARIES.items()}),indent=2)+'\n')
    started=time.monotonic()
    with (root/'results.jsonl').open('w') as ledger:
        for i,case in enumerate(cases):
            for arm in (['control','candidate'] if i%2==0 else ['candidate','control']):
                target=root/arm/case['name'];target.parent.mkdir(exist_ok=True)
                subprocess.run([str(BINARIES[arm]),str(target),'http://127.0.0.1:8081','Qwen3.5-4B-Q4_K_M',
                    str(ROOT/'cases'/arm/(case['name']+'.json'))],check=True)
                row=json.loads((target/'01/result.json').read_text());row['arm']=arm
                ledger.write(json.dumps(row,ensure_ascii=False)+'\n');ledger.flush()
    (root/'elapsed.json').write_text(json.dumps(dict(seconds=time.monotonic()-started))+'\n')


if __name__=='__main__':main()
