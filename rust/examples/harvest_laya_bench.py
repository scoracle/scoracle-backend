"""Bounded local Laya throughput experiment: CPU/GPU, concurrency, and batching.

Cases are THREADS/CONCURRENT_CALLS/ARTICLES_PER_BATCH, e.g. 2/1/1,1/4/1,2/1/4.
One shared read-only model avoids multiplying checkpoint memory. Tokenization is
locked because fast-tokenizer settings are mutable; model calls may overlap. This
exercises an experimental execution path, not the serialized production HTTP adapter.
No database, network inference, or queue writes. Output is append-only per new run.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import hashlib
import importlib.metadata
import json
import math
from pathlib import Path
import platform
import resource
import statistics
import threading
import time


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--model-dir",type=Path,required=True)
    p.add_argument("--revision",required=True)
    p.add_argument("--prepared",type=Path,required=True)
    p.add_argument("--output",type=Path,required=True)
    p.add_argument("--device",choices=("cpu","mps","cuda"),default="cpu")
    p.add_argument("--cases",default="2/1/1,4/1/1,1/4/1")
    p.add_argument("--question-sets",default="gate,full")
    p.add_argument("--limit",type=int,default=24)
    p.add_argument("--daily-articles",type=int,default=8900)
    args = p.parse_args()
    cases = [tuple(map(int,c.split('/'))) for c in args.cases.split(',')]
    if any(len(c)!=3 or min(c)<1 for c in cases) or args.limit<1:
        p.error("positive thread/concurrency/batch sizes and limit required")
    if args.device == "mps" and any(c[1] > 1 for c in cases):
        p.error("Concurrent MPS calls crashed Metal in this runtime; use serialized calls or batching")
    modes=args.question_sets.split(',')
    if set(modes)-{"gate","full"}:
        p.error("question sets must be gate and/or full")
    original=[json.loads(line) for line in args.prepared.read_text().splitlines()][:args.limit]
    if not original:
        p.error("empty corpus")
    output=args.output.open('x')
    import torch
    import laya
    from laya.common import build_sequence, serialize_state, collate_items
    torch.set_num_threads(cases[0][0])
    torch.set_num_interop_threads(1)
    loaded=time.perf_counter()
    agent=laya.load(str(args.model_dir),device=args.device)
    if agent.device.type!=args.device:
        raise RuntimeError("device unavailable; no silent fallback")
    lock=threading.Lock()
    with (args.model_dir/'model.safetensors').open('rb') as f:
        weights_hash=hashlib.file_digest(f,'sha256').hexdigest()
    metadata={"kind":"setup","host":platform.node(),"platform":platform.platform(),
        "model_dir":str(args.model_dir),"revision":args.revision,"weights_sha256":weights_hash,
        "prepared_sha256":hashlib.sha256(args.prepared.read_bytes()).hexdigest(),
        "device":str(agent.device),"parameter_dtype":str(next(agent.model.parameters()).dtype),
        "amp_enabled":agent.amp_enabled,"amp_dtype":str(agent.dtype),
        "runtime":{k:importlib.metadata.version(k) for k in ('laya','torch','transformers')},
        "load_seconds":time.perf_counter()-loaded,"articles":len(original),
        "scope":"in-process classification including tokenization and decoding; excludes fetch, HTTP, DB, queue"}
    def emit(value):
        output.write(json.dumps(value,ensure_ascii=False)+'\n');output.flush()
        if value['kind']!='predictions':print(json.dumps(value),flush=True)
    emit(metadata)

    def execute(chunk):
        started=time.perf_counter()
        # All original entity-specific question text is retained when batching.
        with lock:
            groups=[]
            for row in chunk:
                request=row['request'];ids=list(request['questions'])
                internal={qid:agent._to_internal(request['questions'][qid]) for qid in ids}
                state_ids=agent.tok(serialize_state(request['state']).replace(agent.tok.mask_token,' '),add_special_tokens=False)['input_ids']
                for qid in ids:
                    agent._check_question(qid,request['questions'][qid])
                    empty,_=build_sequence(agent.tok,'',internal[qid],agent.cfg['max_len'],agent.cfg['head_max_len'],state_ids=[])
                    if len(state_ids)>agent.cfg['max_len']-len(empty):
                        raise ValueError(f"input truncation {row['article_id']}/{qid}")
                items=agent._encode_state(request['state'],ids,internal)
                groups.append((row,ids,internal,items))
            batch=collate_items([g[3] for g in groups],agent.tok.pad_token_id)
        with torch.inference_mode():
            logits,act=agent._forward(batch)
        if agent.device.type!=args.device:
            raise RuntimeError("inference silently changed device")
        results=[];offset=0
        for row,ids,internal,items in groups:
            answers=agent._decode_answers(logits,act,items,ids,internal,offset)
            offset+=len(items)
            if set(answers)!=set(ids):raise ValueError("missing answers")
            for qid,answer in answers.items():
                probs=answer['probabilities']
                if set(probs)!=set(row['request']['questions'][qid]['criteria']):raise ValueError("option mismatch")
                if any(not math.isfinite(v) or not 0<=v<=1 for v in probs.values()) or abs(sum(probs.values())-1)>0.001:
                    raise ValueError("invalid distribution")
            results.append({'article_id':row['article_id'],'answers':answers})
        return results,time.perf_counter()-started

    for mode in modes:
        rows=[]
        for row in original:
            request=row['request']
            questions=request['questions'] if mode=='full' else {k:v for k,v in request['questions'].items() if k in ('entity','content')}
            rows.append(dict(row,request=dict(request,questions=questions)))
        # The public SDK reference catches mistakes in our heterogeneous batch path.
        references={}
        for row in rows[:4]:
            references[row['article_id']]=agent.predict(**row['request'])['answers']
        for threads,concurrency,batch_size in cases:
            torch.set_num_threads(threads)
            execute(rows[:batch_size])  # Warm this shape outside measured work.
            chunks=[rows[i:i+batch_size] for i in range(0,len(rows),batch_size)]
            latencies=[];predictions=[];errors=[]
            started=time.perf_counter()
            with ThreadPoolExecutor(max_workers=concurrency) as workers:
                futures=[workers.submit(execute,chunk) for chunk in chunks]
                for future in as_completed(futures):
                    try:
                        results,seconds=future.result();predictions.extend(results)
                        latencies.extend([seconds]*len(results))
                    except Exception as exc:errors.append(repr(exc))
            seconds=time.perf_counter()-started
            delta=0.0;flips=0
            for row in predictions:
                if row['article_id'] not in references:continue
                for qid,answer in row['answers'].items():
                    ref=references[row['article_id']][qid]
                    flips+=answer['choice']!=ref['choice']
                    delta=max(delta,max(abs(value-ref['probabilities'][key]) for key,value in answer['probabilities'].items()))
            latency=sorted(latencies)
            throughput=len(predictions)/seconds
            rss=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
            rss_mb=rss/(1024*1024) if platform.system()=='Darwin' else rss/1024
            key=f'{mode}:{threads}/{concurrency}/{batch_size}'
            emit({'kind':'result','case':key,'questions_per_article':len(rows[0]['request']['questions']),
                  'threads':threads,'concurrency':concurrency,'batch_size':batch_size,
                  'successful_articles':len(predictions),'errors':errors,'wall_seconds':seconds,
                  'articles_per_second':throughput,'projected_8900_minutes':args.daily_articles/throughput/60 if throughput else None,
                  'p50_service_ms':1000*statistics.median(latency) if latency else None,
                  'p95_service_ms':1000*latency[max(0,math.ceil(len(latency)*.95)-1)] if latency else None,
                  'process_lifetime_peak_rss_mb':rss_mb,'reference_max_probability_delta':delta,'reference_choice_flips':flips})
            emit({'kind':'predictions','case':key,'rows':predictions})
            if errors:raise RuntimeError('benchmark case failed; stopping further load')
    output.close()


if __name__=='__main__':
    main()
