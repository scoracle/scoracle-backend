"""Freeze one-call writer comparisons; source text and publisher values are never rewritten."""
import copy,hashlib,json
from pathlib import Path
ROOT=Path(__file__).resolve().parent
OLD=ROOT.parent/'topic-grouping-2026-11-01'
base=(OLD/'grouped.txt').read_text()
(ROOT/'baseline.txt').write_text(base)
rules='''STATEMENT-LEVEL SOURCE OWNERSHIP
Each factual statement belongs to exactly one report object. Write that publisher's statement separately from another publisher's statement. Several reports about one topic do not mean every publisher supplied every detail. Never put a list of publishers in front of a combined account. Cite only reports actually used; retain a named original source or speaker when the chosen report supplies one. A report may support several statements, but a statement must not borrow a date, number, role, opponent, cause or outcome from a neighboring report. Keep a denial or qualification in its own attributed statement when necessary. Use the anchor report, then only the additional statements needed to preserve the topic and qualifications. You need not exhaust every headline in a group.
An unspecified number remains unspecified: do not supply its currency, unit, financial meaning or relationship to an action. If a detail cannot be paraphrased without interpreting it, quote its exact short wording with attribution or omit that detail while retaining the discussion topic.
A page or headline naming a game, date, summary, score, statistics, highlights or profile does not itself establish that the event happened or supply the named details. Describe what the page lists or discusses. Do not turn a fixture-page title into a completed game, a page labelled highlights into a witnessed performance, or a statistics heading into a numerical result.
'''
assert 'WRITE THE BODY\n' in base
prose=base.replace('WRITE THE BODY\n',rules+'\nWRITE THE BODY\n')
(ROOT/'prose.txt').write_text(prose)
structured=prose
start=structured.index('For each selected report_key, locate that exact report object')
end=structured.index('\n\nWHAT THE WORDS SUPPORT',start)
structured=structured[:start]+'''For each statement, choose one report_key from its topic's reports. The program retrieves the publisher from that exact object and renders the attribution. You do not generate the publisher field. The chosen report alone must support the clause, including who spoke or acted. A correct report_key does not prove that your clause is supported. Within a report, retain any named original source or speaker: repeating another outlet's claim is not independent confirmation. Do not transfer a claim from one report to another.''' +structured[end:]
start=structured.index('WRITE THE BODY\n');end=structured.index('\n\nWRITE THE HEADLINE',start)
structured=structured[:start]+'''WRITE THE STATEMENTS
Return one to three statement objects per topic, each with report_key, attribution and clause. Use one source per statement. attribution must be one of: "reports that", "notes that", "discusses", "asks whether", "lists", "quotes". Choose the form that preserves the source's meaning and certainty. clause supplies the rest of that sentence, with final punctuation. Do not repeat the publisher as an attribution inside clause: code renders publisher + space + attribution + space + clause. A publisher may still occur as the actor or original source when the evidence actually names it in that role. Rendered statements are joined with spaces into one natural paragraph; do not add a separate body or report_keys list.
For example, a statement with attribution="discusses" and clause="Rowan Vale's role and nationality." becomes "Gazette discusses Rowan Vale's role and nationality." Its report_key chooses Gazette; the program does not judge whether the clause is faithful. Preserve actor, action, uncertainty and any qualification within the clause. Keep each clause supported by only its chosen report. Do not write an extra factual bridge between statements. References to body elsewhere in these instructions mean this rendered paragraph.''' +structured[end:]
start=structured.index('Return only JSON in this shape:');end=structured.index('\n\nWORKED EXAMPLES',start)
structured=structured[:start]+'''Return only JSON in this shape: {"narratives":[{"statements":[{"report_key":"report_1","attribution":"discusses","clause":"the supplied topic."}],"headline":"Named subject: specific topic"}]}. Use one entry for each supplied topic group, up to three groups, in their order. Include its anchor report among the statements and preserve every relevant qualification. Each headline is at most 140 characters and each clause at most 1000 characters; the rendered paragraph must fit 1200 characters. These are ceilings, not targets. If no faithful output is possible, an empty list is allowed but will not count as successful topic coverage.''' +structured[end:]
# The same worked-example facts, expressed through the new output contract.
outputs=[
 [('Rowan Vale: profile',[('report_1','discusses',"Rowan Vale's role and nationality.")])],
 [('Rowan Vale: loan reports',[('report_1','reports that','an unnamed source claims Harbor have agreed a loan for Rowan Vale, without confirmation from Harbor.'),('report_2','reports that','Harbor deny a loan agreement for Vale.')])],
 [('Rowan Vale: signing report',[('report_1','reports that','Harbor signed forward Rowan Vale.')]),('Avery Reed: planned talks',[('report_2','reports that','Harbor plan future talks about midfielder Avery Reed.')]),('Taylor Moss: community discussion',[('report_3','discusses',"Taylor Moss's community work.")])],
 [('Rowan Vale: training discussion',[('report_1','reports that','Rowan Vale discussed training.'),('report_1','notes that','the transcript includes an example of a false statement.')])],
 [('Harbor: match report',[('report_1','reports that','Harbor beat Forest 4–2.'),('report_1','reports that','Rowan Vale scored in the 72nd minute.')])],
 [('Rowan Vale: talks with Harbor',[('report_1','reports that','Harbor held talks with Rowan Vale.'),('report_1','notes that','Gazette corrected its initial description of an agreement: no offer was made and no agreement was reached.')])]
]
lines=[];index=0
for line in structured.splitlines():
 if line.startswith('{"narratives":'):
  line=json.dumps({'narratives':[{'statements':[{'report_key':k,'attribution':a,'clause':c} for k,a,c in statements],'headline':h} for h,statements in outputs[index]]},ensure_ascii=False,separators=(',',':'));index+=1
 lines.append(line)
assert index==6
structured='\n'.join(lines)
structured=structured.replace('2. For each entry, locate the selected report objects. Use their publishers and their text together. Write short, attributed, complete statements. Preserve each actor, action, number and qualification. Database attributes do not belong in the body unless those reports state them.','2. For each entry, choose one report_key per statement, then its attribution form and a supported clause. Code supplies that report’s publisher. Preserve each actor, action, number and qualification. Database attributes cannot supply a clause’s claims.')
(ROOT/'structured.txt').write_text(structured)
originals={c['name']:c for c in map(json.loads,(OLD/'cases.jsonl').read_text().splitlines())}
challenges={'ac-milan':[[29,36],[34,37],[39]],'chicago-bears':[[16],[20],[31]],'la-clippers':[[5],[10],[29,36]]}
cases=[]
for cohort in ['known','reserved']:
 for slug,original in originals.items():
  prior=json.loads((OLD/'selected-runs'/f'{slug}-pass-1/selected/input.json').read_text())
  packet=json.loads(prior['user_prompt'])
  full=json.loads((ROOT.parent/'packet-size-2026-11-01'/f'{slug}-runs'/f'{slug}-40-pass-1-json-body-first/input.json').read_text())
  sources=json.loads(full['user_prompt'])['fresh']
  groups=original['reference_groups'][:3] if cohort=='known' else challenges[slug]
  packet['fresh']=[{'topic_key':f'topic_{i}','anchor_report_key':f'report_{ids[0]}','reports':[copy.deepcopy(sources[j-1]) for j in ids]} for i,ids in enumerate(groups,1)]
  keys=sorted({r['report_key'] for g in packet['fresh'] for r in g['reports']})
  schema=copy.deepcopy(prior['schema']);refs=schema['properties']['narratives']['items']['properties']['report_keys'];refs['maxItems']=len(keys);refs['items']['enum']=keys
  user=json.dumps(packet,ensure_ascii=False,separators=(',',':'),sort_keys=True)
  if cohort=='known':assert user==prior['user_prompt'] and schema==prior['schema']
  for g in packet['fresh']:
   for r in g['reports']:assert r==sources[int(r['report_key'].split('_')[1])-1]
  cases.append({'name':f'{slug}-{cohort}','parts':original['parts'],'user_prompt':user,'schema':schema,
   'expect':{'narratives_empty':False},'review':['One statement must preserve its selected source, speaker, actor, uncertainty and scope. Missing topics/qualifications are failures. Page titles do not establish completed events. Unspecified numeric meaning must stay unspecified.'],'cohort':cohort})
(ROOT/'cases-prose.jsonl').write_text(''.join(json.dumps(c,ensure_ascii=False)+'\n' for c in cases))
structured_cases=copy.deepcopy(cases)
for case in structured_cases:
 keys=case['schema']['properties']['narratives']['items']['properties']['report_keys']['items']['enum']
 case['schema']={'type':'object','additionalProperties':False,'required':['narratives'],'properties':{'narratives':{'type':'array','maxItems':3,'items':{'type':'object','additionalProperties':False,'required':['statements','headline'],'properties':{'statements':{'type':'array','minItems':1,'maxItems':3,'items':{'type':'object','additionalProperties':False,'required':['report_key','attribution','clause'],'properties':{'report_key':{'type':'string','enum':keys},'attribution':{'type':'string','enum':['reports that','notes that','discusses','asks whether','lists','quotes']},'clause':{'type':'string','minLength':1,'maxLength':1000}}}},'headline':{'type':'string','minLength':1,'maxLength':140}}}}}}
(ROOT/'cases-structured.jsonl').write_text(''.join(json.dumps(c,ensure_ascii=False)+'\n' for c in structured_cases))
assert all(a['user_prompt']==b['user_prompt'] and a['parts']==b['parts'] for a,b in zip(cases,structured_cases))
manifest={'source_provenance':'../packet-size-2026-11-01/manifest.json','groups':{c['name']:[[r['report_key'] for r in g['reports']] for g in json.loads(c['user_prompt'])['fresh']] for c in cases},'reserved_means':'These exact topic packets have not previously been generated or tuned against. Headlines come from the same previously inspected 120-report corpus; not independently unseen subjects or news.',
 'sha256':{p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in ROOT.iterdir() if p.name in ['baseline.txt','prose.txt','structured.txt','cases-prose.jsonl','cases-structured.jsonl']}}
(ROOT/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print('Frozen six real-data packets across three subjects; prompts and statement schemas saved.')
