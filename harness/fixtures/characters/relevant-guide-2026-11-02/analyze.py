"""Verify paired evidence/settings and summarize complete native-token receipts."""
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent

def read(path):
    return json.loads(path.read_text())

if __name__ == '__main__':
    cases = []
    totals = {}
    notes = read(ROOT / 'review-notes.json') if (ROOT / 'review-notes.json').exists() else {}
    for case in read(ROOT / 'selection.json'):
        name = case['name']
        row = dict(case=name, task=case['task'], synthetic=case['synthetic'])
        settings, evidence, parts, providers = [], [], [], []
        for arm in ['baseline', 'candidate', 'final', 'followup']:
            matches = list(ROOT.glob(f'batch-*/{arm}/{name}/01')) if arm in ['baseline', 'candidate'] else list(ROOT.glob(f'{arm}/{name}/01'))
            if not matches:
                continue
            assert len(matches) == 1
            folder = matches[0]
            result = read(folder / 'result.json')
            prepared = read(folder / 'prepared.json')
            request = read(folder / 'request.json')
            tokens = len(prepared['coverage']['token_ids'])
            assert result['input_tokens'] == tokens, (name, arm, 'native coverage mismatch')
            assert request.pop('prompt') == prepared['coverage']['token_ids']
            assert prepared['coverage']['rendered_prompt'].count((folder / 'prompt.json').read_text()) == 1
            settings.append(request)
            providers.append(prepared['coverage']['provider'])
            evidence.append((folder / 'prompt.json').read_text().split('\n"meta":', 1)[1])
            parts.append(read(folder / 'fixture.json')['parts'])
            row[arm] = dict(status=result['status'], input_tokens=tokens, output_tokens=result['output_tokens'],
                output=read(folder / 'output.json'), note=notes.get(name, {}).get(arm, ''),
                receipt=str((folder / 'result.json').relative_to(ROOT)))
            tally = totals.setdefault(arm, dict(calls=0, accepted=0, input_tokens=0, output_tokens=0, wall_ms=0))
            tally['calls'] += 1
            tally['accepted'] += result['status'] == 'accepted'
            for key in ['input_tokens', 'output_tokens', 'wall_ms']:
                tally[key] += result[key]
        assert all(s == settings[0] for s in settings), (name, 'settings')
        assert all(p == providers[0] for p in providers), (name, 'provider')
        assert all(e == evidence[0] for e in evidence), (name, 'evidence bytes')
        assert all(p == parts[0] for p in parts), (name, 'parts')
        cases.append(row)
    variants = read(ROOT / 'selected-variants.json')
    selected = [r[variants[r['task']]] for r in cases]
    for row in cases:
        row['selected_arm'] = variants[row['task']]
    current = dict(calls=len(selected), accepted=sum(r['status']=='accepted' for r in selected),
        input_tokens=sum(r['input_tokens'] for r in selected), output_tokens=sum(r['output_tokens'] for r in selected)) if all(selected) else None
    summary = dict(totals=totals, current=current, unique_generations=sum(t['calls'] for t in totals.values()), cases=cases,
        caveat='Development cases, no held-out accuracy claim. Mechanical acceptance is not factual correctness. Shared-server timings are not throughput measurements.')
    (ROOT / 'review.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2)+'\n')
    lines = ['# Complete outputs and manual source review', '', summary['caveat'], '']
    for row in cases:
        lines += ['## '+row['case'], '', 'Current source uses the **'+row['selected_arm']+'** arm for this character.', '']
        for arm in ['baseline','candidate','final','followup']:
            if arm not in row: continue
            r = row[arm]
            lines += [f"### {arm} — {r['status']} — {r['input_tokens']} input tokens", '',
                '```json', json.dumps(r['output'], ensure_ascii=False, indent=2), '```', '', r['note'], '',
                f"[Receipt]({r['receipt']})", '']
    (ROOT / 'outputs.md').write_text('\n'.join(lines))
    print(json.dumps(dict(totals=totals, current=current), indent=2))
