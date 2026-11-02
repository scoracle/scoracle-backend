"""Verify native token receipts and retain source review for both refactor batches."""
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent
BASELINE = ROOT.parent / 'top-five-2026-11-02'


def main():
    previous = {r['case']: r for r in json.loads((BASELINE / 'review.json').read_text())['cases']}
    rows = []
    lines = ['# Shared-prompt output comparison', '',
             'One generation per case per batch; temperature 0, thinking disabled, concurrency 1, no retries. '
             'Only Journalist, Insider and Influencer. Acceptance is mechanical, not factual accuracy.', '',
             '## Final retained assembly', '']
    for path in sorted((ROOT / 'outputs').glob('[0-9]*/result.json')):
        result = json.loads(path.read_text())
        case = pathlib.Path(result['fixture']).stem
        old = previous[case]
        output = json.loads(path.with_name('output.json').read_text())
        prepared = json.loads(path.with_name('prepared.json').read_text())
        baseline_prepared = json.loads((BASELINE / 'outputs' / path.parent.name / 'prepared.json').read_text())
        tokens = len(prepared['coverage']['token_ids'])
        assert tokens == result['input_tokens'] == old['input_tokens'] < 4096
        assert prepared['coverage']['token_ids'] == baseline_prepared['coverage']['token_ids']
        assert result['temperature'] == 0 and result['completion_reason'] == 'eos'
        assert output == old['output'], f'New output requires a new manual review: {case}'
        row = dict(case=case, task=result['task'], articles=old['articles'],
                   input_tokens=tokens, previous_input_tokens=old['input_tokens'],
                   output_tokens=result['output_tokens'], status=result['status'],
                   native_input_identical=True, output_matches_baseline=True,
                   review=old['review'], output=output, receipt=str(path.relative_to(ROOT)))
        rows.append(row)
        lines += [f"### {case}", '', f"{row['articles']} articles; {tokens} native input tokens; {row['status']}. Identical parsed JSON to baseline.", '',
                  '```json', json.dumps(output, ensure_ascii=False, indent=2), '```', '',
                  row['review'], '', f"[Receipt]({row['receipt']})", '']
    assert len(rows) == 7
    trial = json.loads((ROOT / 'voice-at-end-review.json').read_text())
    lines += ['## Discarded sentence-order variant', '',
              trial['reason'], '', 'All seven outputs and failures are retained below.', '']
    for row in trial['cases']:
        path = ROOT / row['receipt']
        lines += [f"### {row['name']}", '', f"{row['input_tokens']} native input tokens; {row['status']}.", '',
                  '```json', path.with_name('output.json').read_text(), '```', '', row['review'], '',
                  f"[Receipt]({row['receipt']})", '']
    (ROOT / 'outputs.md').write_text('\n'.join(lines))
    (ROOT / 'review.json').write_text(json.dumps(dict(model_calls=14, retained_batch_calls=7,
        accepted=sum(r['status']=='accepted' for r in rows), discarded_batch='voice-at-end-review.json',
        qualification='Development refactor comparison; no held-out accuracy claim.', cases=rows),ensure_ascii=False,indent=2)+'\n')
    (ROOT / 'input-tokens.json').write_text(json.dumps([
        {k:r[k] for k in ('case','task','articles','input_tokens','previous_input_tokens','output_tokens','native_input_identical')}
        for r in rows],indent=2)+'\n')
    print('Seven final outputs match the prior trial; native token IDs and counts match exactly. Both batches retained.')


if __name__ == '__main__':
    main()
