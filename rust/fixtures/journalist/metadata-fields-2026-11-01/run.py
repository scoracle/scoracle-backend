import importlib.util,json,hashlib,subprocess,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('boundary_run',ROOT.parent/'context-boundaries-2026-11-01/run.py');base=importlib.util.module_from_spec(spec);spec.loader.exec_module(base)
request=base.request;wire=base.wire
BIN=ROOT.parents[2]/'target/debug/examples/journalist_assembly_trial'
def invoke(file,directory):
 subprocess.run([str(BIN),str(ROOT/file),str(ROOT/'system.txt'),str(ROOT/directory),base.URL,base.MODEL,'statements'],check=True)
def main():
 for name,digest in json.loads((ROOT/'manifest.json').read_text())['sha256'].items():assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==digest
 before=request('/v1/models');assert before['data'][0]['id']==base.MODEL
 mode=sys.argv[1]
 if mode=='development':
  (ROOT/'model-before.json').write_text(json.dumps(before,indent=2)+'\n');admission=[]
  for file in [*sorted(ROOT.glob('development-*.jsonl')),*sorted(ROOT.glob('validation-*.jsonl')),ROOT/'diagnostics.jsonl']:
   for c in map(json.loads,file.read_text().splitlines()):
    body={'messages':[{'role':'system','content':(ROOT/'system.txt').read_text()},{'role':'user','content':wire(c)}],'add_generation_prompt':True,'chat_template_kwargs':{'enable_thinking':False}}
    rendered=request('/apply-template',body)['prompt'];assert wire(c) in rendered and body['messages'][0]['content'] in rendered
    tokens=request('/tokenize',{'content':rendered,'add_special':True,'parse_special':True})['tokens']
    admission.append({'file':file.name,'name':c['name'],'input_tokens':len(tokens),'total_with_reserve':len(tokens)+2100,'fits':len(tokens)+2100<=7900})
  (ROOT/'admission.json').write_text(json.dumps(admission,indent=2)+'\n');assert all(a['fits'] for a in admission)
  for arm in ['typed-source-first','typed-context-first']:invoke(f'development-{arm}.jsonl',f'development--{arm}')
  invoke('diagnostics.jsonl','diagnostic--counterfactual')
 elif mode=='validation':
  s=json.loads((ROOT/'selection.json').read_text());arm=s['selected'];assert s['qualifies_for_followup']
  invoke(f'development-{arm}.jsonl',f'repeat--{arm}')
  subprocess.run([str(BIN),str(ROOT.parent/'context-boundaries-2026-11-01/validation-control.jsonl'),str(ROOT/'system.txt'),str(ROOT/'validation--control'),base.URL,base.MODEL,'statements'],check=True)
  invoke(f'validation-{arm}.jsonl',f'validation--{arm}')
 else:raise ValueError(mode)
 after=request('/v1/models');(ROOT/f'model-after-{mode}.json').write_text(json.dumps(after,indent=2)+'\n')
 for obj in [before,after]:
  for m in obj['data']:m.pop('created',None)
 assert before==after
if __name__=='__main__':main()
