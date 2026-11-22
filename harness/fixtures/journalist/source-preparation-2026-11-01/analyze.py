"""Full receipt/provenance audit; copy validity and semantic fidelity remain separate."""
import hashlib,json,statistics,sys
from pathlib import Path
ROOT=Path(sys.argv[1]).resolve() if len(sys.argv)>1 else Path(__file__).resolve().parent
for n,d in json.loads((ROOT/'manifest.json').read_text())['sha256'].items():assert hashlib.sha256((ROOT/n).read_bytes()).hexdigest()==d
admissions={}
for f in ROOT.glob('admission-*.json'):
 for r in json.loads(f.read_text()):admissions[(r['file'],r['name'])]=r
records=[];source_records={};writer_records={};repeated={}
for directory in sorted(ROOT.iterdir()):
 if not directory.is_dir() or not (directory/'results.jsonl').exists():continue
 phase,arm=directory.name.split('-',1);cohort='validation' if phase=='validation' else 'development';stage='prepare' if arm=='preparation' else 'write'
 file=f'{cohort}-sources.jsonl' if stage=='prepare' else f'{cohort}-{arm}.jsonl'
 cases={c['name']:c for c in map(json.loads,(ROOT/file).read_text().splitlines())}
 rows=list(map(json.loads,(directory/'results.jsonl').read_text().splitlines()));assert len(rows)==len(cases)
 for r in rows:
  c=cases[r['name']];path=directory/(c['name'] if stage=='prepare' else f"{c['name']}-{r['variant']}")
  inp=json.loads((path/'input.json').read_text());assert inp['user_prompt']==c['user_prompt']
  assert inp['system']==(ROOT/('preparer.txt' if stage=='prepare' else 'writer.txt')).read_text()
  packet=json.loads(c['user_prompt'])
  if stage=='prepare':
   assert inp['provenance']==c['provenance'] and inp['schema_raw']==c['schema_raw']
   assert set(packet)=={'article_id','headline','publisher','published_at','evidence_scope'} and packet['article_id']==c['provenance']['article_id']
  else:
   assert inp['parts']==c['parts'] and inp['source_packet']==packet and inp['schema']==c['schema'] and json.loads(inp['schema_raw'])==c['schema']
   original=next(x for x in map(json.loads,(ROOT/f'{cohort}-control.jsonl').read_text().splitlines()) if x['name']==c['name']);clean=json.loads(c['user_prompt'])
   for g in clean['fresh']:
    for s in g['reports']:s.pop('preparation',None)
   assert clean==json.loads(original['user_prompt']) and c['parts']==original['parts']
   if arm=='automatic':
    for g in packet['fresh']:
     for source in g['reports']:
      article=c['parts']['reports'][int(source['report_key'].split('_')[1])-1]
      note=ROOT/f'{cohort}-preparation'/str(article['id']);prepinput=json.loads((note/'input.json').read_text())
      assert prepinput['provenance']['report']==article and source['preparation']==json.loads((note/'output.json').read_text())
      assert json.loads(prepinput['user_prompt'])['headline']==source['headline']
  receipt=json.loads((path/'receipt.json').read_text());req=receipt['request'];raw=json.loads(receipt['raw_response'])
  assert req['json_schema']==json.loads(inp['schema_raw'])
  admission=admissions[(file,c['name'])]
  assert raw['tokens_evaluated']==len(req['prompt'])==r['input_tokens']==admission['input_tokens']
  assert raw['tokens_predicted']==r['output_tokens']<inp['num_predict']==req['n_predict']==admission['reserved_output']
  assert r['input_tokens']+inp['num_predict']<=inp['num_ctx']==admission['num_ctx']
  assert req['temperature']==0 and req['cache_prompt'] is True and raw['stop'] and raw['stop_type']=='eos' and raw['truncated'] is False
  reply=json.loads((path/'output.json').read_text());assert reply==json.loads(raw['content'])
  if stage=='prepare':
   text=packet['headline'];copy_valid=True;properties=json.loads(inp['schema_raw'])['properties']['statements']['items']['properties']
   for statement in reply['statements']:
    spans=statement['qualifications']+([statement['actor']] if 'actor' in statement else statement['about']+[statement['speaker']])
    copy_valid &= all(s is None or bool(s.strip()) and s in text for s in spans)
    if 'about' in properties:
     assert all(s in properties['about']['items']['enum'] for s in statement['about'])
     assert statement['speaker'] in properties['speaker']['enum']
     assert all(s in properties['qualifications']['items']['enum'] for s in statement['qualifications'])
   assert copy_valid==r['spans_valid']
  repeated.setdefault((stage,arm,c['name']),[]).append((req,reply))
  if stage=='write' and r.get('source_mapping_valid'):
   rendered=json.loads((path/'rendered.json').read_text());assert len(rendered['narratives'])==len(packet['fresh'])==len(reply['narratives'])
   for item,group,prose in zip(reply['narratives'],packet['fresh'],rendered['narratives']):
    sources={s['report_key']:s for s in group['reports']};keys=[];sentences=[]
    for stmt in item['statements']:
     source=sources[stmt['report_key']];sentences.append(source['publisher']+' '+stmt['attribution']+' '+stmt['clause'])
     if stmt['report_key'] not in keys:keys.append(stmt['report_key'])
    assert group['anchor_report_key'] in keys and prose=={'headline':item['headline'],'body':' '.join(sentences),'report_keys':keys}
  r={**r,'run':directory.name,'stage':stage,'native_timings':raw['timings']};records.append(r)
  if stage=='prepare':source_records[(cohort,int(c['name']))]=r
  elif arm=='automatic' and phase!='repeat':writer_records[(cohort,c['name'])]=r
summary={'calls':len(records),'complete_receipts_and_integrity':True,'runs':{},'combined_automatic':[],'repeats':[],'records':records,'total_wall_seconds':sum(r['wall_ms'] for r in records)/1000}
for (stage,arm,name),values in repeated.items():
 if len(values)>1:
  assert all(v[0]==values[0][0] for v in values)
  summary['repeats'].append({'stage':stage,'arm':arm,'name':name,'same_requests':True,'same_parsed_output':all(v[1]==values[0][1] for v in values)})
for run in sorted({r['run'] for r in records}):
 rr=[r for r in records if r['run']==run]
 summary['runs'][run]={'calls':len(rr),'valid_spans_or_mapping':sum(r.get('spans_valid',r.get('source_mapping_valid',False)) for r in rr),'median_ms':statistics.median(r['wall_ms'] for r in rr),'total_ms':sum(r['wall_ms'] for r in rr),'range_ms':[min(r['wall_ms'] for r in rr),max(r['wall_ms'] for r in rr)],'input_tokens':sum(r['input_tokens'] for r in rr),'output_tokens':sum(r['output_tokens'] for r in rr),'input_tokens_range':[min(r['input_tokens'] for r in rr),max(r['input_tokens'] for r in rr)],'output_tokens_range':[min(r['output_tokens'] for r in rr),max(r['output_tokens'] for r in rr)],'cache_tokens_range':[min(r['native_timings']['cache_n'] for r in rr),max(r['native_timings']['cache_n'] for r in rr)]}
for cohort in ['development','validation']:
 if not (ROOT/f'{cohort}-attachment.json').exists():continue
 for c in map(json.loads,(ROOT/f'{cohort}-control.jsonl').read_text().splitlines()):
  ids={c['parts']['reports'][int(s['report_key'].split('_')[1])-1]['id'] for g in json.loads(c['user_prompt'])['fresh'] for s in g['reports']}
  prep=[source_records[(cohort,i)] for i in ids];writer=writer_records.get((cohort,c['name']));total=sum(r['wall_ms'] for r in prep)+(writer['wall_ms'] if writer else 0)
  summary['combined_automatic'].append({'cohort':cohort,'name':c['name'],'source_calls':len(ids),'preparation_ms':sum(r['wall_ms'] for r in prep),'writer_called':writer is not None,'writer_ms':writer['wall_ms'] if writer else 0,'total_ms':total,'input_tokens':sum(r['input_tokens'] for r in prep)+(writer['input_tokens'] if writer else 0),'output_tokens':sum(r['output_tokens'] for r in prep)+(writer['output_tokens'] if writer else 0)})
for f in ROOT.glob('model-after-*.json'):
 a=json.loads((ROOT/'model-before.json').read_text());b=json.loads(f.read_text())
 for obj in [a,b]:
  for model in obj['data']:model.pop('created',None)
 assert a==b
(ROOT/'measurements.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps({k:v for k,v in summary.items() if k!='records'},indent=2))
