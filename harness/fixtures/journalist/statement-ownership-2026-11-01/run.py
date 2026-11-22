"""Run the frozen one-call comparison sequentially; never contacts the database or publishes."""
import hashlib,json,subprocess,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parent
WORKSPACE=ROOT.parents[2]
URL='http://127.0.0.1:8081'
MODEL='Qwen3.5-4B-Q4_K_M'
def identity():
 with urllib.request.urlopen(URL+'/v1/models',timeout=15) as r:return json.load(r)
def stable(value):
 value=json.loads(json.dumps(value))
 for model in value['data']:model.pop('created',None)
 return value
manifest=json.loads((ROOT/'manifest.json').read_text())
for name,digest in manifest['sha256'].items():assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==digest
before=identity();assert before['data'][0]['id']==MODEL
(ROOT/'model-before.json').write_text(json.dumps(before,indent=2)+'\n')
for pass_number,arms in [(1,['baseline','prose','structured']),(2,['structured','prose','baseline'])]:
 for arm in arms:
  subprocess.run([str(WORKSPACE/'target/debug/examples/journalist_assembly_trial'),
    str(ROOT/('cases-structured.jsonl' if arm=='structured' else 'cases-prose.jsonl')),
    str(ROOT/f'{arm}.txt'),str(ROOT/f'{arm}-pass-{pass_number}'),URL,MODEL,
    'statements' if arm=='structured' else 'baseline'],check=True)
after=identity();(ROOT/'model-after.json').write_text(json.dumps(after,indent=2)+'\n')
assert stable(before)==stable(after)
