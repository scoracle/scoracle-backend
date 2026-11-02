import hashlib,importlib.util,json,subprocess,sys
from pathlib import Path
from build_inputs import ROOT,encode,validate
spec=importlib.util.spec_from_file_location('boundary_run',ROOT.parent/'context-boundaries-2026-11-01/run.py');base=importlib.util.module_from_spec(spec);spec.loader.exec_module(base)
BIN=ROOT.parents[2]/'target/debug/examples'
def cases(file):return list(map(json.loads,(ROOT/file).read_text().splitlines()))
def admit(file,stage):
 system=(ROOT/('preparer.txt' if stage=='prepare' else 'writer.txt')).read_text();reserve=800 if stage=='prepare' else 2100;ctx=4096 if stage=='prepare' else 7900
 rows=[]
 for c in cases(file):
  budget=c.get('output_budget',reserve) if stage=='write' else reserve
  body={'messages':[{'role':'system','content':system},{'role':'user','content':c['user_prompt']}],'add_generation_prompt':True,'chat_template_kwargs':{'enable_thinking':False}}
  rendered=base.request('/apply-template',body)['prompt'];assert c['user_prompt'] in rendered and system in rendered
  ids=base.request('/tokenize',{'content':rendered,'add_special':True,'parse_special':True})['tokens']
  rows.append({'file':file,'name':c['name'],'stage':stage,'input_tokens':len(ids),'reserved_output':budget,'num_ctx':ctx,'fits':len(ids)+budget<=ctx})
 return rows

def run(file,stage,directory):
 subprocess.run([str(BIN/('journalist_source_trial' if stage=='prepare' else 'journalist_assembly_trial')),str(ROOT/file),str(ROOT/('preparer.txt' if stage=='prepare' else 'writer.txt')),str(ROOT/directory),base.URL,base.MODEL,*([] if stage=='prepare' else ['statements'])],check=True)

def attach(cohort):
 sources={int(c['name']):c for c in cases(f'{cohort}-sources.jsonl')}
 records={int(r['name']):r for r in map(json.loads,(ROOT/f'{cohort}-preparation/results.jsonl').read_text().splitlines())}
 rows=[];blocked=[];index=[]
 for c in cases(f'{cohort}-control.jsonl'):
  packet=json.loads(c['user_prompt']);bad=[];used=[]
  for g in packet['fresh']:
   for r in g['reports']:
    article=c['parts']['reports'][int(r['report_key'].split('_')[1])-1];i=article['id'];used.append(i)
    assert sources[i]['provenance']['report']==article
    if not records[i].get('spans_valid'):bad.append(i);continue
    note=json.loads((ROOT/f'{cohort}-preparation/{i}/output.json').read_text());validate(note,r['headline'])
    r['preparation']=note
    index.append({'packet':c['name'],'report_key':r['report_key'],'article_id':i,'url':article['classifier_world']['url'],'preparation_output':f'{cohort}-preparation/{i}/output.json','source_fingerprint':hashlib.sha256(sources[i]['user_prompt'].encode()).hexdigest()})
  if bad:blocked.append({'name':c['name'],'invalid_preparation_ids':bad,'writer_called':False});continue
  c['user_prompt']=encode(packet);rows.append(c)
 (ROOT/f'{cohort}-automatic.jsonl').write_text(''.join(json.dumps(c,ensure_ascii=False)+'\n' for c in rows))
 (ROOT/f'{cohort}-attachment.json').write_text(json.dumps({'blocked':blocked,'links':index},ensure_ascii=False,indent=2)+'\n')
 return rows

def main():
 for n,d in json.loads((ROOT/'manifest.json').read_text())['sha256'].items():assert hashlib.sha256((ROOT/n).read_bytes()).hexdigest()==d
 before=base.request('/v1/models');assert before['data'][0]['id']==base.MODEL
 mode=sys.argv[1];admissions=[]
 if mode=='development':
  (ROOT/'model-before.json').write_text(json.dumps(before,indent=2)+'\n')
  for f,s in [('development-sources.jsonl','prepare'),('validation-sources.jsonl','prepare'),('development-control.jsonl','write'),('development-reference.jsonl','write'),('validation-control.jsonl','write')]:admissions.extend(admit(f,s))
  (ROOT/'admission-initial.json').write_text(json.dumps(admissions,indent=2)+'\n');assert all(a['fits'] for a in admissions)
  for arm in ['control','reference']:run(f'development-{arm}.jsonl','write',f'development-{arm}')
  run('development-sources.jsonl','prepare','development-preparation')
  if attach('development'):
   admissions=admit('development-automatic.jsonl','write');(ROOT/'admission-automatic.json').write_text(json.dumps(admissions,indent=2)+'\n');assert all(a['fits'] for a in admissions)
   run('development-automatic.jsonl','write','development-automatic')
 elif mode=='validation':
  selection=json.loads((ROOT/'selection.json').read_text());assert selection['qualifies_for_followup']
  run('development-automatic.jsonl','write','repeat-automatic')
  run('validation-sources.jsonl','prepare','validation-preparation')
  run('validation-control.jsonl','write','validation-control')
  if attach('validation'):
   admissions=admit('validation-automatic.jsonl','write');(ROOT/'admission-validation-automatic.json').write_text(json.dumps(admissions,indent=2)+'\n');assert all(a['fits'] for a in admissions)
   run('validation-automatic.jsonl','write','validation-automatic')
 else:raise ValueError(mode)
 after=base.request('/v1/models');(ROOT/f'model-after-{mode}.json').write_text(json.dumps(after,indent=2)+'\n')
 for obj in [before,after]:
  for m in obj['data']:m.pop('created',None)
 assert before==after
if __name__=='__main__':main()
