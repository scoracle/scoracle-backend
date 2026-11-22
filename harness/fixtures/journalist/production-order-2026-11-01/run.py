import hashlib,importlib.util,json,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('boundary_run',ROOT.parent/'context-boundaries-2026-11-01/run.py');base=importlib.util.module_from_spec(spec);spec.loader.exec_module(base)
for name,digest in json.loads((ROOT/'manifest.json').read_text())['sha256'].items():assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==digest
before=base.request('/v1/models');assert before['data'][0]['id']==base.MODEL
(ROOT/'model-before.json').write_text(json.dumps(before,indent=2)+'\n')
admissions=[]
for c in map(json.loads,(ROOT/'cases.jsonl').read_text().splitlines()):
 body={'messages':[{'role':'system','content':(ROOT/'system.txt').read_text()},{'role':'user','content':c['user_prompt']}],'add_generation_prompt':True,'chat_template_kwargs':{'enable_thinking':False}}
 rendered=base.request('/apply-template',body)['prompt'];assert c['user_prompt'] in rendered
 ids=base.request('/tokenize',{'content':rendered,'add_special':True,'parse_special':True})['tokens']
 admissions.append({'name':c['name'],'input_tokens':len(ids),'total_with_reserve':len(ids)+2100,'fits':len(ids)+2100<=7900})
(ROOT/'admission.json').write_text(json.dumps(admissions,indent=2)+'\n');assert all(a['fits'] for a in admissions)
subprocess.run([str(ROOT.parents[2]/'target/debug/examples/journalist_assembly_trial'),str(ROOT/'cases.jsonl'),str(ROOT/'system.txt'),str(ROOT/'results'),base.URL,base.MODEL,'statements'],check=True)
after=base.request('/v1/models');(ROOT/'model-after.json').write_text(json.dumps(after,indent=2)+'\n')
for obj in [before,after]:
 for m in obj['data']:m.pop('created',None)
assert before==after
