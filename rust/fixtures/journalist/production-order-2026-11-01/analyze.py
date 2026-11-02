import json,hashlib,statistics
from pathlib import Path
ROOT=Path(__file__).resolve().parent
for name,digest in json.loads((ROOT/'manifest.json').read_text())['sha256'].items():assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==digest
cases={c['name']:c for c in map(json.loads,(ROOT/'cases.jsonl').read_text().splitlines())}
originals={c['name']:c for c in map(json.loads,(ROOT.parent/'context-boundaries-2026-11-01/development-control.jsonl').read_text().splitlines())}
admissions={r['name']:r for r in json.loads((ROOT/'admission.json').read_text())}
rows=list(map(json.loads,(ROOT/'results/results.jsonl').read_text().splitlines()));assert len(rows)==len(cases)==6
for r in rows:
 c=cases[r['name']];o=originals[r['name']];packet=json.loads(c['user_prompt'])
 assert packet==json.loads(o['user_prompt']) and c['parts']==o['parts'] and c['schema']==o['schema']
 path=ROOT/'results'/f"{c['name']}-{r['variant']}";inp=json.loads((path/'input.json').read_text())
 assert inp['user_prompt']==c['user_prompt'] and inp['source_packet']==packet and inp['parts']==c['parts'] and inp['schema']==c['schema']
 assert inp['system']==(ROOT/'system.txt').read_text() and json.loads(inp['schema_raw'])==c['schema']
 receipt=json.loads((path/'receipt.json').read_text());req=receipt['request'];raw=json.loads(receipt['raw_response'])
 assert raw['tokens_evaluated']==len(req['prompt'])==r['input_tokens']==admissions[c['name']]['input_tokens']
 assert raw['tokens_predicted']==r['output_tokens']<2100 and req['n_predict']==2100
 assert req['temperature']==0 and req['cache_prompt'] is True and r['input_tokens']+2100<=inp['num_ctx']==7900
 assert raw['stop'] and raw['stop_type']=='eos' and raw['truncated'] is False
 response=json.loads((path/'output.json').read_text());assert response==json.loads(raw['content'])
 if r.get('source_mapping_valid'):
  rendered=json.loads((path/'rendered.json').read_text());assert len(rendered['narratives'])==len(packet['fresh'])==len(response['narratives'])
  for item,group,prose in zip(response['narratives'],packet['fresh'],rendered['narratives']):
   sources={s['report_key']:s for s in group['reports']};keys=[];sentences=[]
   for stmt in item['statements']:
    source=sources[stmt['report_key']];assert stmt['attribution'] in ['reports that','notes that','discusses','asks whether','lists','quotes']
    sentences.append(source['publisher']+' '+stmt['attribution']+' '+stmt['clause'])
    if stmt['report_key'] not in keys:keys.append(stmt['report_key'])
   assert group['anchor_report_key'] in keys and prose=={'headline':item['headline'],'body':' '.join(sentences),'report_keys':keys}
 r['native_timings']=raw['timings']
a=json.loads((ROOT/'model-before.json').read_text());b=json.loads((ROOT/'model-after.json').read_text())
for obj in [a,b]:
 for model in obj['data']:model.pop('created',None)
assert a==b
summary={'calls':6,'complete_receipts_and_integrity':True,'valid_source_mapping':sum(r.get('source_mapping_valid',False) for r in rows),'mechanically_accepted':sum(r['accepted'] for r in rows),'median_ms':statistics.median(r['wall_ms'] for r in rows),'range_ms':[min(r['wall_ms'] for r in rows),max(r['wall_ms'] for r in rows)],'input_tokens_range':[min(r['input_tokens'] for r in rows),max(r['input_tokens'] for r in rows)],'output_tokens_range':[min(r['output_tokens'] for r in rows),max(r['output_tokens'] for r in rows)],'cache_tokens_range':[min(r['native_timings']['cache_n'] for r in rows),max(r['native_timings']['cache_n'] for r in rows)],'total_wall_seconds':sum(r['wall_ms'] for r in rows)/1000,'records':rows}
(ROOT/'measurements.json').write_text(json.dumps(summary,indent=2)+'\n');print(json.dumps({k:v for k,v in summary.items() if k!='records'},indent=2))
