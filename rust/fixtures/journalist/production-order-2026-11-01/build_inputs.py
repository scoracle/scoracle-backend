"""Keep production-captured object order when forming experimental topic groups."""
import copy,json,hashlib
from pathlib import Path
ROOT=Path(__file__).resolve().parent
BASE=ROOT.parent/'context-boundaries-2026-11-01'
encode=lambda obj:json.dumps(obj,ensure_ascii=False,separators=(',',':'))
def build():
 captures={r['subject']['name']:r for r in map(json.loads,(ROOT.parent/'packet-size-2026-11-01/live-capture/results.jsonl').read_text().splitlines())}
 cases=list(map(json.loads,(BASE/'development-control.jsonl').read_text().splitlines()))
 rows=[]
 for c in cases:
  c=copy.deepcopy(c);original=json.loads(c['user_prompt']);record=captures[original['meta']['name']]
  text=record['input']['messages'][1]['content'];production=json.loads(text)
  assert encode(production)==text,'JSON reserialization changed production bytes'
  assert c['parts']['subject']==record['parts']['subject'] and c['parts']['database_record']==record['parts']['database_record']
  assert {r['id']:r for r in c['parts']['reports']}=={r['id']:r for r in record['parts']['reports']}
  # Earlier trials promoted an anchor; match durable article IDs, not request-local slots.
  sources={article['id']:report for article,report in zip(record['parts']['reports'],production['fresh'],strict=True)}
  groups=copy.deepcopy(original['fresh'])
  for group in groups:
   for i,source in enumerate(group['reports']):
    article=c['parts']['reports'][int(source['report_key'].split('_')[1])-1]
    ordered=copy.deepcopy(sources[article['id']]);ordered['report_key']=source['report_key']
    assert ordered==source
    group['reports'][i]=ordered
  production['fresh']=groups
  assert production==original,'a semantic value changed'
  c['user_prompt']=encode(production);rows.append(c)
 (ROOT/'cases.jsonl').write_text(''.join(json.dumps(c,ensure_ascii=False)+'\n' for c in rows))
 (ROOT/'system.txt').write_bytes((BASE/'system.txt').read_bytes())
 (ROOT/'manifest.json').write_text(json.dumps({'sha256':{n:hashlib.sha256((ROOT/n).read_bytes()).hexdigest() for n in ['cases.jsonl','system.txt']},'source':'../packet-size-2026-11-01/live-capture/results.jsonl','contract':'Production-captured top-level and nested meta/report field order; experimental group wrappers retain their prior order. Same grouped semantics, same statement schema/system; not a full production pipeline replay.'},indent=2)+'\n')
 print('Restored production-captured field order without changing any supplied value or source membership.')
if __name__=='__main__':build()
