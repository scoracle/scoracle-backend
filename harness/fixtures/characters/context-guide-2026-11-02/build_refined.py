import pathlib,json,re,subprocess
root=pathlib.Path('fixtures/characters/context-guide-2026-11-02')
roles={'narratives':('journalist','NARRATIVES_PROMPT_VERSION'),'vibe':('influencer','VIBE_PROMPT_VERSION'),'transfer':('insider','PROMPT_VERSION'),'rating':('scout','RATING_PROMPT_VERSION'),'momentum':('analyst','MOMENTUM_PROMPT_VERSION'),'oracle':('oracle','ORACLE_PROMPT_VERSION')}
(root/'cases/refined').mkdir()
for p in sorted((root/'cases/control').glob('*.json')):
 fx=json.loads(p.read_text()); role,const=roles[fx['task']]
 fx['prompt_version']=re.search(r'pub const '+const+r': &str = "([^"]+)',pathlib.Path('src/plugins',role,'prompt.rs').read_text())[1]
 out=root/'cases/refined'/p.name;out.write_text(json.dumps(fx,indent=2)+'\n')
 fx['user_prompt']=subprocess.check_output(['target/release/examples/character_package',str(out)],text=True)
 old=json.loads(json.loads(p.read_text())['user_prompt']); new=json.loads(fx['user_prompt'])
 old['meta'].pop('target')
 for field in ['job','meta','fresh','tone']: assert old[field]==new[field], (p.name,field)
 assert list(new)==['job','context_guide','meta','fresh','tone']
 assert list(new['context_guide'])==['meta','fresh','tone']
 out.write_text(json.dumps(fx,ensure_ascii=False,indent=2)+'\n')
(root/'refined-instructions.json').write_text(json.dumps({role:pathlib.Path('src/plugins',role,'instructions.txt').read_text() for role,_ in roles.values()},indent=2)+'\n')
# Preview an actual Chelsea payload without additional model calls or database access.
p=root.parent/'job-target-fresh-2026-11-02/chelsea-preview.json'
fx=json.loads(p.read_text());fx['prompt_version']=json.loads((root/'cases/refined/narratives-player-37-NBA.json').read_text())['prompt_version']
p=root/'chelsea-preview.json';p.write_text(json.dumps(fx,indent=2)+'\n')
fx['user_prompt']=subprocess.check_output(['target/release/examples/character_package',str(p)],text=True)
p.write_text(json.dumps(fx,ensure_ascii=False,indent=2)+'\n')
(root/'chelsea-request.json').write_text(json.dumps(json.loads(fx['user_prompt']),ensure_ascii=False,indent=2)+'\n')
print('12 paired fixtures: job, structured metadata, fresh evidence and tone identical; guide and instruction contract changed.')
