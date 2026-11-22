"""Freeze equal-evidence arms through the before/after Rust assemblers; no database access."""
import datetime
import html
import json
import pathlib
import re
import subprocess

ROOT = pathlib.Path(__file__).resolve().parent
REPO = ROOT.parents[2]
EXPORTERS = {'control': '/private/tmp/scoracle-insider-unindexed-package',
             'indexed': str(REPO / 'target/release/examples/character_package')}
VERSIONS = {'control': 'insider-source-v21-perspective',
            'indexed': 'insider-source-v22-report-indices'}


def main():
    rows = [json.loads(line) for line in (ROOT / 'final-frozen-reporting.jsonl').read_text().splitlines()]
    assert len(rows) == 12
    ledger = []
    for row in rows:
        key = f"{row['entity_type']}-{row['entity_id']}-{row['sport']}"
        path = next(p for task in ['narratives', 'rating']
                    for folder in ['identity-capture', 'replacement-capture']
                    if (p := ROOT / folder / f'{task}-{key}.json').exists())
        identity = json.loads(path.read_text())['parts']
        subject = identity['subject']
        reports, held, seen = [], [], set()
        for source in row['reports']:
            resolved = source['classifier_world']['target_identity_resolved']
            dedup = (source['source'].strip().lower(), html.unescape(source['context']).strip())
            reason = 'unresolved_target_identity' if not resolved else 'known_copy' if dedup in seen else None
            if reason:
                held.append(dict(article_id=source['article_id'], reason=reason))
                continue
            seen.add(dedup)
            assert source['published_at_epoch'] is not None
            reports.append(dict(
                classifier_world=dict(source['classifier_world'], article_id=source['article_id']),
                publisher=source['source'], headline=source['headline'], publisher_excerpt='',
                published_at=datetime.datetime.fromtimestamp(source['published_at_epoch'], datetime.timezone.utc).isoformat().replace('+00:00', 'Z'),
                co_mentions=[dict(name=c['name'], entity_type=c['entity_type']) for c in source['identity_candidates']
                             if (c['entity_type'], c['entity_id']) != (subject['entity_type'], subject['entity_id'])]))
        assert reports, key
        parts = dict(subject=subject, database_record=identity.get('database_record'), reports=reports)
        prompts = {}
        for arm, exporter in EXPORTERS.items():
            directory = ROOT / 'cases' / arm
            directory.mkdir(parents=True, exist_ok=True)
            target = directory / f'{key}.json'
            fixture = dict(name=key, task='transfer', sport=row['sport'], parts=parts,
                           system='', user_prompt='', prompt_version=VERSIONS[arm], temperature=0.0,
                           expect={}, review=['Check source-reference correctness independently of prose meaning, reported status and movement ownership.'])
            target.write_text(json.dumps(fixture, ensure_ascii=False, indent=2) + '\n')
            prompts[arm] = subprocess.check_output([exporter, str(target)], text=True)
            fixture['user_prompt'] = prompts[arm]
            target.write_text(json.dumps(fixture, ensure_ascii=False, indent=2) + '\n')
        # Byte equality after removing the one new field rules out reordered evidence or instructions.
        assert re.sub(r',"report_index":\d+', '', prompts['indexed']) == prompts['control'], key
        assert [r['report_index'] for r in json.loads(prompts['indexed'])['fresh']['reports']] == list(range(len(reports)))
        ledger.append(dict(case=key, subject=subject['name'], edition_date=row['edition_date'],
                           reports=len(reports), held=held,
                           article_ids=[r['classifier_world']['article_id'] for r in reports]))
    (ROOT / 'case-provenance.json').write_text(json.dumps(ledger, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps(ledger, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
