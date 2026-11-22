"""Compare full native inputs and outputs; keep parser results separate from source review."""
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent
PREVIOUS = ROOT.parent / 'shared-prompt-2026-11-02'


def main():
    previous = {r['case']: r for r in json.loads((PREVIOUS / 'review.json').read_text())['cases']}
    rows = []
    lines = ['# Context-component development outputs', '',
        'Only Journalist, Insider and Influencer. Temperature 0, thinking disabled, concurrency 1, no retries. '
        'Mechanical acceptance is not factual accuracy.', '']
    for path in sorted((ROOT / 'outputs').glob('[0-9]*/result.json')):
        result = json.loads(path.read_text())
        case = pathlib.Path(result['fixture']).stem
        old = previous[case]
        prepared = json.loads(path.with_name('prepared.json').read_text())
        before = json.loads((PREVIOUS / old['receipt']).with_name('prepared.json').read_text())
        tokens = len(prepared['coverage']['token_ids'])
        assert tokens == result['input_tokens'] == old['input_tokens'] < 4096
        assert prepared['coverage']['token_ids'] == before['coverage']['token_ids']
        assert result['temperature'] == 0 and result['completion_reason'] == 'eos'
        output = json.loads(path.with_name('output.json').read_text())
        assert output == old['output'], f'Manually review changed output before reusing notes: {case}'
        row = dict(case=case, task=result['task'], articles=old['articles'],
            input_tokens=tokens, baseline_input_tokens=old['input_tokens'],
            output_tokens=result['output_tokens'], status=result['status'],
            native_token_ids_match=True, output_matches_baseline=True,
            review=old['review'], output=output, receipt=str(path.relative_to(ROOT)))
        rows.append(row)
        lines += [f"## {case}", '', f"{row['articles']} articles; {tokens} complete native input tokens; {row['status']}.", '',
            '```json', json.dumps(output, ensure_ascii=False, indent=2), '```', '', row['review'], '',
            f"[Receipt]({row['receipt']})", '']
    assert len(rows) == 7
    summary = dict(model_calls=7, accepted=sum(r['status']=='accepted' for r in rows),
        qualification='Development comparison on identical evidence; no held-out accuracy claim.',
        input_tokens_saved_by_this_refactor=0, cases=rows)
    (ROOT / 'review.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2) + '\n')
    (ROOT / 'outputs.md').write_text('\n'.join(lines))
    (ROOT / 'input-tokens.json').write_text(json.dumps([
        {k:r[k] for k in ('case','task','articles','input_tokens','baseline_input_tokens','output_tokens','native_token_ids_match')}
        for r in rows], indent=2) + '\n')
    print(f"Seven complete replays; {summary['accepted']} mechanically accepted; native inputs and outputs match baseline.")


if __name__ == '__main__':
    main()
