from pathlib import Path
import json,re,urllib.request,time
root=Path('/Users/scotty/scoracle/scoracle-backend/rust');trial=root/'fixtures/influencer/tuning-2026-11-13'
code=(root/'src/plugins/influencer/request.rs').read_text()
context=json.loads(re.search(r'pub const TARGET_CONTEXT: &str = ("[^"]*");',code)[1])
schema=json.loads(re.search(r'pub const SCHEMA_RAW: &str = r#"(.*?)"#;',code)[1])
version='vibe-frame-v59-target-context'
assert (root/'src/plugins/influencer/character_instructions.rs').read_text()==(trial/'emotional-claims-instructions.rs').read_text()
system=json.loads(re.search(r'pub const SYSTEM: &str = ("[^"]*");',(trial/'emotional-claims-instructions.rs').read_text())[1])
out=trial/'target-context';out.mkdir();cases=trial/'target-context-cases';cases.mkdir()
def save(p,v):p.write_text(json.dumps(v,indent=2,ensure_ascii=False)+'\n')
def call(path,data=None,timeout=20):
 req=urllib.request.Request('http://127.0.0.1:8081'+path,data=None if data is None else json.dumps(data,ensure_ascii=False).encode(),headers={'Content-Type':'application/json'})
 with urllib.request.urlopen(req,timeout=timeout) as response:raw=response.read().decode()
 return json.loads(raw)
def metadata():
 p=call('/props');m=call('/v1/models')['data'];slots=call('/slots');assert len(m)==1 and m[0]['id']=='Qwen3.5-4B-Q4_K_M'
 return {'model':{k:m[0].get(k) for k in ['id','aliases','meta','architecture']},'model_path':p['model_path'],'chat_template':p['chat_template'],'context':min(s['n_ctx'] for s in slots),'build_info':p.get('build_info')}
def prepare(prompt):
 desc={'model':'Qwen3.5-4B-Q4_K_M','messages':[{'role':'system','content':system},{'role':'user','content':prompt}],'add_generation_prompt':True,'chat_template_kwargs':{'enable_thinking':False}}
 rendered=call('/apply-template',desc)['prompt'];assert prompt in rendered and system in rendered
 tokens=call('/tokenize',{'content':rendered,'add_special':True,'parse_special':True})['tokens'];assert len(tokens)+3500<=10000
 return desc,rendered,tokens
provider=metadata();save(out/'run.json',{'runner':'temporary Python reproduction of native llama.cpp transport; Rust compilation stalled','model':'Qwen3.5-4B-Q4_K_M','context':10000,'output_limit':3500,'temperature':0,'thinking':False,'concurrency':1,'retries':0,'publishes':False,'database_access':False,'rust_parser_executed':False,'provider':provider})
manifest=json.loads((trial/'manifest.json').read_text());assert len(manifest)==15
rows=[]
for entry in manifest:
 i=entry['case'];old_dir=trial/'emotional-claims'/f'{i:02}';fx=json.loads((old_dir/'fixture.json').read_text());original=fx['user_prompt'];old_req=json.loads((old_dir/'request.json').read_text());old_prepared=json.loads((old_dir/'prepared.json').read_text())
 assert provider==old_prepared['coverage']['provider'],'provider changed since baseline'
 assert old_req['json_schema']==schema and old_req['n_predict']==3500 and old_req['temperature']==0
 _,old_rendered,old_tokens=prepare(original)
 assert old_tokens==old_req['prompt'] and old_rendered==old_prepared['coverage']['rendered_prompt'],'baseline native transport not reproduced'
 prompt='{\n  "target_context": '+json.dumps(context,ensure_ascii=False)+',\n'+original[2:]
 check=json.loads(prompt);assert list(check)[0]=='target_context';assert check.pop('target_context')==context and check==json.loads(original)
 fx['prompt_version']=version;fx['user_prompt']=prompt;save(cases/Path(entry['fixture']).name,fx)
 folder=out/f'{i:02}';folder.mkdir();save(folder/'fixture.json',fx);(folder/'prompt.json').write_text(prompt+'\n')
 desc,rendered,tokens=prepare(prompt);req={k:v for k,v in old_req.items() if k!='json_schema'};req['prompt']=tokens;req['json_schema']=schema
 save(folder/'description.json',desc);save(folder/'request.json',req);save(folder/'prepared.json',{'coverage':{'provider':provider,'rendered_prompt':rendered,'token_ids':tokens,'context':provider['context']},'request':req,'baseline_tokens_reproduced':True})
 assert metadata()==provider
 row={'case':i,'name':entry['name'],'prompt_version':version,'baseline_tokens_reproduced':True,'input_tokens':len(tokens)}
 print('case',i,entry['name'],flush=True);start=time.monotonic()
 try:
  response=call('/completion',req,timeout=300);save(folder/'response.json',response)
  row['output_tokens']=response.get('tokens_predicted');assert response.get('stop')==True and response.get('stop_type')=='eos' and response.get('truncated')==False,'incomplete output'
  assert response['tokens_evaluated']==len(tokens),'input token mismatch'
  raw=response['content'];(folder/'output.json').write_text(raw+'\n');v=json.loads(raw)
  assert isinstance(v,dict) and set(v)=={'body','score','headline'}
  if not all(x is None for x in v.values()):
   assert type(v['score']) is int and 0<=v['score']<=100
   assert isinstance(v['body'],str) and v['body'].strip() and len(v['body'])<=12000
   assert isinstance(v['headline'],str) and v['headline'].strip() and len(v['headline'])<=140 and '\n' not in v['headline'] and '\r' not in v['headline']
  row['status']='structurally_valid'
 except Exception as e:row['status']='error';row['error']=str(e)
 row['wall_ms']=round((time.monotonic()-start)*1000);rows.append(row)
 with (out/'results.jsonl').open('a') as f:f.write(json.dumps(row)+'\n')
 print(row['status'],row.get('error',''),row['wall_ms'],flush=True)
assert len(rows)==15
save(out/'summary.json',{'completed':len(rows),'cases':rows,'rust_parser_executed':False})
