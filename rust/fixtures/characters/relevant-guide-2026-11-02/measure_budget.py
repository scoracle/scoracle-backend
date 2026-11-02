"""Zero-generation complete native input measurement for the unchanged 24-report preview."""
import json
import pathlib
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent
BASE = ROOT.parent / 'context-guide-2026-11-02'

def post(route, data):
    request = urllib.request.Request('http://127.0.0.1:8081'+route, data=json.dumps(data).encode(), headers={'Content-Type':'application/json'})
    with urllib.request.urlopen(request, timeout=60) as response:
        return json.load(response)

if __name__ == '__main__':
    rows = []
    evidence = []
    for arm, folder, instructions in [('baseline', BASE, BASE/'refined-instructions.json'), ('final', ROOT, ROOT/'final-instructions.json')]:
        fixture = json.loads((folder/'chelsea-preview.json').read_text())
        system = json.loads(instructions.read_text())['journalist'].strip()
        evidence.append(fixture['user_prompt'].split('\n"meta":',1)[1])
        rendered = post('/apply-template', dict(model='Qwen3.5-4B-Q4_K_M', messages=[dict(role='system',content=system),dict(role='user',content=fixture['user_prompt'])],add_generation_prompt=True,chat_template_kwargs={'enable_thinking':False}))['prompt']
        assert system in rendered and fixture['user_prompt'] in rendered
        tokens = post('/tokenize', dict(content=rendered, add_special=True, parse_special=True))['tokens']
        rows.append(dict(arm=arm, reports=len(fixture['parts']['reports']), input_tokens=len(tokens),output_reservation=2100, fits_7900=len(tokens)+2100<=7900, fits_10240=len(tokens)+2100<=10240, model_generations=0))
    assert evidence[0] == evidence[1]
    (ROOT/'budget-check.json').write_text(json.dumps(rows,indent=2)+'\n')
    print(json.dumps(rows,indent=2))
