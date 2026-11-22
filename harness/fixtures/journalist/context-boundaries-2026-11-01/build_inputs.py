"""Freeze semantically identical context layouts and new real-source validation packets."""
import copy,hashlib,json
from pathlib import Path
ROOT=Path(__file__).resolve().parent
OLD=ROOT.parent/'statement-calibration-2026-11-01'
ARMS={'control':None,'json-context-first':'json-context-first','sections-source-first':'sections-source-first','sections-context-first':'sections-context-first'}
(ROOT/'system.txt').write_bytes((OLD/'examples.txt').read_bytes())
development=list(map(json.loads,(OLD/'cases.jsonl').read_text().splitlines()))
selection={'Liverpool':('liverpool',[[21],[22],[39]]),'Boston Celtics':('boston',[[5],[24],[27]]),'Green Bay Packers':('green-bay',[[1,24],[10,16],[32]])}
validation=[];sources=[]
for record in map(json.loads,(ROOT/'live-capture/results.jsonl').read_text().splitlines()):
 assert record['selected']==40 and not record.get('preparation_error')
 slug,groups=selection[record['subject']['name']]
 packet=json.loads(record['input']['messages'][1]['content']);reports=packet['fresh']
 packet['fresh']=[{'topic_key':f'topic_{i}','anchor_report_key':f'report_{ids[0]}','reports':[copy.deepcopy(reports[j-1]) for j in ids]} for i,ids in enumerate(groups,1)]
 keys=sorted({r['report_key'] for g in packet['fresh'] for r in g['reports']})
 c=copy.deepcopy(development[0]);c['name']=slug+'-validation';c['parts']=record['parts'];c['user_prompt']=json.dumps(packet,ensure_ascii=False,separators=(',',':'),sort_keys=True)
 c['schema']['properties']['narratives']['items']['properties']['statements']['items']['properties']['report_key']['enum']=keys
 c['review']=['Preserve actual source actors and unknown details; metadata is background, not publisher evidence.','Review headlines, qualifications, source ownership, coverage and grammar; parser acceptance is not truth.']
 validation.append(c)
 sources.append({'subject':record['subject'],'groups':groups,'sources':[{'report_key':f'report_{i}','article_id':a['id'],'publisher':a['source'],'headline':a['title'],'url':a['classifier_world']['url']} for i,a in enumerate(record['parts']['reports'],1) if f'report_{i}' in keys]})
for cohort,cases in [('development',development),('validation',validation)]:
 for arm,layout in ARMS.items():
  rows=copy.deepcopy(cases)
  for c in rows:
   if layout:c['context_layout']=layout
  (ROOT/f'{cohort}-{arm}.jsonl').write_text(''.join(json.dumps(c,ensure_ascii=False)+'\n' for c in rows))
  assert all(c['parts']==o['parts'] and c['user_prompt']==o['user_prompt'] and c['schema']==o['schema'] for c,o in zip(rows,cases))
(ROOT/'validation-sources.json').write_text(json.dumps({'subjects':sources,'notes':['All40 headlines per new subject inspected before any development inference. Sources themselves are not independently verified.','Liverpool: unnamed striker quote, unspecified Mac Allister contract stance, Van Dijk praise; do not import coach Iraola.','Boston: Robinson and George statements belong to the named players, not database coach Thompson; future game summary page does not establish completion.','Green Bay: preserve LaFleur despite database coach Stenavich; CBS unnamed coach may not inherit that identity; Musgrave season uncertainty survives; $64M veteran stays unnamed.','Manual groups are experiment preparation; their cost and correctness are not qualified for production.']},ensure_ascii=False,indent=2)+'\n')
files=sorted([*ROOT.glob('development-*.jsonl'),*ROOT.glob('validation-*.jsonl'),ROOT/'system.txt',ROOT/'validation-sources.json'])
(ROOT/'manifest.json').write_text(json.dumps({'sha256':{f.name:hashlib.sha256(f.read_bytes()).hexdigest() for f in files}},indent=2)+'\n')
print('Frozen four layouts, six development and three new validation subjects; all source and metadata values retained.')
