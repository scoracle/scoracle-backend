"""Prompt-only preparation comparison; retain the worked writer and all wire inputs."""
import hashlib, json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
BASE = ROOT.parent / 'worked-writer'

def build():
    unchanged = ['writer.txt', 'development-control.jsonl', 'validation-control.jsonl',
                 'development-sources.jsonl', 'validation-sources.jsonl']
    for name in unchanged:
        (ROOT / name).write_bytes((BASE / name).read_bytes())
        assert (ROOT / name).read_bytes() == (BASE / name).read_bytes()
    original = (BASE / 'preparer.txt').read_text()
    marker = 'Example: A Rowan Moss trade might make Harbor contenders\n'
    assert original.count(marker) == 1
    extra = (ROOT / 'instruction-addition.txt').read_text().strip() + '\n\n'
    (ROOT / 'preparer.txt').write_text(original.replace(marker, extra + marker).strip())
    for cohort, count in [('development', 29), ('validation', 11)]:
        sources = list(map(json.loads, (ROOT / f'{cohort}-sources.jsonl').read_text().splitlines()))
        assert len(sources) == count == len({c['name'] for c in sources})
        for c in sources:
            packet = json.loads(c['user_prompt'])
            assert set(packet) == {'article_id', 'headline', 'publisher', 'published_at', 'evidence_scope'}
            assert packet['article_id'] == int(c['name']) == c['provenance']['article_id']
            assert json.loads(c['schema_raw'])['properties']['statements']['maxItems'] == 3
    # The writer/reference screen is reusable because its exact inputs and settings are unchanged.
    assert json.loads((BASE / 'screen-selection.json').read_text())['qualifies_for_automatic']
    (ROOT / 'screen-selection.json').write_text(json.dumps({
        'qualifies_for_automatic': True,
        'reused_screen': '../worked-writer/screen-selection.json',
        'reason': 'Identical writer, source schema, source inputs and writer settings; only preparer instructions change.'
    }, indent=2) + '\n')
    runner = (BASE / 'run_automatic.py').read_text()
    marker = "if mode=='development':\n"
    assert runner.count(marker) == 1
    runner = runner.replace(marker, marker + " (ROOT/'model-before.json').write_text(json.dumps(before,indent=2)+'\\n')\n")
    (ROOT / 'run.py').write_text(runner)
    files = unchanged + ['preparer.txt', 'instruction-addition.txt', 'protocol.txt', 'run.py', 'build_inputs.py', 'screen-selection.json']
    (ROOT / 'manifest.json').write_text(json.dumps({
        'baseline': '../worked-writer',
        'sha256': {n: hashlib.sha256((ROOT / n).read_bytes()).hexdigest() for n in files},
        'baseline_sha256': {n: hashlib.sha256((BASE / n).read_bytes()).hexdigest() for n in unchanged}
    }, indent=2) + '\n')
    print('Frozen prompt-only comparison: 29 development sources, six writer packets; 11 validation sources remain unused.')

if __name__ == '__main__':
    build()
