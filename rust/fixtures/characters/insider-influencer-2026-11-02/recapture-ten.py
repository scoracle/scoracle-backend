"""Export current ten-article requests without model calls or database access."""
import copy
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[2]
VERSIONS = dict(narratives='n135-ten-articles', transfer='insider-source-v35-ten-articles',
                vibe='vibe-frame-v47-ten-articles', rating='s90-ten-articles')


def save(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n')


def export(original):
    fixture = copy.deepcopy(original)
    fixture['prompt_version'] = VERSIONS[fixture['task']]
    with tempfile.NamedTemporaryFile(mode='w', suffix='.json') as temp:
        json.dump(fixture, temp)
        temp.flush()
        request = json.loads(subprocess.check_output([
            str(REPO / 'target/debug/examples/character_package'), temp.name, '--request']))
    fixture['user_prompt'] = request['user_prompt']
    fixture['system'] = ''
    assert fixture['parts'] == original['parts']
    return fixture, request


def main():
    # Keep the earlier five-article requests and generations intact.
    cases = json.loads((ROOT / 'before-fixtures.json').read_text())
    prior = ROOT.parent / 'direct-instructions-2026-11-02'
    for path in sorted((prior / 'cases').glob('*.json')):
        fixture = json.loads(path.read_text())
        if fixture['task'] == 'narratives':
            cases[str(path.relative_to(REPO))] = fixture
    (ROOT / 'ten-cases').mkdir(exist_ok=True)
    requests = {}
    for index, (name, original) in enumerate(cases.items(), 1):
        fixture, request = export(original)
        save(ROOT / 'ten-cases' / f'{index:02}-{fixture["task"]}-{Path(name).name}', fixture)
        packet = json.loads(request['user_prompt'])
        content = packet['new_content']
        articles = content['reports'] if isinstance(content, dict) else content
        assert len(articles) <= 10, name
        requests[name] = request
        slug = {'transfer-team-37-FOOTBALL': 'insider-roma',
                'vibe-person-959-NFL': 'influencer-jerry-jones'}.get(fixture['name'])
        if slug:
            (ROOT / f'{slug}.payload.json').write_text(request['user_prompt'] + '\n')
            (ROOT / f'{slug}.system.txt').write_text(request['system'] + '\n')
            save(ROOT / f'{slug}.request.json', request)
    save(ROOT / 'ten-requests.json', requests)
    quality = {}
    for path in sorted((REPO / 'fixtures/quality').glob('*/*.json')):
        original = json.loads(path.read_text())
        if original.get('task') in VERSIONS:
            fixture, request = export(original)
            save(path, fixture)
            quality[str(path.relative_to(REPO))] = request
    save(ROOT / 'ten-quality-requests.json', quality)
    save(ROOT / 'ten-parity.json', dict(case_count=len(requests), quality_count=len(quality),
        source_parts_unchanged=True, article_cap=10, selection_changed_deliberately=True,
        parsers_unchanged=True, journalist_output_limit=3500,
        tested_generation_roles=['Journalist', 'Insider', 'Influencer']))
    print(f'Exported {len(requests)} cases and {len(quality)} quality requests.')


if __name__ == '__main__':
    main()
