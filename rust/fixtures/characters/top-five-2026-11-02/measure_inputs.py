"""Measure each complete native request before generation; no word/byte estimates."""
import json
import pathlib
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent
ROLES = dict(narratives='journalist',transfer='insider',vibe='influencer')

def post(path, data):
    req=urllib.request.Request('http://127.0.0.1:8081'+path,data=json.dumps(data).encode(),headers={'Content-Type':'application/json'})
    with urllib.request.urlopen(req,timeout=60) as response: return json.load(response)

if __name__ == '__main__':
    systems=json.loads((ROOT/'instructions.json').read_text())
    rows=[]
    for path in sorted((ROOT/'cases').glob('*.json')):
        fixture=json.loads(path.read_text());role=ROLES[fixture['task']]
        system=systems[role].strip().split('\n\n',1)[1]
        user=fixture['user_prompt'];packet=json.loads(user)
        messages=[dict(role='system',content=system),dict(role='user',content=user)]
        rendered=post('/apply-template',dict(model='Qwen3.5-4B-Q4_K_M',messages=messages,add_generation_prompt=True,chat_template_kwargs={'enable_thinking':False}))['prompt']
        assert system in rendered and user in rendered
        tokens=post('/tokenize',dict(content=rendered,add_special=True,parse_special=True))['tokens']
        reports=packet['fresh'] if isinstance(packet['fresh'],list) else packet['fresh']['reports']
        reserve=max(900,min(3,len(reports))*700) if fixture['task']=='narratives' else (1400 if fixture['task']=='transfer' else 600)
        row=dict(case=path.stem,task=fixture['task'],articles=len(reports),input_tokens=len(tokens),below_4096=len(tokens)<4096,output_reservation=reserve,total_reserved=len(tokens)+reserve)
        rows.append(row);print(row,flush=True)
    (ROOT/'input-tokens.json').write_text(json.dumps(rows,indent=2)+'\n')
    assert all(r['below_4096'] for r in rows)
