"""Build review cases from frozen real evidence; current Rust assemblers own wire output."""
import datetime as dt
import html
import json
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parent
REPO = ROOT.parents[2]
EXPORT = REPO / 'target/release/examples/character_package'
REVIEW = [
    'Does the card fulfill its character purpose and help explain the entity?',
    'Is the central reading supported, with subject, speaker, measurements and reported status preserved?',
    'Record material errors separately from awkward wording; parser acceptance is not truth.',
]

def save(root, name, fixture):
    path = root / (name + '.json')
    path.write_text(json.dumps(fixture, ensure_ascii=False, indent=2) + '\n')
    fixture['user_prompt'] = subprocess.check_output([str(EXPORT), str(path)], text=True)
    path.write_text(json.dumps(fixture, ensure_ascii=False, indent=2) + '\n')

def current(task):
    return json.loads(next((REPO / 'fixtures/quality' / task).glob('*.json')).read_text())['prompt_version']

def fixture(task, name, parts, sport):
    return dict(name=name, task=task, sport=sport, prompt_version=current(task), system='',
                user_prompt='', parts=parts, temperature=0.0, expect={}, review=REVIEW)

def build():
    cases = ROOT / 'cases'
    cases.mkdir(exist_ok=True)
    # The same bounded synthetic/edge-case screen used by the checkpoint baseline.
    for p in (ROOT / 'baseline-cases').glob('*.json'):
        old = json.loads(p.read_text())
        matches = [json.loads(q.read_text()) for q in (REPO / 'fixtures/quality' / old['task']).glob('*.json')]
        new = next(x for x in matches if x['name'] == old['name'])
        save(cases, 'screen-' + p.stem, new)
    for p in (REPO / 'fixtures/quality/momentum').glob('synthetic-dated-*.json'):
        save(cases, 'screen-momentum-'+p.stem, json.loads(p.read_text()))
    # The six repeatedly-used development packets: remove experimental groups/notes.
    for line in (REPO / 'fixtures/journalist/source-preparation-2026-11-01/development-control.jsonl').read_text().splitlines():
        old = json.loads(line)
        name = 'regression-' + old['name']
        parts = old['parts']
        groups = json.loads(old['user_prompt'])['fresh']
        served = {int(r['report_key'].split('_')[1])-1 for g in groups for r in g['reports']}
        parts['reports'] = [r for i,r in enumerate(parts['reports']) if i in served]
        save(cases, name, fixture('narratives', name, parts, parts['subject']['sport']))
    ledger = []
    for line in (ROOT / 'frozen-reporting.jsonl').read_text().splitlines():
        row = json.loads(line)
        task = {'transfers':'transfer'}.get(row['stage'], row['stage'])
        key = f"{row['entity_type']}-{row['entity_id']}-{row['sport']}"
        identity_path = next(p for d in ['real-capture', 'sparse-capture'] if (p := ROOT/d/('narratives-'+key+'.json')).exists())
        identity = json.loads(identity_path.read_text())['parts']
        subject, record = identity['subject'], identity.get('database_record')
        sources, mentions, seen, redundant, held = [], [], set(), [], []
        for source in row['reports'] or []:
            candidates = source.pop('identity_candidates')
            resolved = any(c['entity_type'] == subject['entity_type'] and c['entity_id'] == subject['entity_id'] for c in candidates)
            source['classifier_world']['target_identity_resolved'] = resolved
            if not resolved:
                held.append(dict(article_id=source['article_id'],classification_id=source['classification_id'],in_day=True,reason='unresolved_target_identity'))
                continue
            evidence_key = (source['source'].strip().lower(), html.unescape(source['context']).strip())
            if evidence_key in seen:
                redundant.append(source['classification_id'])
                continue
            seen.add(evidence_key)
            sources.append(source)
            mentions.append([dict(name=c['name'],entity_type=c['entity_type']) for c in candidates
                             if (c['entity_type'],c['entity_id']) != (subject['entity_type'],subject['entity_id'])])
        if not sources:
            continue
        if task == 'narratives':
            parts = dict(subject=subject, database_record=record, reports=[dict(id=s['article_id'], title=s['headline'], context=s['context'], source=s['source'], published_at_epoch=s['published_at_epoch'],classifier_world=s['classifier_world']) for s in sources])
        elif task == 'vibe':
            # The frozen assignment dates the package, not a newly collected day.
            from zoneinfo import ZoneInfo
            end = dt.datetime.fromisoformat(row['edition_date']).replace(tzinfo=ZoneInfo('America/Detroit'))
            start = end-dt.timedelta(days=1)
            parts = dict(subject=subject, database_record=record, day=dict(start=int(start.timestamp()), end=int(end.timestamp()), cutoff=int(dt.datetime.fromisoformat(row['frozen_at']).timestamp())), sources=sources, excluded=held)
        else:
            reports=[]
            for s, candidates in zip(sources, mentions, strict=True):
                world=dict(s['classifier_world'],article_id=s['article_id'])
                reports.append(dict(classifier_world=world,publisher=s['source'],published_at=dt.datetime.fromtimestamp(s['published_at_epoch'],dt.timezone.utc).isoformat().replace('+00:00','Z'),headline=s['headline'],publisher_excerpt='',co_mentions=candidates))
            parts=dict(subject=subject,database_record=record,reports=reports)
        name='real-'+task+'-'+key
        save(cases,name,fixture(task,name,parts,subject['sport']))
        ledger.append(dict(case=name,edition_date=row['edition_date'],selected=len(sources),redundant_spectrum_ids=redundant,held=held,article_ids=[s['article_id'] for s in sources]))
    # Statistics and dated peer inputs come from the live production loaders, read-only.
    for p in (ROOT/'real-capture').glob('*.json'):
        f=json.loads(p.read_text())
        if f['task'] in ('rating','momentum','oracle'):
            f['prompt_version'] = current(f['task'])
            save(cases,'real-'+p.stem,f)
    (ROOT/'case-provenance.json').write_text(json.dumps(ledger,indent=2)+'\n')

if __name__ == '__main__':
    build()
