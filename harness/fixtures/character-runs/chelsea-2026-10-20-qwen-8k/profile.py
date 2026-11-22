import json,copy,pathlib,urllib.request,hashlib
ROOT=pathlib.Path(__file__).resolve().parent.parent/'chelsea-2026-10-20'
OUT=pathlib.Path('/private/tmp/chelsea-token-profile.json')
def api(path,payload):
 r=urllib.request.Request('http://127.0.0.1:8081'+path,data=json.dumps(payload,ensure_ascii=False).encode(),headers={'Content-Type':'application/json'})
 return json.load(urllib.request.urlopen(r,timeout=30))
def tokens(text):return len(api('/tokenize',{'content':text,'add_special':True,'parse_special':True})['tokens'])
def template(prompt,system=None):
 messages=([{'role':'system','content':system}] if system else [])+[{'role':'user','content':prompt}]
 return api('/apply-template',{'model':'Qwen3.5-4B-Q4_K_M','messages':messages,'add_generation_prompt':True,'chat_template_kwargs':{'enable_thinking':False}})['prompt']
def count(v,system=None):return tokens(template(json.dumps(v,ensure_ascii=False,separators=(',',':')),system))
def strip(v):
 if isinstance(v,dict):
  return {k:strip(x) for k,x in v.items() if k not in ('url','gate','spectrum')}
 if isinstance(v,list):return [strip(x) for x in v]
 return v
results={}
for name in ['journalist','influencer','scout','insider','analyst']:
 source=json.loads((ROOT/(name+'-input.json')).read_text());receipt=json.loads((ROOT/(name+'-output.json')).read_text());system=None
 if name=='influencer':
  prompt=receipt['product']['receipt']['packet'];system=json.loads((ROOT.parent.parent/'influencer/top3-followup-2026-10-20/synthetic-cases.json').read_text())[0]['system']
 elif name=='scout':prompt=source['assignment']['built_prompt']
 else:prompt=source['prompt']
 v=json.loads(prompt);full=tokens(template(prompt,system));clean=strip(v);clean_count=count(clean,system)
 empty=copy.deepcopy(v)
 if 'fresh' in empty:empty['fresh']=[] if isinstance(empty['fresh'],list) else {}
 if 'memories' in empty:empty['memories']=[]
 overhead=count(empty,system)
 row={'reported_input_tokens':receipt['calls'][0]['input_tokens'],'qwen_input_tokens':full,'qwen_input_without_routing_metadata':clean_count,'routing_metadata_marginal_tokens':full-clean_count,'instructions_identity_template_tokens':overhead,'reserved_output_tokens':receipt['calls'][0]['request']['n_predict'],'memories':len(source.get('parts',source).get('memories',[])),'full_prompt_sha256':hashlib.sha256(prompt.encode()).hexdigest()}
 results[name]=row
 print(name,row)
OUT.write_text(json.dumps({'tokenizer':'live Qwen3.5 native /apply-template + /tokenize; no inference','cases':results},indent=2)+'\n')

# Recheck the retained synthetic 58-headline, 20-memory budget without generation.
stress_path = pathlib.Path(__file__).resolve().parent / 'memory-stress-packet.json'
stress = json.loads(stress_path.read_text())
stress_input = count(stress)
assert stress_input == 5556
assert stress_input + 2100 == 7656 < 7900
print('58 headlines + 20 synthetic memories:', stress_input, '+ 2100 output =', stress_input + 2100)
