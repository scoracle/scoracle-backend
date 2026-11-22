"""Recapture through production assembly and assert the refactor's allowed differences."""
import json
import pathlib
import re
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent
REPO = ROOT.parents[2]
ROLES = dict(narratives='journalist', transfer='insider', vibe='influencer',
             rating='scout', momentum='analyst', oracle='oracle')


def save(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n')


def main():
    before = json.loads((ROOT / 'before-requests.json').read_text())
    fixtures = json.loads((ROOT / 'before-fixtures.json').read_text())
    for path in sorted((ROOT.parent / 'top-five-2026-11-02/cases').glob('*.json')):
        fixtures[str(path.relative_to(REPO))] = json.loads(path.read_text())
    after = {}
    rows = []
    preview = ['# Complete current requests\n\nSystem rules precede each character JSON. No model calls for Scout, Analyst or Oracle.\n']
    (ROOT / 'cases').mkdir(exist_ok=True)
    for name, original in fixtures.items():
        fixture = dict(original)
        role = ROLES[fixture['task']]
        request_code = (REPO / 'src/plugins' / role / 'request.rs').read_text()
        fixture['prompt_version'] = re.search(r'pub const \w*PROMPT_VERSION: &str = "([^"]+)"', request_code)[1]
        with tempfile.NamedTemporaryFile(mode='w', suffix='.json') as temp:
            json.dump(fixture, temp)
            temp.flush()
            captured = json.loads(subprocess.check_output([
                str(REPO / 'target/debug/examples/character_package'), temp.name, '--request']))
        old = before[name]
        a, b = json.loads(old['user_prompt']), json.loads(captured['user_prompt'])
        assert a['meta'] == b['meta'] and a['fresh'] == b['fresh'], name
        assert {k: v for k, v in a['context_guide'].items() if k != 'tone'} == b['context_guide'], name
        assert list(b) == ['job', 'context_guide', 'meta', 'fresh'], name
        expected_system = old['system']
        if role == 'scout':
            expected_system = expected_system.replace('tone sets expression only. ', '')
        assert captured['system'] == expected_system, name
        assert {k: v for k, v in old.items() if k not in ('system', 'user_prompt')} == {
            k: v for k, v in captured.items() if k not in ('system', 'user_prompt')}, name
        assert a['job'] == b['job'], name
        if role in ('journalist', 'insider', 'influencer'):
            assert old == captured, name
        assert fixture['parts'] == original['parts']
        fixture['user_prompt'] = captured['user_prompt']
        fixture['system'] = ''
        target = REPO / name if name.startswith('fixtures/quality/') else ROOT / 'cases' / pathlib.Path(name).name
        save(target, fixture)
        after[name] = captured
        rows.append(dict(fixture=name, role=role, evidence_unchanged=True,
                         guide_meanings_unchanged=True, native_options_unchanged=True,
                         prompt_version=fixture['prompt_version']))
        if not name.startswith('fixtures/quality/'):
            preview += [f"## {fixture['name']}\n", '```text\n' + captured['system'] + '\n```\n',
                        '```json\n' + json.dumps(b, indent=2, ensure_ascii=False) + '\n```\n']
    decision = subprocess.check_output([str(REPO / 'target/debug/examples/classifier_replay'),
        str(REPO / 'fixtures/classifier/team-gender-2026-11-01/production-builder-cases.jsonl'), '--requests-only'])
    assert decision == (ROOT / 'decision-before.jsonl').read_bytes()
    (ROOT / 'decision-after.jsonl').write_bytes(decision)
    save(ROOT / 'after-requests.json', after)
    save(ROOT / 'parity.json', dict(character_requests=len(rows), quality_fixtures=52,
        decision_requests=len(decision.splitlines()), decision_byte_identical=True,
        allowed_changes=['voice extracted without changing job wording or order', 'trailing tone removed for remaining characters',
                         'Scout obsolete tone-field reference removed', 'prompt versions bumped'], cases=rows))
    (ROOT / 'prompt-preview.md').write_text('\n'.join(preview))
    print(f'{len(rows)} requests verified; decision payloads byte-identical; fixtures recaptured.')


if __name__ == '__main__':
    main()
