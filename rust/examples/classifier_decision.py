"""Offline full-source decision benchmark; no admission, database writes or promotion.

prepare SOURCE.jsonl SCHEMA.json REQUESTS.jsonl
run REQUESTS.jsonl RECEIPTS.jsonl --backend laya|kev|intern [--url URL | --checkpoint DIR]
"""
import argparse
import hashlib
import importlib.util
import json
import math
import time
from pathlib import Path

from classifier_infer import call


def prepare(source, schema):
    target = source['query_entities'][0]
    questions = {}
    for family, spec in schema['vectors'].items():
        for label, description in spec['labels'].items():
            scope = f" About candidate target {target['name']}." if spec['scope'] == 'target_window' else ''
            questions[f'{family}.{label}'] = {'type': 'noul', 'instructions': description + scope}
    for name, spec in schema['ordinal_vectors'].items():
        questions[name] = {'type': 'choice', 'instructions': f"{name} for {target['name']}; unknown if unsupported.",
                          'criteria': {**spec['anchors'], 'unknown': 'Unsupported or unknown'}}
    questions['extraction_usable'] = {'type': 'noul', 'instructions':
        'Does the body contain usable reporting, rather than only an interstitial or publisher furniture?'}
    # Diagnostic predicates do not replace exact claim/speaker/time qualification.
    for name, statement in {
        'current_emotion': 'Current expressed feelings are attributed to the target or a named member or supporter.',
        'historical_emotion': 'Historical expressed feelings are attributed to the target or a named member or supporter.',
        'conditional_emotion': 'Future conditional feelings are attributed to the target or a named member or supporter.',
        'emotion_denial': 'A feeling itself is explicitly denied by the target or a named member or supporter.',
        'report_denial': 'A reported statement or event concerning the target is explicitly denied.',
        'withdrawn': 'An earlier emotional claim concerning the target is explicitly withdrawn or corrected.',
    }.items():
        questions[f'diagnostic.{name}'] = {'type': 'noul', 'instructions':
            statement + f" Target: {target['name']}. A denied or withdrawn report is not an expressed feeling."}
    state = (f"Candidate target: {target['name']}\nPublisher: {source['source']}\n"
             f"Report timestamp: {source.get('published_at')}\nComplete source body:\n{source['body']}")
    return {'article_id': source['article_id'], 'target': target,
            'body_sha256': hashlib.sha256(source['body'].encode()).hexdigest(),
            'state': state, 'questions': questions, 'production_eligible': False}


def preflight(url, request, backend):
    def tokenize(text):
        return call(url, '/tokenize', {'content': text, 'add_special': False, 'parse_special': True})['tokens']
    counts = []
    for q in request['questions'].values():
        if q['type'] == 'noul':
            options = [('false', 'no, the statement does not hold'), ('true', 'yes, the statement holds')]
        else:
            options = list(q['criteria'].items())
        if backend == 'laya':
            head = f"{q['type']} question: {q['instructions']}"
            rendered = [(key, f'[MASK] {key}: {description}') for key, description in options]
            option_lengths = [len(tokenize(s)) for _, s in rendered]
            # Match upstream fill_task_laya limits; refuse clipped question/options.
            if max(option_lengths) > 41 or sum(option_lengths) + len(tokenize(head)) > 192:
                raise ValueError('Laya question/option head would be clipped')
            prompt = '[CLS]' + head + '[SEP]' + ''.join(s for _, s in rendered) + '[SEP]' + request['state'] + '[SEP]'
        else:
            if q['type'] == 'noul':
                options = [('no', None), ('yes', None)]
            prompt = '<|fim_prefix|>' + request['state'] + '<|fim_middle|>' + q['instructions']
            prompt += ''.join('<|box_start|>' + key + (': ' + value if value else '') + '<|box_end|>'
                              for key, value in options) + '<|fim_suffix|>'
        counts.append(len(tokenize(prompt)))
    if max(counts) > 8192:
        raise ValueError(f'complete input exceeds 8192 tokens: {max(counts)}')
    return {'question_token_counts': counts, 'input_tokens': sum(counts), 'full_source': True,
            'source_truncated': False, 'head_truncated': False}


def validate_answers(questions, answers):
    if set(answers) != set(questions):
        raise ValueError('missing or extra answers')
    for name, q in questions.items():
        a = answers[name]
        if q['type'] == 'noul':
            if not math.isfinite(a['noul']) or not 0 <= a['noul'] <= 1:
                raise ValueError('invalid predicate score')
        else:
            if set(a['probabilities']) != set(q['criteria']):
                raise ValueError('candidate options changed')
            probs = list(a['probabilities'].values())
            if any(not math.isfinite(p) or not 0 <= p <= 1 for p in probs) or abs(sum(probs) - 1) > .001:
                raise ValueError('invalid decision distribution')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['prepare', 'run', 'check'])
    parser.add_argument('paths', nargs='*', type=Path)
    parser.add_argument('--backend', choices=['laya', 'kev', 'intern'])
    parser.add_argument('--url', default='http://127.0.0.1:11437')
    parser.add_argument('--checkpoint', type=Path)
    args = parser.parse_args()
    if args.mode == 'check':
        validate_answers({'a': {'type': 'noul'}}, {'a': {'noul': .4}})
        for bad in ({}, {'a': {'noul': math.nan}}, {'a': {'noul': 2}}):
            try:
                validate_answers({'a': {'type': 'noul'}}, bad)
            except (ValueError, KeyError):
                continue
            raise AssertionError('bad answer accepted')
        print('Decision boundary check passed')
        return
    if args.mode == 'prepare':
        source, schema, output = args.paths
        schema = json.loads(schema.read_text())
        with output.open('x') as f:
            for row in map(json.loads, source.read_text().splitlines()):
                f.write(json.dumps(prepare(row, schema), ensure_ascii=False) + '\n')
        return
    requests, output = args.paths
    engine = None
    if args.backend == 'intern':
        spec = importlib.util.spec_from_file_location('intern_inference', args.checkpoint / 'inference.py')
        module = importlib.util.module_from_spec(spec)
        import sys
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
        engine = module.DecisionEngine(args.checkpoint, device='cuda', dtype='float32', attn_implementation='eager')
    with output.open('x') as f:
        for row in map(json.loads, requests.read_text().splitlines()):
            receipt = {k: row[k] for k in ('article_id', 'target', 'body_sha256')}
            receipt.update(backend=args.backend, production_eligible=False, status='error', responses=[])
            started = time.perf_counter()
            try:
                items = list(row['questions'].items())
                size = 16 if engine else len(items)
                answers = {}
                for offset in range(0, len(items), size):
                    request = {'state': row['state'], 'questions': dict(items[offset:offset + size])}
                    if engine:
                        _, batch, _ = engine.backend.encode(request)
                        pre = {'input_tokens': int(batch['input_ids'].shape[-1]), 'full_source': True,
                               'source_truncated': False, 'head_truncated': False}
                        inference_started = time.perf_counter()
                        result = engine.predict(request)
                    else:
                        pre = preflight(args.url, request, args.backend)
                        inference_started = time.perf_counter()
                        result = call(args.url, '/v1/systemone', request)
                    receipt['responses'].append({'preflight': pre, 'response': result,
                        'inference_ms': (time.perf_counter() - inference_started) * 1000})
                    if not engine:
                        if result['usage']['input_tokens'] != pre['input_tokens']:
                            raise ValueError('native token count differs from exact full-source preflight')
                    validate_answers(request['questions'], result['answers'])
                    answers.update(result['answers'])
                validate_answers(row['questions'], answers)
                receipt.update(status='complete', answers=answers)
            except Exception as e:
                receipt['error'] = f'{type(e).__name__}: {e}'
            receipt['elapsed_ms'] = (time.perf_counter() - started) * 1000
            f.write(json.dumps(receipt, ensure_ascii=False, allow_nan=False) + '\n')
            f.flush()
            print(row['article_id'], receipt['status'], round(receipt['elapsed_ms'], 2), receipt.get('error', ''), flush=True)


if __name__ == '__main__':
    main()
