"""Prove current assembly matches the selected, already generated inputs byte for byte."""
import hashlib
import json
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parent
REPO = ROOT.parents[2]
ROLES = dict(narratives='journalist', vibe='influencer', rating='scout', transfer='insider', momentum='analyst', oracle='oracle')

if __name__ == '__main__':
    variants = json.loads((ROOT/'selected-variants.json').read_text())
    systems = json.loads((ROOT/'final-instructions.json').read_text())
    checks = []
    lines = ['# Complete current rendered requests', '', 'System and user messages from current production assembly. Each matches a selected generation receipt; task → context guide → target and evidence remains the user-message order.', '']
    for path in sorted((ROOT/'cases/candidate').glob('*.json')):
        fixture = json.loads(path.read_text())
        prompt = subprocess.check_output([str(REPO/'target/debug/examples/character_package'),str(path)],text=True)
        assert fixture['user_prompt'] == prompt
        arm = variants[fixture['task']]
        pattern = f'batch-*/{arm}/{path.stem}/01' if arm == 'candidate' else f'{arm}/{path.stem}/01'
        matches = list(ROOT.glob(pattern)); assert len(matches) == 1
        folder = matches[0]
        assert prompt == (folder/'prompt.json').read_text(), path
        assert fixture['parts'] == json.loads((folder/'fixture.json').read_text())['parts']
        role = ROLES[fixture['task']]
        source = (REPO/'src/plugins'/role/'instructions.txt').read_text().strip()
        job, system = source.split('\n\n',1)
        assert json.loads(prompt)['job'] == job
        assert system == systems[role]
        rendered = json.loads((folder/'prepared.json').read_text())['coverage']['rendered_prompt']
        assert '<|im_start|>system\n'+system+'<|im_end|>' in rendered, path
        assert prompt in rendered
        checks.append(dict(case=path.stem, prompt_version=fixture['prompt_version'], selected_arm=arm,
            user_sha256=hashlib.sha256(prompt.encode()).hexdigest(), system_sha256=hashlib.sha256(system.encode()).hexdigest(),
            receipt=str((folder/'result.json').relative_to(ROOT)), exact_input_match=True))
        lines += ['## '+path.stem,'','### System','',system,'','### User','','```json',json.dumps(json.loads(prompt),ensure_ascii=False,indent=2),'```','']
    (ROOT/'selection-check.json').write_text(json.dumps(checks,indent=2)+'\n')
    (ROOT/'prompt-preview.md').write_text('\n'.join(lines))
    print(f'{len(checks)} current system/user inputs exactly match selected generation receipts.')
