"""Validate frozen wire layouts, full native receipts and unchanged source provenance."""
import hashlib,json,statistics
from pathlib import Path
from run import ROOT,wire
for name,digest in json.loads((ROOT/'manifest.json').read_text())['sha256'].items():
 assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==digest
admissions={(r['cohort'],r['arm'],r['name']):r for r in json.loads((ROOT/'admission.json').read_text())}
original={r['name']:r for r in map(json.loads,(ROOT.parent/'statement-calibration-2026-11-01/cases.jsonl').read_text().splitlines())}
rows=[];outputs={};requests={}
for directory in sorted(ROOT.glob('*--*')):
 if not directory.is_dir():continue
 phase,arm=directory.name.split('--');cohort='validation' if phase=='validation' else 'development'
 cases={c['name']:c for c in map(json.loads,(ROOT/f'{cohort}-{arm}.jsonl').read_text().splitlines())}
 records=list(map(json.loads,(directory/'results.jsonl').read_text().splitlines()))
 assert len(records)==len(cases),f'incomplete run: {directory}'
 for record in records:
  case=cases[record['name']];packet=json.loads(case['user_prompt'])
  if cohort=='development':
   assert all(case[k]==original[case['name']][k] for k in ['parts','user_prompt','schema'])
  path=directory/f"{case['name']}-{record['variant']}"
  inp=json.loads((path/'input.json').read_text());assert inp['source_packet']==packet
  assert inp['user_prompt']==wire(case) and inp['parts']==case['parts'] and inp['schema']==case['schema']
  assert inp['system']==(ROOT/'system.txt').read_text() and json.loads(inp['schema_raw'])==inp['schema']
  assert inp['context_layout']==case.get('context_layout')
  receipt=json.loads((path/'receipt.json').read_text());req=receipt['request'];raw=json.loads(receipt['raw_response'])
  assert raw['tokens_evaluated']==len(req['prompt'])==record['input_tokens']==admissions[(cohort,arm,case['name'])]['input_tokens']
  assert raw['tokens_predicted']==record['output_tokens']<2100 and req['n_predict']==2100
  assert len(req['prompt'])+2100<=inp['num_ctx']==7900
  assert raw['stop'] and raw['stop_type']=='eos' and raw['truncated'] is False
  assert req['temperature']==0 and req['cache_prompt'] is True
  response=json.loads((path/'output.json').read_text());assert response==json.loads(raw['content'])
  if record.get('source_mapping_valid'):
   rendered=json.loads((path/'rendered.json').read_text())
   assert len(rendered['narratives'])==len(packet['fresh'])==len(response['narratives'])
   for entry,group,prose in zip(response['narratives'],packet['fresh'],rendered['narratives']):
    sources={r['report_key']:r for r in group['reports']};keys=[];sentences=[]
    for stmt in entry['statements']:
     source=sources[stmt['report_key']]
     assert stmt['attribution'] in ['reports that','notes that','discusses','asks whether','lists','quotes']
     sentences.append(source['publisher']+' '+stmt['attribution']+' '+stmt['clause'])
     if stmt['report_key'] not in keys:keys.append(stmt['report_key'])
    assert group['anchor_report_key'] in keys
    assert prose=={'headline':entry['headline'],'body':' '.join(sentences),'report_keys':keys}
  if arm=='control' and cohort=='development':
   previous=ROOT.parent/'statement-calibration-2026-11-01/examples-development'/f"{case['name']}-statements-source-first/output.json"
   assert response==json.loads(previous.read_text())
  key=(arm,case['name']);outputs.setdefault(key,[]).append(response);requests.setdefault(key,[]).append(req)
  rows.append({**record,'run':directory.name,'arm':arm,'phase':phase,'native_timings':raw['timings']})
summary={'complete_receipts_and_integrity':True,'calls':len(rows),'runs':{},'repeats':[],'records':rows}
for run in sorted({r['run'] for r in rows}):
 group=[r for r in rows if r['run']==run]
 summary['runs'][run]={'calls':len(group),'valid_source_mapping':sum(r.get('source_mapping_valid',False) for r in group),'mechanically_accepted':sum(r['accepted'] for r in group),'median_ms':statistics.median(r['wall_ms'] for r in group),'range_ms':[min(r['wall_ms'] for r in group),max(r['wall_ms'] for r in group)],'input_tokens_range':[min(r['input_tokens'] for r in group),max(r['input_tokens'] for r in group)],'output_tokens_range':[min(r['output_tokens'] for r in group),max(r['output_tokens'] for r in group)],'cache_tokens_range':[min(r['native_timings']['cache_n'] for r in group),max(r['native_timings']['cache_n'] for r in group)]}
for key,values in outputs.items():
 if len(values)>1:
  assert all(r==requests[key][0] for r in requests[key]);summary['repeats'].append({'arm':key[0],'name':key[1],'same_requests':True,'same_parsed_output':all(v==values[0] for v in values)})
summary['total_wall_seconds']=sum(r['wall_ms'] for r in rows)/1000
(ROOT/'measurements.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps({k:v for k,v in summary.items() if k!='records'},indent=2))
