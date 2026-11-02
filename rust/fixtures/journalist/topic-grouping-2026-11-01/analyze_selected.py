"""Verify bounded-topic follow-up; manual selection is explicitly not a production throughput result."""
import json,statistics
from pathlib import Path
ROOT=Path(__file__).resolve().parent
cases={c['name']:c for c in map(json.loads,(ROOT/'cases.jsonl').read_text().splitlines())}
rows=list(map(json.loads,(ROOT/'selected-runs/results.jsonl').read_text().splitlines()))
assert len(rows)==6
repeats={}
for row in rows:
 path=ROOT/'selected-runs'/f"{row['name']}-pass-{row['pass']}"/'selected'
 original=json.loads((ROOT.parent/'packet-size-2026-11-01'/f"{row['name']}-runs"/f"{row['name']}-40-pass-1-json-body-first/input.json").read_text())
 packet=json.loads(original['user_prompt']);source={r['report_key']:r for r in packet['fresh']}
 assembled=json.loads((path/'input.json').read_text());new=json.loads(assembled['user_prompt'])
 receipt=json.loads((path/'receipt.json').read_text());request=receipt['request'];raw=json.loads(receipt['raw_response'])
 assert assembled['system']==(ROOT/'grouped.txt').read_text()
 assert new['meta']==packet['meta'] and new['tone']==packet['tone']
 expected=cases[row['name']]['reference_groups'][:3]
 assert row['groups']==expected and len(new['fresh'])==3
 keys=set()
 for g,ids in zip(new['fresh'],expected):
  assert g['anchor_report_key']==f'report_{ids[0]}'
  assert [r['report_key'] for r in g['reports']]==[f'report_{i}' for i in ids]
  for report in g['reports']:
   assert report==source[report['report_key']];keys.add(report['report_key'])
 assert len(keys)==row['visible_reports']
 assert set(assembled['schema']['properties']['narratives']['items']['properties']['report_keys']['items']['enum'])==keys
 assert raw['tokens_evaluated']==len(request['prompt'])==row['writing']['input_tokens']
 assert raw['stop'] and raw['stop_type']=='eos' and raw['truncated'] is False
 assert len(request['prompt'])+request['n_predict']<=assembled['num_ctx']==10000
 assert raw['tokens_predicted']<request['n_predict']==2100
 assert request['cache_prompt'] is True and request['temperature']==0
 output=json.loads((path/'output.json').read_text())
 assert output==json.loads(raw['content'])
 for entry in output['narratives']:assert set(entry['report_keys'])<=keys
 row['native_timings']=raw['timings']
 repeats.setdefault(row['name'],[]).append((request,output))
checks=[]
for name,pair in repeats.items():
 assert len(pair)==2 and pair[0][0]==pair[1][0]
 checks.append({'name':name,'identical_requests':True,'identical_parsed_output':pair[0][1]==pair[1][1]})
summary={'source_and_receipt_checks_passed':True,'writer_calls':6,'manual_selection_cost_not_included':True,
 'median_writer_total_ms':statistics.median(r['total_ms'] for r in rows),
 'writer_total_ms_range':[min(r['total_ms'] for r in rows),max(r['total_ms'] for r in rows)],
 'mechanically_accepted':sum(r['accepted'] for r in rows),
 'citations_within_assigned_topics':sum(r['citations_within_assigned_topics'] for r in rows),
 'repeat_comparisons':checks,'records':rows}
(ROOT/'selected-measurements.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps({k:v for k,v in summary.items() if k!='records'},indent=2))
