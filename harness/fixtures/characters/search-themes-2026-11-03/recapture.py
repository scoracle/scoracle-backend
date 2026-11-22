"""Regenerate current fixtures, asserting that only evidence-guide prose changes."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[2]
VERSIONS = {
    'narratives': 'n136-search-themes',
    'transfer': 'insider-source-v37-search-themes',
    'vibe': 'vibe-frame-v49-search-themes',
}


def export(binary, fixture):
    with tempfile.NamedTemporaryFile(mode='w', suffix='.json') as temp:
        json.dump(fixture, temp)
        temp.flush()
        return json.loads(subprocess.check_output([binary, temp.name, '--request']))


def verify(before, after):
    old, new = json.loads(before['user_prompt']), json.loads(after['user_prompt'])
    assert old['instructions']['new_content'][0] != new['instructions']['new_content'][0]
    old['instructions']['new_content'][0] = new['instructions']['new_content'][0]
    assert old == new, 'Target, evidence, task or field definitions changed'
    assert {k: v for k, v in before.items() if k != 'user_prompt'} == {
        k: v for k, v in after.items() if k != 'user_prompt'
    }, 'System rules, schema or generation settings changed'


def main():
    baseline, candidate = sys.argv[1:]
    checks = []
    for path in sorted((REPO / 'fixtures/quality').glob('*/*.json')):
        fixture = json.loads(path.read_text())
        task = fixture['task']
        if task not in VERSIONS:
            assert export(baseline, fixture) == export(candidate, fixture)
            checks.append({'fixture': str(path.relative_to(REPO)), 'unchanged': True})
            continue
        assert fixture.get('parts') is not None
        before = export(baseline, fixture)
        fixture['prompt_version'] = VERSIONS[task]
        after = export(candidate, fixture)
        verify(before, after)
        fixture['user_prompt'] = after['user_prompt']
        path.write_text(json.dumps(fixture, indent=2, ensure_ascii=False) + '\n')
        checks.append({'fixture': str(path.relative_to(REPO)), 'guide_only': True})
    (ROOT / 'fixture-parity.json').write_text(json.dumps(checks, indent=2) + '\n')
    print(json.dumps({'updated': sum(c.get('guide_only', False) for c in checks),
                      'unrelated_unchanged': sum(c.get('unchanged', False) for c in checks)}))


if __name__ == '__main__':
    main()
