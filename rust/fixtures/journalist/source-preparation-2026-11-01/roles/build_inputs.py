"""Second frozen comparison: explicit speaker/about roles and exact assigned topic count."""
import copy,hashlib,json,re
from pathlib import Path
ROOT=Path(__file__).resolve().parent;OLD=ROOT.parent
encode=lambda obj:json.dumps(obj,ensure_ascii=False,separators=(',',':'))
def spans(text):
 words=list(re.finditer(r'\S+',text));values=set()
 for i in range(len(words)):
  for j in range(i,len(words)):
   raw=text[words[i].start():words[j].end()]
   for value in [raw,raw.strip('.,:;?!\'"‘’“”()[]')]:
    if value:values.add(value)
 assert all(v in text for v in values)
 return sorted(values)
def build():
 addition='''\n\nPREPARATION ROLES\nPreparation statements distinguish about (the people, teams or topics discussed) from speaker (a person or organization explicitly making the statement). Being discussed does not mean speaking. An opinion about a person is not that person's opinion. When speaker is null, do not assign the opinion, quote or prediction to an about entity. Check any non-null speaker against the original headline. The publisher remains the reporting source, separate from the quoted speaker. The prepared claim and qualifications must preserve these roles.\n'''
 (ROOT/'writer.txt').write_text(((OLD/'writer.txt').read_text()+addition).strip())
 notes=json.loads((OLD/'reference.json').read_text());converted={}
 for i,note in notes.items():
  converted[i]={'statements':[{'about':[] if s['actor'] is None else [s['actor']],'speaker':'Pavlovic' if i=='1425437' else ('Bears' if i=='1432888' else None),'kind':s['kind'],'claim':s['claim'],'qualifications':s['qualifications']} for s in note['statements']],'unspecified':note['unspecified']}
 for cohort in ['development','validation']:
  cases=list(map(json.loads,(OLD/f'{cohort}-control.jsonl').read_text().splitlines()))
  for c in cases:
   count=len(json.loads(c['user_prompt'])['fresh']);assert count==3
   c['schema']['properties']['narratives']['minItems']=count;c['schema']['properties']['narratives']['maxItems']=count
  (ROOT/f'{cohort}-control.jsonl').write_text(''.join(json.dumps(c,ensure_ascii=False)+'\n' for c in cases))
  if cohort=='development':
   reference=copy.deepcopy(cases)
   for c in reference:
    packet=json.loads(c['user_prompt'])
    for g in packet['fresh']:
     for r in g['reports']:
      i=str(c['parts']['reports'][int(r['report_key'].split('_')[1])-1]['id']);r['preparation']=converted[i]
    c['user_prompt']=encode(packet)
   (ROOT/'development-reference.jsonl').write_text(''.join(json.dumps(c,ensure_ascii=False)+'\n' for c in reference))
  sources=list(map(json.loads,(OLD/f'{cohort}-sources.jsonl').read_text().splitlines()))
  for c in sources:
   text=json.loads(c['user_prompt'])['headline'];choices=spans(text);schema=json.loads(c['schema_raw']);props=schema['properties']['statements']['items']
   props['required']=['about','speaker','kind','claim','qualifications'];old=props['properties']
   props['properties']={'about':{'type':'array','maxItems':4,'items':{'type':'string','enum':choices}},'speaker':{'type':['string','null'],'enum':[None,*choices]},'kind':old['kind'],'claim':old['claim'],'qualifications':{'type':'array','maxItems':5,'items':{'type':'string','enum':choices}}}
   c['schema_raw']=encode(schema)
  (ROOT/f'{cohort}-sources.jsonl').write_text(''.join(json.dumps(c,ensure_ascii=False)+'\n' for c in sources))
 (ROOT/'reference.json').write_text(json.dumps(converted,ensure_ascii=False,indent=2)+'\n')
 for i,note in converted.items():
  source=next(c for c in map(json.loads,(OLD/'development-sources.jsonl').read_text().splitlines()) if c['name']==i);text=json.loads(source['user_prompt'])['headline']
  for s in note['statements']:
   assert all(x in text for x in s['about']+s['qualifications']) and (s['speaker'] is None or s['speaker'] in text)
 files=[p for p in ROOT.iterdir() if p.suffix in ['.jsonl','.txt'] and p.name not in ['protocol.txt']]+[ROOT/'reference.json']
 (ROOT/'manifest.json').write_text(json.dumps({'sha256':{p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(files)}},indent=2)+'\n')
 print('Frozen speaker/about notes and source-span enums; all three topic slots required, source values and order preserved.')
if __name__=='__main__':build()
