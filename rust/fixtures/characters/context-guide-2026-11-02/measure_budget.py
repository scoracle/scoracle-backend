import json,pathlib,urllib.request
root=pathlib.Path('fixtures/characters/context-guide-2026-11-02')
old=root.parent/'job-target-fresh-2026-11-02'
def post(route,data):
 req=urllib.request.Request('http://127.0.0.1:8081'+route,data=json.dumps(data).encode(),headers={'Content-Type':'application/json'})
 with urllib.request.urlopen(req,timeout=60) as r:return json.load(r)
rows=[]
for arm,source,systems in [('control',old,root/'baseline-instructions.json'),('candidate',root,root/'candidate-instructions.json')]:
 fx=json.loads((source/('initial-chelsea-preview.json' if arm=='candidate' else 'chelsea-preview.json')).read_text());system=json.loads(systems.read_text())['journalist'].strip()
 request={'model':'Qwen3.5-4B-Q4_K_M','messages':[{'role':'system','content':system},{'role':'user','content':fx['user_prompt']}], 'add_generation_prompt':True,'chat_template_kwargs':{'enable_thinking':False}}
 rendered=post('/apply-template',request)['prompt'];assert system in rendered and fx['user_prompt'] in rendered
 tokens=post('/tokenize',dict(content=rendered,add_special=True,parse_special=True))['tokens']
 rows.append(dict(arm=arm,reports=len(fx['parts']['reports']),input_tokens=len(tokens),output_reservation=2100,context_limit=7900,fits=len(tokens)+2100<=7900,model_calls=0))
(root/'budget-check.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(rows,indent=2))
