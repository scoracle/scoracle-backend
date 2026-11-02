"""Frozen context-layout trial; follow-up requires a recorded development selection."""
import hashlib,json,subprocess,sys,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parent
BIN=ROOT.parents[2]/'target/debug/examples/journalist_assembly_trial'
URL='http://127.0.0.1:8081';MODEL='Qwen3.5-4B-Q4_K_M'
ARMS=['control','json-context-first','sections-source-first','sections-context-first']
def request(path,body=None):
 req=urllib.request.Request(URL+path,data=None if body is None else json.dumps(body).encode(),headers={'Content-Type':'application/json'})
 with urllib.request.urlopen(req,timeout=30) as response:return json.load(response)
def wire(case):
 original=case['user_prompt'];packet=json.loads(original);layout=case.get('context_layout')
 if not layout:return original
 order=['fresh','meta','tone'] if layout=='sections-source-first' else ['meta','tone','fresh']
 encode=lambda v:json.dumps(v,ensure_ascii=False,separators=(',',':'))
 if layout=='json-context-first':return encode({k:packet[k] for k in order})
 labels={'meta':'meta — database identity background, not publisher reporting:','fresh':'fresh — publisher reporting evidence:','tone':'tone — writing style only:'}
 return '\n\n'.join(labels[k]+'\n'+encode(packet[k]) for k in order)
def verify():
 for name,digest in json.loads((ROOT/'manifest.json').read_text())['sha256'].items():assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==digest
 assert request('/v1/models')['data'][0]['id']==MODEL

def run(arm,cohort,phase):
 subprocess.run([str(BIN),str(ROOT/f'{cohort}-{arm}.jsonl'),str(ROOT/'system.txt'),str(ROOT/f'{phase}--{arm}'),URL,MODEL,'statements'],check=True)

def main():
 mode=sys.argv[1];verify()
 if mode=='development':
  (ROOT/'model-before.json').write_text(json.dumps(request('/v1/models'),indent=2)+'\n')
  (ROOT/'server-before.json').write_text(json.dumps(request('/props'),indent=2)+'\n')
  admissions=[]
  for cohort in ['development','validation']:
   for arm in ARMS:
    for case in map(json.loads,(ROOT/f'{cohort}-{arm}.jsonl').read_text().splitlines()):
     body={'messages':[{'role':'system','content':(ROOT/'system.txt').read_text()},{'role':'user','content':wire(case)}],'add_generation_prompt':True,'chat_template_kwargs':{'enable_thinking':False}}
     rendered=request('/apply-template',body)['prompt'];assert wire(case) in rendered and body['messages'][0]['content'] in rendered
     ids=request('/tokenize',{'content':rendered,'add_special':True,'parse_special':True})['tokens']
     admissions.append({'cohort':cohort,'arm':arm,'name':case['name'],'input_tokens':len(ids),'total_with_reserve':len(ids)+2100,'fits':len(ids)+2100<=7900})
  (ROOT/'admission.json').write_text(json.dumps(admissions,indent=2)+'\n')
  assert all(a['fits'] for a in admissions)
  for arm in ARMS:run(arm,'development','development')
 elif mode=='validation':
  selection=json.loads((ROOT/'selection.json').read_text());arm=selection['selected'];assert arm in ARMS[1:] and selection['qualifies_for_followup']
  run(arm,'development','repeat')
  for a in ['control',arm]:run(a,'validation','validation')
 else:raise ValueError(mode)
 after=request('/v1/models');(ROOT/f'model-after-{mode}.json').write_text(json.dumps(after,indent=2)+'\n')
 before=json.loads((ROOT/'model-before.json').read_text())
 for obj in [before,after]:
  for model in obj['data']:model.pop('created',None)
 assert before==after
if __name__=='__main__':main()
