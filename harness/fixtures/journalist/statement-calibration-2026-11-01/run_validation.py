"""Run the predeclared twelve follow-up calls; no changes or retries after selection."""
import hashlib,json,subprocess,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parent
BIN=ROOT.parents[2]/'target/debug/examples/journalist_assembly_trial'
URL='http://127.0.0.1:8081';MODEL='Qwen3.5-4B-Q4_K_M'
def request(path,body=None):
 data=None if body is None else json.dumps(body).encode()
 req=urllib.request.Request(URL+path,data=data,headers={'Content-Type':'application/json'})
 with urllib.request.urlopen(req,timeout=30) as response:return json.load(response)
for name in ['development-manifest.json','validation-manifest.json']:
 for file,digest in json.loads((ROOT/name).read_text())['sha256'].items():
  assert hashlib.sha256((ROOT/file).read_bytes()).hexdigest()==digest
assert json.loads((ROOT/'selection.json').read_text())['selected']=='examples'
base=json.loads((ROOT/'live-capture/results.jsonl').read_text().splitlines()[0])['input']
admission=[]
for arm in ['control','examples']:
 for case in map(json.loads,(ROOT/'validation.jsonl').read_text().splitlines()):
  base['messages'][0]['content']=(ROOT/f'{arm}.txt').read_text();base['messages'][1]['content']=case['user_prompt']
  prompt=request('/apply-template',base)['prompt']
  tokens=request('/tokenize',{'content':prompt,'add_special':True,'parse_special':True})['tokens']
  admission.append({'arm':arm,'name':case['name'],'input_tokens':len(tokens),'reserved_output':2100,'total':len(tokens)+2100,'fits':len(tokens)+2100<=7900})
(ROOT/'validation-admission.json').write_text(json.dumps(admission,indent=2)+'\n')
assert all(r['fits'] for r in admission)
for arm,cases,directory in [('examples','cases.jsonl','examples-repeat'),('control','validation.jsonl','control-validation'),('examples','validation.jsonl','examples-validation')]:
 subprocess.run([str(BIN),str(ROOT/cases),str(ROOT/f'{arm}.txt'),str(ROOT/directory),URL,MODEL,'statements'],check=True)
final=request('/v1/models');(ROOT/'model-final.json').write_text(json.dumps(final,indent=2)+'\n')
before=json.loads((ROOT/'model-before.json').read_text())
for value in [before,final]:
 for model in value['data']:model.pop('created',None)
assert final==before
