"""One local generation for each refined field guide; identical evidence and settings."""
import hashlib,json,pathlib,subprocess,time
ROOT=pathlib.Path(__file__).resolve().parent
REPO=ROOT.parents[2]
binary=REPO/'target/release/examples/character_outputs'
root=ROOT/'refinement';root.mkdir()
(root/'run.json').write_text(json.dumps(dict(model='Qwen3.5-4B-Q4_K_M',temperature=0,concurrency=1,retries=0,database_access=False,publishes=False,binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),design='Refined path-to-meaning guides on the same 12 development inputs; earlier paired control retained, no new holdout claim.'),indent=2)+'\n')
started=time.monotonic()
with (root/'results.jsonl').open('w') as ledger:
 for case in json.loads((ROOT/'selection.json').read_text()):
  target=root/case['name']
  subprocess.run([str(binary),str(target),'http://127.0.0.1:8081','Qwen3.5-4B-Q4_K_M',str(ROOT/'cases/refined'/(case['name']+'.json'))],check=True)
  row=json.loads((target/'01/result.json').read_text());row['arm']='refined'
  ledger.write(json.dumps(row,ensure_ascii=False)+'\n');ledger.flush()
(root/'elapsed.json').write_text(json.dumps(dict(seconds=time.monotonic()-started))+'\n')
