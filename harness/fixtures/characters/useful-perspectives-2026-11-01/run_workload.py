"""Four-slot read-only replay of full captured packages, with dependency-stage barriers.
Peer fixtures contain actual saved finished cards, not outputs from this replay.
This measures a bounded replay queue; it cannot certify production edition completion.
"""
import concurrent.futures
import json
import pathlib
import subprocess
import sys
import time
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent
REPO = ROOT.parents[2]
MODEL = 'Qwen3.5-4B-Q4_K_M'
URL = 'http://127.0.0.1:8081'

def identity():
    with urllib.request.urlopen(URL+'/v1/models',timeout=10) as response:
        data=json.load(response)
    assert MODEL in [d['id'] for d in data['data']], data
    return data

def run(root, path, queued, epoch):
    started=time.monotonic()
    output=root/path.stem
    proc=subprocess.run([str(REPO/'target/release/examples/character_outputs'),str(output),URL,MODEL,str(path)],capture_output=True,text=True)
    result_path=output/'01/result.json'
    result=json.loads(result_path.read_text()) if result_path.exists() else dict(status='runner_error',error=proc.stderr)
    result.update(queue_wait_ms=round((started-queued)*1000),started_ms=round((started-epoch)*1000),finished_ms=round((time.monotonic()-epoch)*1000),exit_code=proc.returncode)
    return result

def main():
    root=ROOT/(sys.argv[1] if len(sys.argv)>1 else 'workload')
    root.mkdir()
    before=identity()
    (root/'model-before.json').write_text(json.dumps(before,indent=2)+'\n')
    cases=sorted((ROOT/'cases').glob('real-*.json'))
    groups=[{'narratives','vibe','rating','transfer'},{'momentum'},{'oracle'}]
    epoch=time.monotonic()
    records=[]
    with (root/'results.jsonl').open('w') as ledger:
        for tasks in groups:
            queued=time.monotonic()
            with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
                futures=[pool.submit(run,root,p,queued,epoch) for p in cases if json.loads(p.read_text())['task'] in tasks]
                for future in concurrent.futures.as_completed(futures):
                    record=future.result()
                    records.append(record)
                    ledger.write(json.dumps(record,ensure_ascii=False)+'\n');ledger.flush()
                    print(record.get('name'),record['status'],record.get('wall_ms'),flush=True)
    elapsed=time.monotonic()-epoch
    after=identity()
    assert before['data'][0]['id']==after['data'][0]['id']
    (root/'model-after.json').write_text(json.dumps(after,indent=2)+'\n')
    summary=dict(concurrency=4,wall_seconds=elapsed,requests=len(records),
                 generation_calls=sum('output_tokens' in r for r in records),
                 input_tokens=sum(r.get('input_tokens',0) for r in records),output_tokens=sum(r.get('output_tokens',0) for r in records),
                 sum_request_wall_ms=sum(r.get('wall_ms',0) for r in records),
                 max_queue_wait_ms=max(r['queue_wait_ms'] for r in records),
                 status_counts={s:sum(r['status']==s for r in records) for s in sorted({r['status'] for r in records})},
                 database_access=False,publishes=False,retries=0,
                 qualification='Bounded frozen-workload replay, not a production queue drain. Peer inputs are saved finished cards; parser acceptance is not a semantic quality judgment.')
    (root/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    print(json.dumps(summary),flush=True)

if __name__=='__main__':main()
