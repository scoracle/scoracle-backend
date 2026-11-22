import hashlib,importlib.util,json,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent
sys.path.insert(0,str(ROOT.parent))
spec=importlib.util.spec_from_file_location('preparation_run',ROOT.parent/'run.py');stage=importlib.util.module_from_spec(spec);spec.loader.exec_module(stage);stage.ROOT=ROOT

def validate(note,text):
 assert 1<=len(note['statements'])<=3 and len(note['unspecified'])<=4
 for s in note['statements']:
  assert len(s['about'])<=4 and all(v and v in text for v in s['about'])
  assert s['speaker'] is None or s['speaker'] and s['speaker'] in text
  assert s['kind'] in ['assertion','opinion','question','listing'] and s['claim'].strip()
  assert len(s['qualifications'])<=5 and all(q and q in text for q in s['qualifications'])
 assert all(x.strip() for x in note['unspecified'])
stage.validate=validate
for n,d in json.loads((ROOT/'manifest.json').read_text())['sha256'].items():assert hashlib.sha256((ROOT/n).read_bytes()).hexdigest()==d
before=stage.base.request('/v1/models');assert before['data'][0]['id']==stage.base.MODEL
mode=sys.argv[1]
if mode=='development':
 assert json.loads((ROOT/'screen-selection.json').read_text())['qualifies_for_automatic']
 rows=stage.admit('development-sources.jsonl','prepare');(ROOT/'admission-preparation.json').write_text(json.dumps(rows,indent=2)+'\n');assert all(r['fits'] for r in rows)
 stage.run('development-sources.jsonl','prepare','development-preparation')
 if stage.attach('development'):
  rows=stage.admit('development-automatic.jsonl','write');(ROOT/'admission-automatic.json').write_text(json.dumps(rows,indent=2)+'\n');assert all(r['fits'] for r in rows)
  stage.run('development-automatic.jsonl','write','development-automatic')
elif mode=='validation':
 assert json.loads((ROOT/'selection.json').read_text())['qualifies_for_followup']
 stage.run('development-automatic.jsonl','write','repeat-automatic')
 rows=stage.admit('validation-sources.jsonl','prepare')+stage.admit('validation-control.jsonl','write');(ROOT/'admission-validation.json').write_text(json.dumps(rows,indent=2)+'\n');assert all(r['fits'] for r in rows)
 stage.run('validation-sources.jsonl','prepare','validation-preparation')
 stage.run('validation-control.jsonl','write','validation-control')
 if stage.attach('validation'):
  rows=stage.admit('validation-automatic.jsonl','write');(ROOT/'admission-validation-automatic.json').write_text(json.dumps(rows,indent=2)+'\n');assert all(r['fits'] for r in rows)
  stage.run('validation-automatic.jsonl','write','validation-automatic')
else:raise ValueError(mode)
after=stage.base.request('/v1/models');(ROOT/f'model-after-automatic-{mode}.json').write_text(json.dumps(after,indent=2)+'\n')
for obj in [before,after]:
 for m in obj['data']:m.pop('created',None)
assert before==after
