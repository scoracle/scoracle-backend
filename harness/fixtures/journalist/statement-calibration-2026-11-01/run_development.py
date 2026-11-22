import hashlib,json,subprocess,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parent
BIN=ROOT.parents[2]/'target/debug/examples/journalist_assembly_trial'
URL='http://127.0.0.1:8081';MODEL='Qwen3.5-4B-Q4_K_M'
def identity():
 with urllib.request.urlopen(URL+'/v1/models',timeout=15) as response:return json.load(response)
manifest=json.loads((ROOT/'development-manifest.json').read_text())
for name,digest in manifest['sha256'].items():assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==digest
assert all(r['fits'] for r in json.loads((ROOT/'development-admission.json').read_text()))
before=identity();assert before['data'][0]['id']==MODEL
(ROOT/'model-before.json').write_text(json.dumps(before,indent=2)+'\n')
for arm in ['control','examples','gaps']:
 subprocess.run([str(BIN),str(ROOT/('cases-gaps.jsonl' if arm=='gaps' else 'cases.jsonl')),str(ROOT/f'{arm}.txt'),str(ROOT/f'{arm}-development'),URL,MODEL,'statements'],check=True)
after=identity();(ROOT/'model-after-development.json').write_text(json.dumps(after,indent=2)+'\n')
for value in [before,after]:
 for model in value['data']:model.pop('created',None)
assert before==after
