"""Audit source references and cost independently of the unchanged publication parser."""
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent


def references(findings, reports):
    result = []
    for finding in findings:
        quote = finding['evidence_quote']
        matches = [i for i, report in enumerate(reports) if quote and
                   (quote in report['headline'] or quote in report.get('publisher_excerpt', ''))]
        index = finding['report_index']
        report = reports[index] if 0 <= index < len(reports) else {}
        result.append(dict(report_index=index, matching_report_indices=matches,
                           exact_quote_at_cited_index=index in matches,
                           wrong_index=bool(matches) and index not in matches,
                           absent_quote=not matches,
                           counterparty_in_cited_report=any(m['name'] == finding['counterparty']
                                                           for m in report.get('co_mentions', []))))
    return result


def main():
    records = [json.loads(line) for line in (ROOT / 'trial/results.jsonl').read_text().splitlines()]
    assert len(records) == 24
    cases, totals = [], {}
    for case in json.loads((ROOT / 'case-provenance.json').read_text()):
        pair = dict(case=case['case'], subject=case['subject'], reports=case['reports'])
        requests = []
        for arm in ['control', 'indexed']:
            root = ROOT / 'trial' / arm / case['case'] / '01'
            row = json.loads((root / 'result.json').read_text())
            assert row['status'] != 'error', row
            raw = json.loads((root / 'output.json').read_text())
            fixture = json.loads((root / 'fixture.json').read_text())
            audit = references(raw['findings'], fixture['parts']['reports'])
            request = json.loads((root / 'request.json').read_text())
            request.pop('prompt')
            requests.append(request)
            pair[arm] = dict(status=row['status'], parser_detail=row.get('reading'), output=raw,
                             references=audit, input_tokens=row['input_tokens'], output_tokens=row['output_tokens'],
                             wall_seconds=row['wall_ms'] / 1000,
                             receipt=str((root / 'result.json').relative_to(ROOT)))
        assert requests[0] == requests[1], case['case']
        cases.append(pair)
    for arm in ['control', 'indexed']:
        data = [case[arm] for case in cases]
        refs = [r for d in data for r in d['references']]
        totals[arm] = dict(calls=len(data), accepted=sum(d['status'] == 'accepted' for d in data),
                           findings=len(refs), exact_quote_at_cited_index=sum(r['exact_quote_at_cited_index'] for r in refs),
                           wrong_index_findings=sum(r['wrong_index'] for r in refs),
                           wrong_index_cases=sum(any(r['wrong_index'] for r in d['references']) for d in data),
                           absent_quotes=sum(r['absent_quote'] for r in refs),
                           counterparties_in_cited_report=sum(r['counterparty_in_cited_report'] for r in refs),
                           empty_findings=sum(not d['references'] for d in data),
                           input_tokens=sum(d['input_tokens'] for d in data),
                           output_tokens=sum(d['output_tokens'] for d in data),
                           summed_request_seconds=sum(d['wall_seconds'] for d in data))
    output = dict(totals=totals, elapsed=json.loads((ROOT / 'trial/elapsed.json').read_text()), cases=cases,
                  caveat='Exact quote location and co-mention membership are mechanical properties, not proof of movement or faithful meaning. Empty findings have no reference opportunity.')
    (ROOT / 'analysis.json').write_text(json.dumps(output, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps(totals, indent=2))


if __name__ == '__main__':
    reports = [dict(headline='First source', co_mentions=[]),
               dict(headline='Second source', co_mentions=[dict(name='Harbor')])]
    result = references([dict(report_index=0, evidence_quote='Second source', counterparty='Harbor'),
                         dict(report_index=1, evidence_quote='invented', counterparty='Harbor')], reports)
    assert result[0]['wrong_index'] and not result[0]['counterparty_in_cited_report']
    assert result[1]['absent_quote'] and not result[1]['wrong_index']
    main()
