"""Serve the same database attributes as fields; keep diagnostic counterfactuals separate."""
import copy,hashlib,json
from pathlib import Path
ROOT=Path(__file__).resolve().parent
BASE=ROOT.parent/'context-boundaries-2026-11-01'
def record_text(r):
 text=r['name']
 if 'league_name' in r:
  text+=' — '+r['league_name']
  if 'league_country' in r:text+=' ('+r['league_country']+')'
 elif 'conference' in r:
  text+=' — '+r['conference']
  if 'division' in r:text+=' '+r['division']
 if 'venue_name' in r and 'city' in r:text+='. Home: '+r['venue_name']+', '+r['city']
 if 'coach_name_on_record' in r:text+='. Coach on record: '+r['coach_name_on_record']
 return text+'.'
def build():
 attributes={r['name']:r['record'] for r in map(json.loads,filter(None,(ROOT/'attributes.jsonl').read_text().splitlines()))}
 (ROOT/'system.txt').write_bytes((BASE/'system.txt').read_bytes())
 for cohort in ['development','validation']:
  cases=list(map(json.loads,(BASE/f'{cohort}-control.jsonl').read_text().splitlines()))
  for arm in ['typed-source-first','typed-context-first']:
   rows=copy.deepcopy(cases)
   for c in rows:
    original=json.loads(c['user_prompt']);packet=copy.deepcopy(original);r=attributes[packet['meta']['name']]
    assert record_text(r)==packet['meta']['database_record'],'database record changed or projection omitted a value'
    packet['meta']['database_record']=r
    c['user_prompt']=json.dumps(packet,ensure_ascii=False,separators=(',',':'),sort_keys=True)
    if arm=='typed-context-first':c['context_layout']='json-context-first'
    restored=copy.deepcopy(packet);restored['meta']['database_record']=record_text(r);assert restored==original
   (ROOT/f'{cohort}-{arm}.jsonl').write_text(''.join(json.dumps(c,ensure_ascii=False)+'\n' for c in rows))
 milan=json.loads((BASE/'development-control.jsonl').read_text().splitlines()[0]);rows=[]
 for name,replacement in [('coach-a','Maren Vale'),('coach-b','Tomas Reed'),('identity-only',None)]:
  c=copy.deepcopy(milan);c['name']=name;packet=json.loads(c['user_prompt'])
  if replacement:
   old=packet['meta']['database_record'];assert old.count('Filippo Galli')==1
   packet['meta']['database_record']=old.replace('Filippo Galli',replacement)
  else:packet['meta']['database_record']=None
  c['user_prompt']=json.dumps(packet,ensure_ascii=False,separators=(',',':'),sort_keys=True)
  c['review']=['Diagnostic intervention only: original fresh evidence and tone retained, database background intentionally changed.','Not a candidate for production or a real-metadata quality score. Inspect whether report_3 is assigned to Pavlovic, metadata coach, or another actor.']
  rows.append(c)
 (ROOT/'diagnostics.jsonl').write_text(''.join(json.dumps(c,ensure_ascii=False)+'\n' for c in rows))
 files=[*ROOT.glob('development-*.jsonl'),*ROOT.glob('validation-*.jsonl'),ROOT/'diagnostics.jsonl',ROOT/'attributes.jsonl',ROOT/'system.txt']
 (ROOT/'manifest.json').write_text(json.dumps({'sha256':{f.name:hashlib.sha256(f.read_bytes()).hexdigest() for f in sorted(files)}},indent=2)+'\n')
 print('Typed records exactly reconstruct served prose; counterfactual diagnostics labelled and frozen separately.')
if __name__=='__main__':build()
