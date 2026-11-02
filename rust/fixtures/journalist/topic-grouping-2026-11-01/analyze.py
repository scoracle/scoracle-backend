"""Check source identity and token receipts; report cost without treating manual grouping as free."""
import json,statistics
from pathlib import Path
ROOT=Path(__file__).resolve().parent
cases={c['name']:c for c in map(json.loads,(ROOT/'cases.jsonl').read_text().splitlines())}
results=list(map(json.loads,(ROOT/'runs/results.jsonl').read_text().splitlines()))
assert len(results)==18
identities=[json.loads((ROOT/f'model-{phase}.json').read_text()) for phase in ['before','after']]
for identity in identities:
 for model in identity['data']:model.pop('created',None)
assert identities[0]==identities[1]
all_calls=[]
for row in results:
 directory=ROOT/'runs'/f"{row['name']}-pass-{row['pass']}"
 original=json.loads((ROOT.parent/'packet-size-2026-11-01'/f"{row['name']}-runs"/f"{row['name']}-40-pass-1-json-body-first/input.json").read_text())
 packet=json.loads(original['user_prompt']);source={r['report_key']:r for r in packet['fresh']}
 assert json.loads((directory/'parts.json').read_text())==cases[row['name']]['parts']
 for stage in (['grouper'] if row['mode']=='automatic' else [])+([] if row.get('error') else [row['mode']]):
  path=directory/stage
  assembled=json.loads((path/'input.json').read_text())
  if not (path/'receipt.json').exists():
   assert json.loads((path/'result.json').read_text()).get('error')
   continue
  receipt=json.loads((path/'receipt.json').read_text());request=receipt['request'];raw=json.loads(receipt['raw_response'])
  assert raw['tokens_evaluated']==len(request['prompt'])
  assert raw['stop'] is True and raw['stop_type']=='eos' and raw['truncated'] is False
  assert len(request['prompt'])+request['n_predict']<=assembled['num_ctx']<=10240
  assert raw['tokens_predicted']<request['n_predict']
  assert request['temperature']==0 and request['cache_prompt'] is True
  user=json.loads(assembled['user_prompt'])
  if stage=='grouper':assert user=={'meta':packet['meta'],'reports':packet['fresh']}
  elif stage=='baseline':assert user==packet
  else:
   assert user['meta']==packet['meta'] and user['tone']==packet['tone']
   seen=set()
   for i,g in enumerate(user['fresh']):
    assert g['topic_key']==f'topic_{i+1}'
    assert g['anchor_report_key']==f"report_{row['groups'][i][0]}"
    assert [r['report_key'] for r in g['reports']]==[f'report_{v}' for v in row['groups'][i]]
    for r in g['reports']:
     assert r==source[r['report_key']];seen.add(r['report_key'])
   assert seen==set(source)
  all_calls.append({'name':row['name'],'pass':row['pass'],'stage':stage,'input_tokens':len(request['prompt']),
                    'output_tokens':raw['tokens_predicted'],'timings':raw['timings'],
                    'reserved_output':request['n_predict']})
  if stage=='baseline':
   previous=ROOT.parent/'packet-size-2026-11-01'/f"{row['name']}-runs"/f"{row['name']}-40-pass-1-json-body-first/output.json"
   assert json.loads(raw['content'])==json.loads(previous.read_text())
summary={'complete_source_and_receipt_checks':True,'paths':len(results),'model_calls':len(all_calls),
         'by_mode':{},'repeat_comparisons':[],'records':results,'calls':all_calls}
for mode in ['baseline','reference','automatic']:
 group=[r for r in results if r['mode']==mode]
 summary['by_mode'][mode]={'paths':len(group),'completed_paths':sum('error' not in r and 'error' not in r.get('writing',{}) for r in group),
  'mechanically_accepted':sum(r.get('accepted',False) for r in group),'median_total_ms':statistics.median(r['total_ms'] for r in group),
  'total_ms_range':[min(r['total_ms'] for r in group),max(r['total_ms'] for r in group)]}
 for name in cases:
  outputs=[]
  for p in [1,2]:
   file=ROOT/'runs'/f'{name}-pass-{p}'/mode/'output.json'
   outputs.append(json.loads(file.read_text()) if file.exists() else None)
  if all(v is not None for v in outputs):
   requests=[json.loads((ROOT/'runs'/f'{name}-pass-{p}'/mode/'receipt.json').read_text())['request'] for p in [1,2]]
   assert requests[0]==requests[1]
  summary['repeat_comparisons'].append({'name':name,'mode':mode,'both_have_output':all(v is not None for v in outputs),'same_parsed_output':outputs[0]==outputs[1] if all(v is not None for v in outputs) else None})
completed=[r for r in results if r['mode']=='automatic' and not r.get('error') and not r.get('writing',{}).get('error')]
summary['automatic_completed_median_ms']=statistics.median(r['total_ms'] for r in completed) if completed else None
summary['automatic_grouping_median_ms']=statistics.median(r['grouping']['wall_ms'] for r in results if r['mode']=='automatic')
summary['automatic_total_ms_includes_failed_paths']=True
summary['manual_reference_cost_excludes_human_grouping']=True
for name in cases:
 repeated=[json.loads((ROOT/'runs'/f'{name}-pass-{p}'/'grouper/receipt.json').read_text()) for p in [1,2]]
 assert repeated[0]['request']==repeated[1]['request']
 assert json.loads(json.loads(repeated[0]['raw_response'])['content'])==json.loads(json.loads(repeated[1]['raw_response'])['content'])
(ROOT/'measurements.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps({k:v for k,v in summary.items() if k not in ['records','calls']},indent=2))
