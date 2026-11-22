"""Per-request native tokens and manual source review; acceptance is not accuracy."""
import json
import pathlib

ROOT=pathlib.Path(__file__).resolve().parent
NOTES={
'narratives-chelsea-top-five': "Incorrectly makes Chelsea the source of Estevao's transfer advice and headlines it as advice to leave, neither established by that headline. Pluralizes the single Chelsea player with 23 minutes. The accusation is repeated without explicit publisher attribution. Smaller inputs do not fix these errors.",
'narratives-person-86-FOOTBALL': "Keeps Diomande praise and the Mastantuono discussion separate with the correct actor. 'Time to develop' expands the source's 'needs time'. Broadly preserves the two topics; no general improvement claim.",
'narratives-player-37-NBA': "Separates the jersey story from injury reporting, unlike the prior merged output. Covers the two injury games in separate entries; they could also have been grouped. Correct publishers and Chris Paul's old No. 3 survive. Parser accepts this wording.",
'transfer-player-83946-NFL': "Preserves signing after release but turns signing WITH an NFC East rival into competing WITH one. Repeats draft-bust language through broad outlet attribution. The unchanged parser rejects the expanded Philadelphia Eagles name; findings remain empty. The top five exclude the official club-publisher report because its movement score is lower.",
'transfer-team-37-FOOTBALL': "Invents coach as Pellegrini's role and includes unrelated injury coverage. Ends by claiming no reported movement despite its own signing-plan sentence and the selected Leipzig retention headline. Actor and relevance errors remain despite five articles.",
'vibe-person-23-FOOTBALL': "Keeps positive expressed reactions with De Zerbi and preserves the hope about attitude/mentality. Gives three sentences instead of the requested one or two; within the parser's character limit, so accepted. No invented population-wide mood detected.",
'vibe-person-959-NFL': "Correctly separates Jones's disappointment from the ex-player's criticism of Jones, fixing the previous actor reversal on this case. Still calls two negative reactions mixed and scores them 50; no conflicting valence is established. Three sentences exceed the requested one or two."
}

if __name__=='__main__':
    measured={r['case']:r for r in json.loads((ROOT/'input-tokens.json').read_text())}
    old=json.loads((ROOT.parent/'relevant-guide-2026-11-02/review.json').read_text())
    old={r['case']:r[r['selected_arm']]['input_tokens'] for r in old['cases']}
    old['narratives-chelsea-top-five']=9929
    rows=[];lines=['# Seven capped outputs and source review','','One local generation per case; no retries. Mechanical acceptance is not factual accuracy.','']
    for path in sorted((ROOT/'outputs').glob('*/result.json')):
        result=json.loads(path.read_text());case=pathlib.Path(result['fixture']).stem
        assert result['task'] in ['narratives','transfer','vibe']
        prepared=json.loads(path.with_name('prepared.json').read_text())
        tokens=len(prepared['coverage']['token_ids'])
        assert tokens==result['input_tokens']==measured[case]['input_tokens']<4096
        assert result['temperature']==0 and result['completion_reason']=='eos'
        output=json.loads(path.with_name('output.json').read_text())
        row=dict(case=case,task=result['task'],articles=measured[case]['articles'],previous_input_tokens=old[case],input_tokens=tokens,output_tokens=result['output_tokens'],status=result['status'],review=NOTES[case],output=output,receipt=str(path.relative_to(ROOT)))
        rows.append(row)
        lines += ['## '+case,'',f"{row['articles']} articles; {tokens} complete native input tokens; parser: {row['status']}.",'','```json',json.dumps(output,ensure_ascii=False,indent=2),'```','',NOTES[case],'',f"[Receipt]({row['receipt']})",'']
    assert len(rows)==7
    (ROOT/'review.json').write_text(json.dumps(dict(model_calls=7,accepted=sum(r['status']=='accepted' for r in rows),cases=rows),ensure_ascii=False,indent=2)+'\n')
    (ROOT/'outputs.md').write_text('\n'.join(lines))
    # Full system/user messages are review artifacts, not additional model calls.
    systems=json.loads((ROOT/'instructions.json').read_text());roles=dict(narratives='journalist',transfer='insider',vibe='influencer')
    lines=['# Current complete requests','','Within each JSON request: job → context_guide → meta and fresh. There is no tone field.','']
    for path in sorted((ROOT/'cases').glob('*.json')):
        f=json.loads(path.read_text());system=systems[roles[f['task']]].strip().split('\n\n',1)[1]
        lines+=['## '+path.stem,'','### System contract','',system,'','### JSON request','','```json',json.dumps(json.loads(f['user_prompt']),ensure_ascii=False,indent=2),'```','']
    (ROOT/'prompt-preview.md').write_text('\n'.join(lines))
    print(f"7 completed calls, {min(r['input_tokens'] for r in rows)}–{max(r['input_tokens'] for r in rows)} input tokens per request; all native counts verified.")
