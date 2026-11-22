import copy,hashlib,json
from pathlib import Path
from reference import REFERENCE
ROOT=Path(__file__).resolve().parent
encode=lambda obj:json.dumps(obj,ensure_ascii=False,separators=(',',':'))
def validate(note,text):
 assert 1<=len(note['statements'])<=3 and len(note['unspecified'])<=4
 for s in note['statements']:
  assert s['actor'] is None or s['actor'] and s['actor'] in text
  assert s['kind'] in ['assertion','opinion','question','listing'] and s['claim'].strip()
  assert len(s['qualifications'])<=5 and all(q and q in text for q in s['qualifications'])
 assert all(x.strip() for x in note['unspecified'])
def production_order(c,capture):
 original=json.loads(c['user_prompt']);text=capture['input']['messages'][1]['content'];packet=json.loads(text);assert encode(packet)==text
 assert {r['id']:r for r in c['parts']['reports']}=={r['id']:r for r in capture['parts']['reports']}
 sources={a['id']:r for a,r in zip(capture['parts']['reports'],packet['fresh'],strict=True)}
 groups=copy.deepcopy(original['fresh'])
 for g in groups:
  for i,r in enumerate(g['reports']):
   a=c['parts']['reports'][int(r['report_key'].split('_')[1])-1]
   replacement=copy.deepcopy(sources[a['id']]);replacement['report_key']=r['report_key'];assert replacement==r;g['reports'][i]=replacement
 packet['fresh']=groups;assert packet==original
 c['user_prompt']=encode(packet);return c

def build():
 for name in ['preparer.txt','writer.txt']:
  path=ROOT/name;path.write_text(path.read_text().strip())
 development=list(map(json.loads,(ROOT.parent/'production-order-2026-11-01/cases.jsonl').read_text().splitlines()))
 validation=list(map(json.loads,(ROOT.parent/'context-boundaries-2026-11-01/validation-control.jsonl').read_text().splitlines()))
 captures={r['subject']['name']:r for r in map(json.loads,(ROOT.parent/'context-boundaries-2026-11-01/live-capture/results.jsonl').read_text().splitlines())}
 validation=[production_order(c,captures[c['parts']['subject']['name']]) for c in validation]
 string={'type':'string','minLength':1,'maxLength':500}
 schema={'type':'object','additionalProperties':False,'required':['statements','unspecified'],'properties':{'statements':{'type':'array','minItems':1,'maxItems':3,'items':{'type':'object','additionalProperties':False,'required':['actor','kind','claim','qualifications'],'properties':{'actor':{'type':['string','null']},'kind':{'type':'string','enum':['assertion','opinion','question','listing']},'claim':string,'qualifications':{'type':'array','maxItems':5,'items':string}}}},'unspecified':{'type':'array','maxItems':4,'items':string}}}
 allsources=[]
 for cohort,cases in [('development',development),('validation',validation)]:
  (ROOT/f'{cohort}-control.jsonl').write_text(''.join(json.dumps(c,ensure_ascii=False)+'\n' for c in cases));sources={}
  for c in cases:
   for g in json.loads(c['user_prompt'])['fresh']:
    for r in g['reports']:
     a=c['parts']['reports'][int(r['report_key'].split('_')[1])-1]
     prompt={'article_id':a['id'],**{k:v for k,v in r.items() if k!='report_key'}}
     assert prompt['evidence_scope']=='headline'
     item={'name':str(a['id']),'user_prompt':encode(prompt),'schema_raw':encode(schema),'provenance':{'article_id':a['id'],'url':a['classifier_world']['url'],'report':a}}
     if a['id'] in sources:assert sources[a['id']]==item
     sources[a['id']]=item
  (ROOT/f'{cohort}-sources.jsonl').write_text(''.join(json.dumps(s,ensure_ascii=False)+'\n' for s in sources.values()))
  allsources.extend({'cohort':cohort,**s['provenance']} for s in sources.values())
  if cohort=='development':
   assert set(sources)==set(REFERENCE)
   for i,s in sources.items():validate(REFERENCE[i],json.loads(s['user_prompt'])['headline'])
   rows=copy.deepcopy(cases)
   for c in rows:
    packet=json.loads(c['user_prompt'])
    for g in packet['fresh']:
     for r in g['reports']:
      a=c['parts']['reports'][int(r['report_key'].split('_')[1])-1];r['preparation']=REFERENCE[a['id']]
    c['user_prompt']=encode(packet)
   (ROOT/'development-reference.jsonl').write_text(''.join(json.dumps(c,ensure_ascii=False)+'\n' for c in rows))
 (ROOT/'reference.json').write_text(json.dumps(REFERENCE,ensure_ascii=False,indent=2)+'\n')
 (ROOT/'sources.json').write_text(json.dumps(allsources,ensure_ascii=False,indent=2)+'\n')
 files=['development-control.jsonl','development-reference.jsonl','validation-control.jsonl','development-sources.jsonl','validation-sources.jsonl','reference.json','sources.json','preparer.txt','writer.txt']
 (ROOT/'manifest.json').write_text(json.dumps({'sha256':{n:hashlib.sha256((ROOT/n).read_bytes()).hexdigest() for n in files}},indent=2)+'\n')
 print('Frozen29 development sources, six development packets and three unused validation subjects; exact source spans and production field order checked.')
if __name__=='__main__':build()
