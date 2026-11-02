"""Freeze general boundary examples and an optional uncertainty-before-writing contract."""
import copy,hashlib,json
from pathlib import Path
ROOT=Path(__file__).resolve().parent
OLD=ROOT.parent/'statement-ownership-2026-11-01'
control=(OLD/'structured.txt').read_text()
(ROOT/'control.txt').write_text(control)
addition='''BOUNDARY AND LANGUAGE PRACTICE
An incomplete headline deserves an incomplete factual account, not an invented explanation. Before writing, distinguish words the report states from details you would need to explain those words. A streak can stay an unspecified streak; a figure can stay unspecified or be omitted; a live show can stay a live show. Do not supply the missing category from memory, the subject's sport, division, role or location. Do not expand a show into a game broadcast. Missing information is not permission to infer a likely answer.
Keep explicit limits even when they do not explain themselves: if two conditions are needed, retain that condition without inventing the conditions. If a question is followed by an unlikely outcome, preserve both the question/possibility and the unlikely qualification. Quotation marks around a question's wording do not convert it into an affirmative report.
The attribution and clause must form a grammatical sentence. After "reports that" or "notes that", use a complete subject-and-verb clause. After "discusses", "lists" or "quotes", use the thing discussed/listed/quoted, not another standalone sentence. After "asks whether", use the unresolved proposition. For "Avery says the club is improving", use "reports that" + "Avery says the club is improving." or "discusses" + "Avery's assessment of the club." Never join "discusses" to "Avery says ...". Preserve the source's wording when a fluent paraphrase would add information.
A title is a restrained label for an actual statement. It need not name every person or detail. When different people have different roles or statuses, name one supported topic or state their roles separately; never give them a shared injury, transfer, opinion or action just to shorten the label. Interest is not negotiations, an illness is not a stated injury, and a discussion of a possible move is not a move announcement. Choose the narrower label when a broader label suggests an extra fact.

Example G: retain an unspecified meaning without completing it.
Input:
{"meta":{"name":"Harbor","database_record":"Northern division. Home: River Stadium."},"fresh":[{"topic_key":"topic_1","anchor_report_key":"report_1","reports":[{"report_key":"report_1","publisher":"Gazette","headline":"Jordan Moss set to end an 11-year streak","evidence_scope":"headline"}]}],"tone":"Clear and direct."}
Correct output:
{"narratives":[{"statements":[{"report_key":"report_1","attribution":"reports that","clause":"Jordan Moss is set to end an 11-year streak."}],"headline":"Jordan Moss: streak report"}]}
The source does not explain the type of streak. Neither division nor venue supplies it. Keep the supplied future status; do not invent wins, appearances, absences, opponents or occasions.

Example H: avoid assigning an amount a meaning the source does not give.
Input:
{"meta":{"name":"Harbor"},"fresh":[{"topic_key":"topic_1","anchor_report_key":"report_1","reports":[{"report_key":"report_1","publisher":"Gazette","headline":"Harbor move for Rowan Vale: 42 million next summer","evidence_scope":"headline"}]}],"tone":"Clear and direct."}
Correct output:
{"narratives":[{"statements":[{"report_key":"report_1","attribution":"discusses","clause":"Harbor's reported move for Rowan Vale."}],"headline":"Rowan Vale: move report"}]}
Omitting the undefined figure preserves the topic. It must not become a fee, budget, offer, price, valuation, currency or agreement. An exact short quotation is also permissible, but adding an interpretation around it is not.

Example I: different statuses require distinct wording and preserved conditions.
Input:
{"meta":{"name":"Harbor"},"fresh":[{"topic_key":"topic_1","anchor_report_key":"report_1","reports":[{"report_key":"report_1","publisher":"Gazette","headline":"Rowan Vale ill; could Avery Reed return? Two approvals needed","evidence_scope":"headline"},{"report_key":"report_2","publisher":"Ledger","headline":"Avery Reed return? Early exit deemed unlikely","evidence_scope":"headline"}]}],"tone":"Clear and direct."}
Correct output:
{"narratives":[{"statements":[{"report_key":"report_1","attribution":"reports that","clause":"Rowan Vale is ill and discusses a possible Avery Reed return requiring two approvals."},{"report_key":"report_2","attribution":"discusses","clause":"a possible Avery Reed return, with an early exit deemed unlikely."}],"headline":"Avery Reed: return question"}]}
Only Vale is described as ill. The title selects Reed's return question rather than implying both people are ill. The two-approval condition and unlikely qualification survive.
'''
# Example I must not make Vale the actor discussing the return.
addition=addition.replace('"attribution":"reports that","clause":"Rowan Vale is ill and discusses a possible Avery Reed return requiring two approvals."','"attribution":"discusses","clause":"Rowan Vale\'s illness and a possible Avery Reed return requiring two approvals."')
marker='APPLY TO THE ACTUAL INPUT\n'
assert control.count(marker)==1
revised=control.replace(marker,addition+'\n'+marker)
(ROOT/'examples.txt').write_text(revised)
gap_rule='''UNSPECIFIED DETAILS BEFORE WRITING
Each statement has an unspecified array immediately after report_key. Before its attribution and clause, list up to three short descriptions of missing details that could change the statement if guessed, such as the type of an unexplained streak, what an amount represents, or an unidentified speaker. Use [] when none is needed for this statement. This is a record of evidence limits, not a claim about the world or a place for reasoning. Do not list unrelated trivia. Your clause and title must leave these details unresolved: omit them, preserve the source wording, or describe the question without answering it. The program does not verify this list for you, and correctly naming a gap does not justify filling it in the clause.
'''
gaps=revised.replace('WRITE THE STATEMENTS\n',gap_rule+'\nWRITE THE STATEMENTS\n').replace('each with report_key, attribution and clause','each with report_key, unspecified, attribution and clause')
lines=[]
for line in gaps.splitlines():
 if line.startswith('{"narratives":'):
  value=json.loads(line)
  for entry in value['narratives']:
   for s in entry['statements']:
    clause=s['clause']
    unknown=[]
    if '11-year streak' in clause:unknown=['type of streak']
    elif "Harbor's reported move for Rowan Vale" in clause:unknown=['meaning and unit of the amount']
    elif "role and nationality" in clause:unknown=['role','nationality']
    elif 'example of a false statement' in clause:unknown=['speaker of the example statement']
    s['unspecified']=unknown
  line=json.dumps(value,ensure_ascii=False,separators=(',',':'))
 lines.append(line)
gaps='\n'.join(lines).replace('"report_key":"report_1","attribution":"discusses","clause":"the supplied topic."','"report_key":"report_1","unspecified":[],"attribution":"discusses","clause":"the supplied topic."')
(ROOT/'gaps.txt').write_text(gaps)
cases=list(map(json.loads,(OLD/'cases-structured.jsonl').read_text().splitlines()))
(ROOT/'cases.jsonl').write_text(''.join(json.dumps(c,ensure_ascii=False)+'\n' for c in cases))
gap_cases=copy.deepcopy(cases)
for case in gap_cases:
 statement=case['schema']['properties']['narratives']['items']['properties']['statements']['items']
 statement['required'].append('unspecified')
 statement['properties']['unspecified']={'type':'array','maxItems':3,'items':{'type':'string','minLength':1,'maxLength':80}}
(ROOT/'cases-gaps.jsonl').write_text(''.join(json.dumps(c,ensure_ascii=False)+'\n' for c in gap_cases))
assert all(a['parts']==b['parts'] and a['user_prompt']==b['user_prompt'] for a,b in zip(cases,gap_cases))
paths=[ROOT/n for n in ['control.txt','examples.txt','gaps.txt','cases.jsonl','cases-gaps.jsonl']]
(ROOT/'development-manifest.json').write_text(json.dumps({'sha256':{p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},'source_provenance':'../packet-size-2026-11-01/manifest.json'},indent=2)+'\n')
print('Frozen development inputs and general examples; no live subject names in new instructions.')
