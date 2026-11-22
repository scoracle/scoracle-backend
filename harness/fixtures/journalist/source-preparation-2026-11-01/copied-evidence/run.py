import hashlib, importlib.util, json, sys
from pathlib import Path
from contract import validate, self_check

ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT.parent))
spec = importlib.util.spec_from_file_location('preparation_run', ROOT.parent / 'run.py')
stage = importlib.util.module_from_spec(spec); spec.loader.exec_module(stage)
stage.ROOT = ROOT; stage.validate = validate

def admit(file, mode, record):
    rows = stage.admit(file, mode)
    (ROOT / record).write_text(json.dumps(rows, indent=2) + '\n')
    assert all(r['fits'] for r in rows)

def automatic(cohort):
    admit(f'{cohort}-sources.jsonl', 'prepare', f'admission-{cohort}-preparation.json')
    stage.run(f'{cohort}-sources.jsonl', 'prepare', f'{cohort}-preparation')
    checks = []
    for c in stage.cases(f'{cohort}-sources.jsonl'):
        path = ROOT / f'{cohort}-preparation' / c['name'] / 'output.json'
        try:
            validate(json.loads(path.read_text()), json.loads(c['user_prompt'])['headline'])
            checks.append({'name': c['name'], 'all_text_fields_copied': True})
        except (AssertionError, KeyError, TypeError, FileNotFoundError, json.JSONDecodeError) as error:
            checks.append({'name': c['name'], 'all_text_fields_copied': False, 'error': repr(error)})
    (ROOT / f'{cohort}-copy-check.json').write_text(json.dumps(checks, indent=2) + '\n')
    assert all(c['all_text_fields_copied'] for c in checks), 'No writer calls after a copy failure'
    assert len(stage.attach(cohort)) == len(stage.cases(f'{cohort}-control.jsonl'))
    admit(f'{cohort}-automatic.jsonl', 'write', f'admission-{cohort}-automatic.json')
    stage.run(f'{cohort}-automatic.jsonl', 'write', f'{cohort}-automatic')

self_check()
for file, digest in json.loads((ROOT / 'manifest.json').read_text())['sha256'].items():
    assert hashlib.sha256((ROOT / file).read_bytes()).hexdigest() == digest
before = stage.base.request('/v1/models'); assert before['data'][0]['id'] == stage.base.MODEL
mode = sys.argv[1]
if mode == 'development':
    (ROOT / 'model-before.json').write_text(json.dumps(before, indent=2) + '\n')
    admit('development-verbatim.jsonl', 'write', 'admission-development-verbatim.json')
    stage.run('development-verbatim.jsonl', 'write', 'development-verbatim')
    automatic('development')
elif mode == 'validation':
    selection = json.loads((ROOT / 'selection.json').read_text())
    assert selection['qualifies_for_followup']
    arm = selection['selected']; assert arm in ['verbatim', 'automatic']
    stage.run(f'development-{arm}.jsonl', 'write', f'repeat-{arm}')
    for candidate in ['control', 'verbatim']:
        admit(f'validation-{candidate}.jsonl', 'write', f'admission-validation-{candidate}.json')
        stage.run(f'validation-{candidate}.jsonl', 'write', f'validation-{candidate}')
    if arm == 'automatic': automatic('validation')
else: raise ValueError(mode)
after = stage.base.request('/v1/models')
(ROOT / f'model-after-{mode}.json').write_text(json.dumps(after, indent=2) + '\n')
for obj in [before, after]:
    for model in obj['data']: model.pop('created', None)
assert before == after
