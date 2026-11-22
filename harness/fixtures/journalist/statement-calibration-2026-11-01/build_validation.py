"""Freeze new-subject source groups before development inference; keep full DB provenance."""
import copy,json,hashlib
from pathlib import Path
ROOT=Path(__file__).resolve().parent
selection={'Arsenal':('arsenal',[[1],[21],[27]]),'Tampa Bay Buccaneers':('tampa-bay',[[8,28],[10],[38,32,35,37]]),'Cleveland Cavaliers':('cleveland',[[6,9,12,15],[10],[33]])}
prototype=json.loads((ROOT/'cases.jsonl').read_text().splitlines()[0])
cases=[];manifest=[]
for line in (ROOT/'live-capture/results.jsonl').read_text().splitlines():
 record=json.loads(line);assert record['selected']==40 and not record.get('preparation_error')
 slug,groups=selection[record['subject']['name']]
 packet=json.loads(record['input']['messages'][1]['content']);sources=packet['fresh']
 packet['fresh']=[{'topic_key':f'topic_{i}','anchor_report_key':f'report_{ids[0]}','reports':[copy.deepcopy(sources[j-1]) for j in ids]} for i,ids in enumerate(groups,1)]
 keys=sorted({r['report_key'] for g in packet['fresh'] for r in g['reports']})
 case=copy.deepcopy(prototype);case['name']=slug+'-validation';case['parts']=record['parts'];case['cohort']='new-subject-validation';case['user_prompt']=json.dumps(packet,ensure_ascii=False,separators=(',',':'),sort_keys=True)
 case['schema']['properties']['narratives']['items']['properties']['statements']['items']['properties']['report_key']['enum']=keys
 cases.append(case)
 manifest.append({'subject':record['subject'],'groups':groups,'sources':[{'report_key':f'report_{i}','article_id':a['id'],'publisher':a['source'],'headline':a['title'],'url':a['classifier_world']['url']} for i,a in enumerate(record['parts']['reports'],1) if f'report_{i}' in keys]})
(ROOT/'validation.jsonl').write_text(''.join(json.dumps(c,ensure_ascii=False)+'\n' for c in cases))
gapped=copy.deepcopy(cases)
for c in gapped:
 stmt=c['schema']['properties']['narratives']['items']['properties']['statements']['items'];stmt['required'].append('unspecified');stmt['properties']['unspecified']={'type':'array','maxItems':3,'items':{'type':'string','minLength':1,'maxLength':80}}
(ROOT/'validation-gaps.jsonl').write_text(''.join(json.dumps(c,ensure_ascii=False)+'\n' for c in gapped))
(ROOT/'validation-manifest.json').write_text(json.dumps({'subjects':manifest,'selection_notes':['All40 headlines per subject were inspected before development inference. No source text rewritten.','Arsenal chosen topics test keeping an opponent player with the opponent, an unexplained bill and an unnamed striker/unsupplied timeline.','Tampa Vea trade questions preserve source opinion and numeric details without inferring what the bill represents. The adjacent unnamed star stays unnamed. Mayfield group includes both the conditional return headline and the later returns wording, without assuming the condition was satisfied or what return means.','Cleveland fixture-page headings do not establish results. The unspecified traded player must not become Mobley from the next topic; the Mobley decision remains unspecified.'],'sha256':{n:hashlib.sha256((ROOT/n).read_bytes()).hexdigest() for n in ['validation.jsonl','validation-gaps.jsonl']}},ensure_ascii=False,indent=2)+'\n')
print('Frozen three new-subject validation packets with original source IDs/URLs.')
