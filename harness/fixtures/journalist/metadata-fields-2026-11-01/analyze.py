"""Check typed-field preservation and causal interventions independently of semantic review."""
import json,hashlib,statistics
from pathlib import Path
from build_inputs import ROOT,BASE,record_text
from run import wire
for name,digest in json.loads((ROOT/'manifest.json').read_text())['sha256'].items():assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==digest
admissions={(r['file'],r['name']):r for r in json.loads((ROOT/'admission.json').read_text())}
rows=[];responses={}
for directory in sorted(ROOT.glob('*--*')):
 phase,arm=directory.name.split('--');cohort='validation' if phase=='validation' else 'development'
 file='diagnostics.jsonl' if phase=='diagnostic' else f'{cohort}-{arm}.jsonl'
 casepath=BASE/file if arm=='control' else ROOT/file
 cases={c['name']:c for c in map(json.loads,casepath.read_text().splitlines())}
 originals={c['name']:c for c in map(json.loads,(BASE/f'{cohort}-control.jsonl').read_text().splitlines())}
 records=list(map(json.loads,(directory/'results.jsonl').read_text().splitlines()));assert len(records)==len(cases)
 for r in records:
  c=cases[r['name']];packet=json.loads(c['user_prompt']);orig=originals['ac-milan-known' if phase=='diagnostic' else c['name']];original=json.loads(orig['user_prompt'])
  assert c['parts']==orig['parts'] and c['schema']==orig['schema']
  assert packet['fresh']==original['fresh'] and packet['tone']==original['tone']
  restored=json.loads(c['user_prompt'])
  if phase!='diagnostic' and arm!='control':restored['meta']['database_record']=record_text(restored['meta']['database_record'])
  elif phase=='diagnostic':
   expected=original['meta']['database_record']
   if c['name']=='identity-only':expected=None
   else:expected=expected.replace('Filippo Galli',{'coach-a':'Maren Vale','coach-b':'Tomas Reed'}[c['name']])
   assert restored['meta']['database_record']==expected
   restored['meta']['database_record']=original['meta']['database_record']
  assert restored==original
  path=directory/f"{c['name']}-{r['variant']}";inp=json.loads((path/'input.json').read_text())
  assert inp['user_prompt']==wire(c) and inp['source_packet']==packet and inp['parts']==c['parts']
  assert inp['system']==(ROOT/'system.txt').read_text() and inp['schema']==c['schema'] and json.loads(inp['schema_raw'])==c['schema']
  receipt=json.loads((path/'receipt.json').read_text());req=receipt['request'];raw=json.loads(receipt['raw_response'])
  assert raw['tokens_evaluated']==len(req['prompt'])==r['input_tokens']
  if arm!='control':assert r['input_tokens']==admissions[(file,c['name'])]['input_tokens']
  assert req['temperature']==0 and req['cache_prompt'] is True and req['n_predict']==2100
  assert r['input_tokens']+2100<=inp['num_ctx']==7900
  assert raw['stop'] and raw['stop_type']=='eos' and raw['truncated'] is False
  assert raw['tokens_predicted']==r['output_tokens']<2100
  response=json.loads((path/'output.json').read_text());assert response==json.loads(raw['content'])
  if r.get('source_mapping_valid'):
   rendered=json.loads((path/'rendered.json').read_text());assert len(response['narratives'])==len(packet['fresh'])==len(rendered['narratives'])
   for item,group,prose in zip(response['narratives'],packet['fresh'],rendered['narratives']):
    sources={s['report_key']:s for s in group['reports']};keys=[];sentences=[]
    for stmt in item['statements']:
     source=sources[stmt['report_key']];assert stmt['attribution'] in ['reports that','notes that','discusses','asks whether','lists','quotes']
     sentences.append(source['publisher']+' '+stmt['attribution']+' '+stmt['clause'])
     if stmt['report_key'] not in keys:keys.append(stmt['report_key'])
    assert group['anchor_report_key'] in keys and prose=={'headline':item['headline'],'body':' '.join(sentences),'report_keys':keys}
  responses.setdefault((arm,c['name']),[]).append((req,response))
  rows.append({**r,'run':directory.name,'phase':phase,'arm':arm,'native_timings':raw['timings']})
summary={'calls':len(rows),'complete_receipts_and_integrity':True,'runs':{},'repeats':[],'records':rows,'total_wall_seconds':sum(r['wall_ms'] for r in rows)/1000}
for name in sorted({r['run'] for r in rows}):
 rr=[r for r in rows if r['run']==name]
 summary['runs'][name]={'calls':len(rr),'source_mappings_valid':sum(r.get('source_mapping_valid',False) for r in rr),'mechanically_accepted':sum(r['accepted'] for r in rr),'median_ms':statistics.median(r['wall_ms'] for r in rr),'range_ms':[min(r['wall_ms'] for r in rr),max(r['wall_ms'] for r in rr)],'input_tokens_range':[min(r['input_tokens'] for r in rr),max(r['input_tokens'] for r in rr)],'output_tokens_range':[min(r['output_tokens'] for r in rr),max(r['output_tokens'] for r in rr)],'cache_tokens_range':[min(r['native_timings']['cache_n'] for r in rr),max(r['native_timings']['cache_n'] for r in rr)]}
for (arm,name),rr in responses.items():
 if len(rr)>1:
  assert all(x[0]==rr[0][0] for x in rr);summary['repeats'].append({'arm':arm,'name':name,'same_requests':True,'same_parsed_output':all(x[1]==rr[0][1] for x in rr)})
for file in ROOT.glob('model-after-*.json'):
 a=json.loads((ROOT/'model-before.json').read_text());b=json.loads(file.read_text())
 for obj in [a,b]:
  for model in obj['data']:model.pop('created',None)
 assert a==b
(ROOT/'measurements.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps({k:v for k,v in summary.items() if k!='records'},indent=2))
