import json,pathlib,shutil
r=pathlib.Path('fixtures/characters/context-guide-2026-11-02')
notes={
'narratives-person-86-FOOTBALL': ['Keeps two distinct discussions and the correct speakers and publishers. Some redundant attribution across duplicate headlines.','Preserves the same two stories and qualifications.','Preserves the same two stories; the Mastantuono headline stays a restrained topic label.'],
'narratives-player-37-NBA': ['Combines knee absence and jersey-number news in one entry, contrary to distinct-story selection.','Still bundles unrelated stories. Native number/subject guard rejects the Chris Paul/No. 3 construction; source does contain the old number, so guard rejection is not by itself a semantic verdict.','Same story bundling and number/subject guard rejection remain.'],
'vibe-person-23-FOOTBALL': ['Supported delight and happiness, preserving hope about attitude and mentality.','Keeps positive tone but weakens the distinction between hope, praise and the report crediting a bonding trip for improvement.','Keeps the reported positive reactions but turns hope about attitude and mentality into praise of them.'],
'vibe-person-959-NFL': ['Correctly keeps the ex-player criticism aimed at Jones, but leaks instruction language into the output.','Misassigns criticism to Jones himself and invents a conflict/division between two negative reports.','Corrects the criticism direction but pluralizes one former star and still calls two negative reports conflicting/mixed.'],
'rating-team-2-NBA': ['Correct main ranks and supplied directions, with the 89-game sample; restrained interpretation still preferable to dominant/stagnant language.','Main ranks and directions survive, but phrasing 46.4 rebounds across 89 games can imply a total that the projection does not establish.','Keeps target sample separate from peer count, retains per-measure values, bands and directions. Longer than necessary; no wrong measurement detected.'],
'rating-player-744-NFL': ['Keeps 828 players, supplied ranks and five-appearance thin sample.','Incorrectly changes the comparison group to 828 teams despite a player target.','Uses 828 peers, preserves the player measurements and five-appearance limit. Threshold explanation concerns comparison, not a newly computed rank.'],
'momentum-player-744-NFL': ['Changes 91.7/90.8 percentiles to top 90th and strengthens preparedness to get paid elsewhere into desire to leave.','Avoids the invented percentile and desire to leave; describes current contrast and lack of an observed time comparison.','Again strengthens conditional preparedness into desire to leave, despite accurate current performance/coverage contrast.'],
'momentum-synthetic-dated-divergence': ['Calls different start dates the same window and prints internal measure names.','Uses ordinary measure meanings, but last two days is loosely anchored and a similar window hides the exact separate date ranges.','Again claims a shared two-observation window and conflates statistical distinctiveness with performance. The windows have different start dates.'],
'oracle-team-2-NBA': ['Mislabels composite 54.2 as poor and invents strategic disjointedness.','Reverses elite rebounding and turnovers into poor measures and adds a defensive causal story.','Rebounding is restored but elite turnovers still become poor. Adds unsupported structural/functional diagnoses.'],
'oracle-player-744-NFL': ['Expands a supplied rising direction label into rising on-field metrics without dated underlying comparisons.','Keeps elite relative ranks, five appearances and contract frustration, but its disconnect framing is more emphatic than necessary.','Keeps the core contrast and sample limit; complex disconnect and future stability are embellishments rather than useful evidence distinctions.'],
 'transfer-team-37-FOOTBALL': ['Turns reported injury absences of Pellegrini/Svilar into departures.','Correctly calls the Pellegrini absence an injury, though including it is outside the movement brief.','Departure error returns; also attaches the 50m package too loosely to Roma/El Aynaoui although the source describes Leipzig retaining two players.'],
 'transfer-player-83946-NFL': ['Correct signing/practice-squad story but adopts publisher draft-bust characterization as its own.','Preserves Raiders release before Eagles signing, but still adopts draft-bust characterization without attribution.','Same issue; source/alias parser also rejects New York Giants. No findings qualify in any arm under the full-name quote contract.'],
}
records=[]
for case in json.loads((r/'selection.json').read_text()):
 name=case['name'];record={'case':name,'task':case['task'],'synthetic':case['synthetic']}
 settings=[]
 for i,(arm,folder) in enumerate([('control',r/'trial/control'),('initial',r/'trial/candidate'),('refined',r/'refinement')]):
  p=folder/name/'01';receipt=json.loads((p/'result.json').read_text())
  request=json.loads((p/'request.json').read_text());request.pop('prompt');settings.append(request)
  record[arm]={'status':receipt['status'],'note':notes[name][i],'output':json.loads((p/'output.json').read_text()),'receipt':str((p/'result.json').relative_to(r))}
 assert settings[0]==settings[1]==settings[2],name
 records.append(record)
rows=[json.loads(l) for l in (r/'refinement/results.jsonl').read_text().splitlines()]
assert len(rows)==12 and all(x['completion_reason']=='eos' for x in rows)
totals=dict(calls=len(rows),accepted=sum(x['status']=='accepted' for x in rows),input_tokens=sum(x['input_tokens'] for x in rows),output_tokens=sum(x['output_tokens'] for x in rows),wall_seconds=sum(x['wall_ms'] for x in rows)/1000)
(r/'review.json').write_text(json.dumps({'method':'Manual comparison of each complete output against its captured input. Mechanical acceptance and source fidelity are separate. Development cases, not held-out accuracy.','refined_totals':totals,'records':records},indent=2,ensure_ascii=False)+'\n')
lines=['# Context guide: complete outputs and source review','', 'Same evidence, schema and generation settings in all three arms. These are development cases; parser acceptance is not factual validation.','']
for item in records:
 lines += ['## '+item['case'],'']
 for arm in ['control','initial','refined']:
  value=item[arm];lines += ['### '+arm+' — '+value['status'],'','```json',json.dumps(value['output'],ensure_ascii=False,indent=2),'```','',value['note'],'']
(r/'outputs.md').write_text('\n'.join(lines)+'\n')
shutil.copy2('/private/tmp/scoracle-context-guide-refined-tests.txt',r/'validation.txt')
# Human-readable view of the exact assembled guide, with full JSON alongside it.
fx=json.loads((r/'cases/refined/narratives-person-86-FOOTBALL.json').read_text());packet=json.loads(fx['user_prompt'])
lines=['# Journalist prompt preview','', 'The system message supplies the detailed output contract. The assembled request below uses the same runtime path as production.','', '## Job','',packet['job'],'','## Context guide','']
for field,guide in packet['context_guide'].items():lines += ['### '+field,'',guide,'']
lines += ['## Target data','','```json',json.dumps(packet['meta'],ensure_ascii=False,indent=2),'```','','## Fresh reporting','']
for report in packet['fresh']:
 lines += ['- **'+report['report_key']+' · '+report['publisher']+'** ('+report['published_at']+', '+report['evidence_scope']+'): '+report.get('headline',report.get('publisher_excerpt',''))]
lines += ['', 'URLs and article IDs are preserved in the [complete request](cases/refined/narratives-person-86-FOOTBALL.json).','', '## Generated output','', '```json',json.dumps(records[0]['refined']['output'],ensure_ascii=False,indent=2),'```','']
(r/'prompt-preview.md').write_text('\n'.join(lines))
print(totals)
