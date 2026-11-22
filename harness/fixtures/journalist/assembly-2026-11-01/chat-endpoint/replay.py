import json,pathlib,urllib.request,time
root=pathlib.Path('fixtures/journalist/assembly-2026-11-01')
out=root/'chat-endpoint';out.mkdir()
for line in (root/'cases.jsonl').read_text().splitlines():
 c=json.loads(line);name=c['name'];p=out/name;p.mkdir()
 i=json.loads((root/'layout-order-complete'/f'{name}-json-body-first'/'input.json').read_text())
 request={'model':i['model'],'messages':[{'role':'system','content':i['system']},{'role':'user','content':i['user_prompt']}], 'temperature':0,'max_tokens':i['num_predict'],'chat_template_kwargs':{'enable_thinking':False},'response_format':{'type':'json_schema','json_schema':{'name':'journalist','strict':True,'schema':i['schema']}}}
 (p/'input.json').write_text(json.dumps(request,indent=2))
 start=time.time()
 with urllib.request.urlopen(urllib.request.Request('http://127.0.0.1:8081/v1/chat/completions',data=json.dumps(request).encode(),headers={'Content-Type':'application/json'}),timeout=180) as response:r=json.load(response)
 (p/'receipt.json').write_text(json.dumps(r,indent=2));(p/'output.json').write_text(r['choices'][0]['message']['content'])
 record={'name':name,'wall_ms':round((time.time()-start)*1000),'usage':r.get('usage'),'finish_reason':r['choices'][0]['finish_reason']}
 with (out/'results.jsonl').open('a') as f:f.write(json.dumps(record)+'\n')
 print(json.dumps(record),flush=True)
