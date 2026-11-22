"""Aggregate retained receipts, counting each experiment ledger once."""
import collections
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent
RUNS = ['baseline', 'candidate-screen', 'candidate-screen-admitted',
        'workload', 'final-screen', 'workload-final']


def summarize(rows):
    return dict(attempts=len(rows), calls=sum('output_tokens' in r for r in rows),
                input_tokens=sum(r.get('input_tokens', 0) for r in rows),
                output_tokens=sum(r.get('output_tokens', 0) for r in rows),
                summed_request_seconds=sum(r.get('wall_ms', 0) for r in rows) / 1000,
                statuses=dict(collections.Counter(r['status'] for r in rows)))


def main():
    all_rows, runs = [], {}
    for name in RUNS:
        root = ROOT / name
        rows = [json.loads(line) for line in (root / 'results.jsonl').read_text().splitlines()]
        assert len(rows) == {'baseline': 18, 'candidate-screen': 24,
                             'candidate-screen-admitted': 21, 'workload': 42,
                             'final-screen': 26, 'workload-final': 42}[name]
        all_rows.extend(rows)
        result = summarize(rows)
        result['by_task'] = {task: summarize([r for r in rows if r['task'] == task])
                             for task in sorted({r['task'] for r in rows})}
        responses = [json.loads(p.read_text()) for p in root.rglob('response.json')]
        result['uncached_input_tokens'] = sum(r.get('timings', {}).get('prompt_n', 0) for r in responses)
        result['server_prompt_seconds'] = sum(r.get('timings', {}).get('prompt_ms', 0) for r in responses) / 1000
        result['server_decode_seconds'] = sum(r.get('timings', {}).get('predicted_ms', 0) for r in responses) / 1000
        if name.startswith('workload'):
            run = json.loads((root / 'summary.json').read_text())
            result['elapsed_seconds'] = run['wall_seconds']
            result['max_queue_wait_seconds'] = run['max_queue_wait_ms'] / 1000
        runs[name] = result
    output = dict(runs=runs, total=summarize(all_rows),
                  caveat='Request durations overlap in four-slot runs. Server durations sum per-slot work; none is an electricity or full-night capacity measurement.')
    (ROOT / 'analysis.json').write_text(json.dumps(output, indent=2) + '\n')
    print(json.dumps(output, indent=2))


if __name__ == '__main__':
    assert summarize([dict(status='error', wall_ms=1),
                      dict(status='accepted', input_tokens=10, output_tokens=2, wall_ms=9)]) == dict(
        attempts=2, calls=1, input_tokens=10, output_tokens=2,
        summed_request_seconds=0.01, statuses={'error': 1, 'accepted': 1})
    main()
