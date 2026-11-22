"""Reassemble frozen parts; keep historical captures and every evidence value intact."""
import json
import pathlib
import re
import subprocess

ROOT = pathlib.Path(__file__).resolve().parent
REPO = ROOT.parents[2]
BASE = ROOT.parent / 'context-guide-2026-11-02'
ROLES = dict(narratives='journalist', vibe='influencer', rating='scout', transfer='insider', momentum='analyst', oracle='oracle')

def capture(source, target):
    old = json.loads(source.read_text())
    fx = dict(old)
    role = ROLES[fx['task']]
    code = (REPO / 'src/plugins' / role / 'prompt.rs').read_text()
    fx['prompt_version'] = re.search(r'pub const \w*PROMPT_VERSION: &str = "([^"]+)', code)[1]
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(fx, ensure_ascii=False, indent=2) + '\n')
    fx['user_prompt'] = subprocess.check_output([str(REPO / 'target/debug/examples/character_package'), str(target)], text=True)
    before, after = (json.loads(f['user_prompt']) for f in (old, fx))
    for field in ['job', 'meta', 'fresh', 'tone']:
        assert before[field] == after[field], (source, field)
    # Serialization of the evidence is also byte-identical, including field order.
    assert old['user_prompt'].split('\n"meta":', 1)[1] == fx['user_prompt'].split('\n"meta":', 1)[1]
    assert old['parts'] == fx['parts']
    target.write_text(json.dumps(fx, ensure_ascii=False, indent=2) + '\n')

if __name__ == '__main__':
    for source in sorted((BASE / 'cases/refined').glob('*.json')):
        capture(source, ROOT / 'cases/candidate' / source.name)
    for task in ROLES:
        for source in (REPO / 'fixtures/quality' / task).glob('*.json'):
            capture(source, source)
    capture(BASE / 'chelsea-preview.json', ROOT / 'chelsea-preview.json')
    (ROOT / 'selection.json').write_text((BASE / 'selection.json').read_text())
    print('Evidence and parts unchanged; current cases regenerated through production assembly.')
