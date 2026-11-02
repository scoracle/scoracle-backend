"""Freeze before requests, or recapture identical requests through production after refactoring."""
import hashlib
import json
import pathlib
import re
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent
REPO = ROOT.parents[2]
PREVIOUS = ROOT.parent / 'shared-prompt-2026-11-02'
ROLES = dict(narratives='journalist', transfer='insider', vibe='influencer',
             rating='scout', momentum='analyst', oracle='oracle')


def save(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n')


def export(fixture):
    with tempfile.NamedTemporaryFile(mode='w', suffix='.json') as temp:
        json.dump(fixture, temp)
        temp.flush()
        return json.loads(subprocess.check_output([
            str(REPO / 'target/debug/examples/character_package'), temp.name, '--request']))


def decision():
    return subprocess.check_output([str(REPO / 'target/debug/examples/classifier_replay'),
        str(REPO / 'fixtures/classifier/team-gender-2026-11-01/production-builder-cases.jsonl'), '--requests-only'])


def main():
    if sys.argv[1:] == ['--before']:
        assert not (ROOT / 'before-requests.json').exists(), 'preserve the frozen baseline'
        paths = [p for task in ROLES for p in sorted((REPO / 'fixtures/quality' / task).glob('*.json'))]
        paths += sorted((PREVIOUS / 'cases').glob('*.json'))
        fixtures = {str(p.relative_to(REPO)): json.loads(p.read_text()) for p in paths}
        save(ROOT / 'before-fixtures.json', fixtures)
        save(ROOT / 'before-requests.json', {p: export(f) for p, f in fixtures.items()})
        (ROOT / 'decision-before.jsonl').write_bytes(decision())
        (ROOT / 'planning-document-sha256.txt').write_text(hashlib.sha256(
            (REPO.parent / 'docs/planning_docs/PLAN-production-recovery-2026-10-22.md').read_bytes()).hexdigest() + '\n')
        print(f'Frozen {len(paths)} full requests and 24 decision requests without model calls.')
        return
    assert not sys.argv[1:]
    before = json.loads((ROOT / 'before-requests.json').read_text())
    fixtures = json.loads((ROOT / 'before-fixtures.json').read_text())
    after = {}
    (ROOT / 'cases').mkdir(exist_ok=True)
    preview = ['# Current development requests\n\nComplete system rules and JSON, rendered through production assembly.\n']
    for name, original in fixtures.items():
        fixture = dict(original)
        role = ROLES[fixture['task']]
        code = (REPO / 'src/plugins' / role / 'request.rs').read_text()
        fixture['prompt_version'] = re.search(r'pub const \w*PROMPT_VERSION: &str = "([^"]+)"', code)[1]
        captured = export(fixture)
        assert captured == before[name], f'Request changed: {name}'
        assert fixture['parts'] == original['parts']
        fixture['user_prompt'] = captured['user_prompt']
        fixture['system'] = ''
        quality = name.startswith('fixtures/quality/')
        target = REPO / name if quality else ROOT / 'cases' / pathlib.Path(name).name
        save(target, fixture)
        after[name] = captured
        if not quality:
            preview += [f"## {fixture['name']}\n", '```text\n' + captured['system'] + '\n```\n',
                        '```json\n' + json.dumps(json.loads(captured['user_prompt']), indent=2, ensure_ascii=False) + '\n```\n']
    native = decision()
    assert native == (ROOT / 'decision-before.jsonl').read_bytes()
    (ROOT / 'decision-after.jsonl').write_bytes(native)
    save(ROOT / 'after-requests.json', after)
    save(ROOT / 'parity.json', dict(character_requests=len(after), quality_fixtures=52,
        decision_requests=len(native.splitlines()), all_requests_byte_identical=True,
        system_rules_schema_options_and_should_call_unchanged=True, candidate_parts_unchanged=True))
    (ROOT / 'prompt-preview.md').write_text('\n'.join(preview))
    print(f'{len(after)} complete requests and {len(native.splitlines())} decision requests unchanged; fixtures regenerated.')


if __name__ == '__main__':
    main()
