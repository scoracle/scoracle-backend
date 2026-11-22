import hashlib,json
from pathlib import Path
ROOT=Path(__file__).resolve().parent;ROLES=ROOT.parent/'roles'
def build():
 base=(ROOT.parents[1]/'production-order-2026-11-01/system.txt').read_text()
 addition='''PREPARATION NOTES\nA report may include preparation with statements and unspecified details. These are fallible source-scoped interpretations, not additional publisher evidence or corroboration. Check every prepared claim against the original headline; omit invented details and preserve any qualification lost by preparation. Keep the original report as the only evidence for attribution. Preparation's about field names what is discussed; speaker names who explicitly says it. An opinion about a person is not that person's opinion. A null speaker must not be filled from an about entity or database metadata. Even an exact copied name or qualification does not prove the interpretation is faithful. An empty unspecified array proves nothing. When preparation is absent, apply the existing evidence rules directly to the original reports.\n\n'''
 marker='APPLY TO THE ACTUAL INPUT\n';assert base.count(marker)==1
 (ROOT/'writer.txt').write_text(base.replace(marker,addition+marker))
 for file in ['development-control.jsonl','development-reference.jsonl','validation-control.jsonl']:
  rows=list(map(json.loads,(ROLES/file).read_text().splitlines()))
  for c in rows:c['output_budget']=1200
  (ROOT/file).write_text(''.join(json.dumps(c,ensure_ascii=False)+'\n' for c in rows))
 for file in ['development-sources.jsonl','validation-sources.jsonl','reference.json','preparer.txt']:(ROOT/file).write_bytes((ROLES/file).read_bytes())
 files=['writer.txt','development-control.jsonl','development-reference.jsonl','validation-control.jsonl','development-sources.jsonl','validation-sources.jsonl','reference.json','preparer.txt']
 (ROOT/'manifest.json').write_text(json.dumps({'sha256':{f:hashlib.sha256((ROOT/f).read_bytes()).hexdigest() for f in files}},indent=2)+'\n')
 print('Restored earlier worked-example writer with optional-note contract; same role notes/source data, exact3 slots and1200 output reservation.')
if __name__=='__main__':build()
