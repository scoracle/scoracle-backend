"""Count every retained generation; reference checks do not score factual meaning."""
import collections
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent
RUNS = ['trial', 'ordered', 'worked', 'worked-ordered', 'eligible', 'sampled', 'sampled-rest', 'metadata']


def references(output, reports):
    results = []
    for finding in output['findings']:
        index, quote = finding['report_index'], finding['evidence_quote']
        report = reports[index] if 0 <= index < len(reports) else {}
        results.append(dict(exact_quote=bool(quote) and any(quote in report.get(k, '') for k in ['headline', 'publisher_excerpt']),
            supplied_counterparty=any(m['name'] == finding['counterparty'] for m in report.get('co_mentions', []))))
    return results


def main():
    totals = collections.defaultdict(lambda: dict(calls=0, input_tokens=0, output_tokens=0,
        measured_wall_ms=0, missing_wall_measurements=0, accepted=0, findings=0, exact_quotes=0))
    records = []
    for run in RUNS:
        for response_path in sorted((ROOT / run).glob('*/response.json')):
            path = response_path.parent
            response = json.loads(response_path.read_text())
            assert response['stop'] and response['stop_type'] == 'eos' and not response['truncated']
            row = json.loads((path / 'result.json').read_text()) if (path / 'result.json').exists() else {}
            arm = row.get('arm', run)
            output = json.loads((path / 'output.json').read_text())
            fixture_path = path / 'fixture.json'
            if not fixture_path.exists():
                fixture_path = ROOT / 'worked' / path.name / 'fixture.json'
            fixture = json.loads(fixture_path.read_text())
            refs = references(output, fixture['parts']['reports'])
            t = totals[arm]
            t['calls'] += 1
            t['input_tokens'] += response['tokens_evaluated']
            t['output_tokens'] += response['tokens_predicted']
            t['measured_wall_ms'] += row.get('wall_ms') or 0
            t['missing_wall_measurements'] += row.get('wall_ms') is None
            t['accepted'] += row.get('status') == 'accepted'
            t['findings'] += len(refs)
            t['exact_quotes'] += sum(r['exact_quote'] for r in refs)
            records.append(dict(run=run, case=path.name, arm=arm, output=output, references=refs,
                status=row.get('status', 'bookkeeping_failed_after_generation'),
                parser_detail=row.get('parser_detail'), receipt=str(response_path.relative_to(ROOT))))
    worked = sorted((ROOT / 'worked').glob('*/output.json'))
    eligible = sorted((ROOT / 'eligible').glob('*/output.json'))
    assert len(worked) == len(eligible) == 12
    assert all(json.loads(a.read_text())['body'] == json.loads(b.read_text())['body'] for a,b in zip(worked, eligible))
    result = dict(totals=dict(totals), calls=sum(t['calls'] for t in totals.values()),
        input_tokens=sum(t['input_tokens'] for t in totals.values()),
        output_tokens=sum(t['output_tokens'] for t in totals.values()),
        measured_wall_seconds=sum(t['measured_wall_ms'] for t in totals.values())/1000,
        missing_wall_measurements=sum(t['missing_wall_measurements'] for t in totals.values()),
        eligibility_preserves_all_12_bodies=True, records=records)
    (ROOT / 'analysis.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps({k:v for k,v in result.items() if k != 'records'}, indent=2))


if __name__ == '__main__':
    assert references({'findings':[{'report_index':0,'evidence_quote':'Elsewhere','counterparty':'Harbor'}]},
        [{'headline':'Original','co_mentions':[]}]) == [{'exact_quote':False,'supplied_counterparty':False}]
    main()
