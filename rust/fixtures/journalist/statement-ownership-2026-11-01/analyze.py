"""Verify source preservation, renderer behavior and model receipts without rerunning inference."""
import hashlib,json,statistics
from pathlib import Path
ROOT=Path(__file__).resolve().parent
manifest=json.loads((ROOT/'manifest.json').read_text())
for name,digest in manifest['sha256'].items():assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==digest
identities=[json.loads((ROOT/f'model-{p}.json').read_text()) for p in ['before','after']]
for value in identities:
 for m in value['data']:m.pop('created',None)
assert identities[0]==identities[1]
cases={c['name']:c for c in map(json.loads,(ROOT/'cases-prose.jsonl').read_text().splitlines())}
structured_cases={c['name']:c for c in map(json.loads,(ROOT/'cases-structured.jsonl').read_text().splitlines())}
rows=[];pairs={}
for arm in ['baseline','prose','structured']:
 for pass_number in [1,2]:
  directory=ROOT/f'{arm}-pass-{pass_number}'
  records=list(map(json.loads,(directory/'results.jsonl').read_text().splitlines()));assert len(records)==6
  for result in records:
   case=(structured_cases if arm=='structured' else cases)[result['name']]
   path=directory/f"{result['name']}-{result['variant']}"
   prepared=json.loads((path/'input.json').read_text())
   assert prepared['user_prompt']==case['user_prompt'] and prepared['parts']==case['parts']
   assert prepared['system']==(ROOT/f'{arm}.txt').read_text()
   assert prepared['schema']==case['schema']
   if arm=='structured':assert json.loads(prepared['schema_raw'])==case['schema']
   receipt=json.loads((path/'receipt.json').read_text());request=receipt['request'];raw=json.loads(receipt['raw_response'])
   assert raw['tokens_evaluated']==len(request['prompt'])==result['input_tokens']
   assert raw['tokens_predicted']==result['output_tokens']<2100
   assert raw['stop'] is True and raw['stop_type']=='eos' and raw['truncated'] is False
   assert len(request['prompt'])+request['n_predict']<=prepared['num_ctx']==7900
   assert request['n_predict']==2100 and request['temperature']==0 and request['cache_prompt'] is True
   response=json.loads((path/'output.json').read_text());assert response==json.loads(raw['content'])
   packet=json.loads(case['user_prompt']);groups=packet['fresh']
   rendered=response
   if arm=='structured' and result.get('source_mapping_valid'):
    rendered=json.loads((path/'rendered.json').read_text())
    assert len(response['narratives'])==len(groups)==len(rendered['narratives'])
    for entry,group,output in zip(response['narratives'],groups,rendered['narratives']):
     sources={r['report_key']:r for r in group['reports']};keys=[];sentences=[]
     for statement in entry['statements']:
      key=statement['report_key'];source=sources[key]
      assert statement['attribution'] in ['reports that','notes that','discusses','asks whether','lists','quotes']
      sentences.append(source['publisher']+' '+statement['attribution']+' '+statement['clause'])
      if key not in keys:keys.append(key)
     assert group['anchor_report_key'] in keys
     assert output=={'headline':entry['headline'],'body':' '.join(sentences),'report_keys':keys}
   coverage=False
   if arm!='structured' or result.get('source_mapping_valid'):
    coverage=len(rendered['narratives'])==len(groups) and all(g['anchor_report_key'] in n['report_keys'] and set(n['report_keys'])<={r['report_key'] for r in g['reports']} for n,g in zip(rendered['narratives'],groups))
   if arm=='baseline' and case['cohort']=='known':
    slug=result['name'].removesuffix('-known')
    prior=json.loads((ROOT.parent/'topic-grouping-2026-11-01/selected-runs'/f'{slug}-pass-1/selected/output.json').read_text())
    assert response==prior
   pairs.setdefault((arm,result['name']),[]).append((request,response))
   rows.append({**result,'arm':arm,'pass':pass_number,'cohort':case['cohort'],'anchor_and_group_coverage':coverage,'native_timings':raw['timings']})
assert len(rows)==36
repeats=[]
for (arm,name),pair in pairs.items():
 assert len(pair)==2 and pair[0][0]==pair[1][0]
 repeats.append({'arm':arm,'name':name,'identical_requests':True,'identical_parsed_output':pair[0][1]==pair[1][1]})
summary={'model_calls':36,'complete_receipts_and_source_integrity':True,'repeat_comparisons':repeats,'by_arm':{},'records':rows}
for arm in ['baseline','prose','structured']:
 group=[r for r in rows if r['arm']==arm]
 summary['by_arm'][arm]={'calls':len(group),'median_wall_ms':statistics.median(r['wall_ms'] for r in group),'range_wall_ms':[min(r['wall_ms'] for r in group),max(r['wall_ms'] for r in group)],'mechanically_accepted':sum(r['accepted'] for r in group),'anchor_and_group_coverage':sum(r['anchor_and_group_coverage'] for r in group),'source_mapping_valid':sum(r.get('source_mapping_valid',False) for r in group) if arm=='structured' else None,'input_tokens_range':[min(r['input_tokens'] for r in group),max(r['input_tokens'] for r in group)],'output_tokens_range':[min(r['output_tokens'] for r in group),max(r['output_tokens'] for r in group)],'cache_tokens_range':[min(r['native_timings']['cache_n'] for r in group),max(r['native_timings']['cache_n'] for r in group)]}
(ROOT/'measurements.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps({k:v for k,v in summary.items() if k!='records'},indent=2))
