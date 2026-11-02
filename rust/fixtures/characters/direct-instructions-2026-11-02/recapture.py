"""Recapture the three plain briefs through production; preserve evidence and unrelated requests."""
import copy
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[2]


def save(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n')


def export(fixture):
    with tempfile.NamedTemporaryFile(mode='w', suffix='.json') as temp:
        json.dump(fixture, temp)
        temp.flush()
        return json.loads(subprocess.check_output([
            str(REPO / 'target/debug/examples/character_package'), temp.name, '--request']))


def main():
    before = json.loads((ROOT / 'before-requests.json').read_text())
    fixtures = json.loads((ROOT / 'before-fixtures.json').read_text())
    after = {}
    changed = 0
    versions = dict(narratives='n134-recap-hook-last', transfer='insider-source-v33-plain-brief', vibe='vibe-frame-v45-plain-brief')
    previews = ['# Journalist requests\n\nExact system rules and user JSON from production assembly.\n']
    (ROOT / 'cases').mkdir(exist_ok=True)
    for name, original in fixtures.items():
        fixture = copy.deepcopy(original)
        if fixture['task'] in versions:
            fixture['prompt_version'] = versions[fixture['task']]
        current = export(fixture)
        old = before[name]
        if fixture['task'] not in versions:
            assert current == old, name
        else:
            changed += 1
            a, b = json.loads(old['user_prompt']), json.loads(current['user_prompt'])
            assert list(b) == ['instructions', 'target', 'new_content']
            assert b['target'] == a['target'], name
            assert b['instructions']['target'] == a['instructions']['target'], name
            if fixture['task'] == 'narratives':
                for i, article in enumerate(a['new_content'], 1):
                    assert article.pop('report_key') == f'report_{i}'
                    article['id'] = i
                count = len(b['new_content'])
                item = copy.deepcopy(old['format_schema']['properties']['narratives']['items'])
                hook = item['properties'].pop('headline')
                hook['type'] = ['string', 'null']
                item['required'] = ['body', 'report_ids']
                ids = item['properties'].pop('report_keys')
                ids['items'] = {'type': 'integer', 'enum': list(range(1, count + 1))}
                item['properties']['report_ids'] = ids
                schema = dict(type='object', additionalProperties=False, required=['paragraphs', 'headline'],
                    properties=dict(paragraphs=dict(type='array', maxItems=count, items=item), headline=hook))
                assert current['format_schema'] == schema, name
                assert json.loads(current['format_schema_raw']) == schema, name
                assert '"properties":{"paragraphs":' in current['format_schema_raw']
                assert current['num_predict'] == max(900, count * 700)
                allowed = {'system', 'user_prompt', 'format_schema', 'format_schema_raw', 'num_predict'}
            else:
                # Only the plain-language task changes for Insider and Influencer.
                a['instructions']['task'] = b['instructions']['task']
                assert a == b, name
                allowed = {'user_prompt'}
            assert b['new_content'] == a['new_content'], name
            for key in old.keys() - allowed:
                assert current[key] == old[key], (name, key)
            fixture['user_prompt'] = current['user_prompt']
            fixture['system'] = ''
            assert fixture['parts'] == original['parts']
            if name.startswith('fixtures/quality/'):
                save(REPO / name, fixture)
            else:
                save(ROOT / 'cases' / Path(name).name, fixture)
                previews += [f"## {fixture['name']}\n", '```text\n' + current['system'] + '\n```\n',
                             '```json\n' + current['user_prompt'] + '\n```\n']
                if Path(name).name in ('chelsea-live.fixture.json', 'dak-prescott.fixture.json'):
                    slug = Path(name).name.removesuffix('.fixture.json')
                    (ROOT / (slug + '.payload.json')).write_text(current['user_prompt'] + '\n')
                    (ROOT / (slug + '.system.txt')).write_text(current['system'] + '\n')
        after[name] = current
    save(ROOT / 'after-requests.json', after)
    save(ROOT / 'parity.json', dict(total=len(after), briefs_changed=changed,
        other_requests_byte_identical=len(after)-changed, source_wording_and_selection_unchanged=True,
        target_and_target_guide_unchanged=True, source_parts_unchanged=True,
        schema_change='Journalist: paragraphs with integer report_ids, then one headline; paragraph count bounded by selected articles, body and hook length bounds unchanged'))
    (ROOT / 'prompt-preview.md').write_text('\n'.join(previews))
    print(f'{changed} brief requests regenerated; {len(after)-changed} other requests unchanged.')


if __name__ == '__main__':
    main()
