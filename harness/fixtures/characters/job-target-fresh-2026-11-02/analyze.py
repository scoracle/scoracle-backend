"""Summarize paired receipts separately from the source review."""
import json
import pathlib

ROOT=pathlib.Path(__file__).resolve().parent


def main():
    rows=[json.loads(line) for line in (ROOT/'trial/results.jsonl').read_text().splitlines()]
    assert len(rows)==24
    totals={}
    records=[]
    for arm in ['control','candidate']:
        group=[r for r in rows if r['arm']==arm]
        assert len(group)==12 and all(r['status'] in ['accepted','rejected'] for r in group)
        totals[arm]=dict(calls=len(group),accepted=sum(r['status']=='accepted' for r in group),
            input_tokens=sum(r['input_tokens'] for r in group),output_tokens=sum(r['output_tokens'] for r in group),
            wall_seconds=sum(r['wall_ms'] for r in group)/1000)
    for case in json.loads((ROOT/'selection.json').read_text()):
        pair=dict(case=case['name'],task=case['task'],synthetic=case['synthetic'])
        settings=[]
        for arm in ['control','candidate']:
            root=ROOT/'trial'/arm/case['name']/'01'
            result=json.loads((root/'result.json').read_text())
            request=json.loads((root/'request.json').read_text());request.pop('prompt')
            settings.append(request)
            pair[arm]=dict(status=result['status'],parser_detail=result.get('reading'),
                output=json.loads((root/'output.json').read_text()),receipt=str((root/'result.json').relative_to(ROOT)))
        assert settings[0]==settings[1], 'only the messages should differ'
        records.append(pair)
    result=dict(totals=totals,elapsed=json.loads((ROOT/'trial/elapsed.json').read_text()),records=records,
        caveat='Parser acceptance is not semantic fidelity. Two purposive development cases per role do not estimate routine accuracy. Shared workstation timings are not throughput certification.')
    (ROOT/'analysis.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
    print(json.dumps(totals,indent=2))


if __name__=='__main__':main()
