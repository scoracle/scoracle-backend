import hashlib,importlib.util,json,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('boundary_run',ROOT.parents[1]/'context-boundaries-2026-11-01/run.py');base=importlib.util.module_from_spec(spec);spec.loader.exec_module(base)
for n,d in json.loads((ROOT/'manifest.json').read_text())['sha256'].items():assert hashlib.sha256((ROOT/n).read_bytes()).hexdigest()==d
before=base.request('/v1/models');assert before['data'][0]['id']==base.MODEL
(ROOT/'model-before.json').write_text(json.dumps(before,indent=2)+'\n');admissions=[]
for arm in ['control','reference']:
 for c in map(json.loads,(ROOT/f'development-{arm}.jsonl').read_text().splitlines()):
  system=(ROOT/'writer.txt').read_text();body={'messages':[{'role':'system','content':system},{'role':'user','content':c['user_prompt']}],'add_generation_prompt':True,'chat_template_kwargs':{'enable_thinking':False}}
  rendered=base.request('/apply-template',body)['prompt'];assert c['user_prompt'] in rendered and system in rendered
  ids=base.request('/tokenize',{'content':rendered,'add_special':True,'parse_special':True})['tokens'];admissions.append({'file':f'development-{arm}.jsonl','name':c['name'],'stage':'write','input_tokens':len(ids),'reserved_output':2100,'num_ctx':7900,'fits':len(ids)+2100<=7900})
(ROOT/'admission-initial.json').write_text(json.dumps(admissions,indent=2)+'\n');assert all(a['fits'] for a in admissions)
for arm in ['control','reference']:
 subprocess.run([str(ROOT.parents[3]/'target/debug/examples/journalist_assembly_trial'),str(ROOT/f'development-{arm}.jsonl'),str(ROOT/'writer.txt'),str(ROOT/f'development-{arm}'),base.URL,base.MODEL,'statements'],check=True)
after=base.request('/v1/models');(ROOT/'model-after-screen.json').write_text(json.dumps(after,indent=2)+'\n')
for obj in [before,after]:
 for model in obj['data']:model.pop('created',None)
assert before==after
