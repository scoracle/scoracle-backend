"""Pinned GLiNER2.5 full-spectrum + qualified-span experiment; never production.

SOURCE.jsonl SCHEMA.json RECEIPTS.jsonl --checkpoint DIR
"""
import argparse
import hashlib
import json
import time
from pathlib import Path


def quote(body, span):
    start, end, text = span['start'], span['end'], span['text']
    if start < 0 or start >= end or body[start:end] != text:
        raise ValueError('not an exact source character span')
    cursor, occurrence = 0, 0
    while (found := body.find(text, cursor)) != -1:
        if found == start:
            return {'quote': text, 'occurrence': occurrence}
        cursor = found + len(text)
        occurrence += 1
    raise ValueError('source span is not a non-overlapping quote occurrence')


def source_coverage(body, encoded):
    # GLiNER2 2.0.0 appends one terminal period; retain that transformation explicitly.
    if encoded not in (body, body + '.'):
        raise ValueError('preprocessor changed or omitted source bytes')
    return {'full_source': True, 'source_truncated': False, 'model_text': encoded,
            'appended_terminal_period': encoded != body}


def proposal(body, result, qualifiers):
    def spans(values):
        if values is None or values == []:
            return None
        return [quote(body, x) for x in (values if isinstance(values, list) else [values])]
    claims = []
    for raw in result.get('claim', []):
        def category(name):
            return raw[name]['text']
        claims.append({'evidence': quote(body, raw['evidence']),
            'target_relation': category('target_relation'), 'target_evidence': spans(raw.get('target_evidence')),
            'kind': category('kind'), 'time_scope': category('time_scope'),
            'candidate_dimensions': [x['text'] for x in raw.get('candidate_dimensions', [])],
            'qualifiers': {key: spans(raw.get(key)) for key in qualifiers}})
    usable = result['extraction_usable']['label'] == 'true'
    return {'complete_source_review': True, 'extraction_usable': usable, 'claims': claims}


def make_schema(model, vectors, target):
    from classifier_decision import prepare
    questions = prepare({'article_id': 0, 'body': 'schema only', 'source': '',
                         'query_entities': [target]}, vectors)['questions']
    schema = model.create_schema()
    for family, spec in vectors['vectors'].items():
        labels = {}
        for label, description in spec['labels'].items():
            scope = f" About candidate target {target['name']}." if spec['scope'] == 'target_window' else ''
            labels[label] = description + scope
        schema.classification(family, labels, multi_label=True, cls_threshold=0)
    for name, spec in vectors['ordinal_vectors'].items():
        schema.classification(name, {**spec['anchors'], 'unknown': 'Unsupported or unknown'},
                              description=questions[name]['instructions'])
    schema.classification('extraction_usable', {'true': 'Usable reporting',
        'false': 'Only an interstitial, navigation, advertisement or publisher furniture'})
    schema.classification('diagnostic', {name.split('.', 1)[1]: q['instructions']
        for name, q in questions.items() if name.startswith('diagnostic.')}, multi_label=True, cls_threshold=0)
    builder = schema.structure('claim', mode='natural', anchor='evidence')
    builder.field('evidence', dtype='str', cardinality='required_one',
        description='Complete literal assertion, reported event or expression of feeling with its qualification')
    builder.field('target_relation', dtype='str', choices=['direct_subject', 'other_subject', 'unknown'],
        description=f"Relationship of this claim to candidate target {target['name']}; names alone are not relevance")
    builder.field('target_evidence', dtype='list', description=f"Literal identity linking this claim to {target['name']}")
    builder.field('kind', dtype='str', choices=['emotion', 'emotion_denial', 'conditional_emotion',
        'report_denial', 'withdrawn', 'information'], description='Claim kind; denial of a statement is distinct from denial of a feeling')
    builder.field('time_scope', dtype='str', choices=['current', 'historical', 'future', 'unknown'],
        description='Explicit event or feeling time; publication date does not establish it')
    builder.field('candidate_dimensions', dtype='list', choices=list(vectors['vectors']['emotion']['labels']))
    descriptions = {
        'speaker': 'Literal identity of the speaker or reporting source who makes this claim',
        'subject': 'Literal identity of the person or team the assertion or feeling describes',
        'counterparty': 'Literal other participant in the reported event',
        'reported_event_time': 'Literal date or temporal expression qualifying this event or feeling',
        'negation': 'Literal words denying this particular assertion or feeling',
        'uncertainty': 'Literal condition, uncertainty or speculation qualifying this particular claim',
        'source_disagreement': 'Literal correction, withdrawal or disagreement qualifying this particular claim',
    }
    for name in vectors['qualifiers']:
        builder.field(name, dtype='list', description=descriptions[name])
    return schema


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('schema', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--checkpoint', type=Path)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    if args.check:
        assert quote('éé Alex Alex', {'start': 8, 'end': 12, 'text': 'Alex'}) == {'quote': 'Alex', 'occurrence': 1}
        assert source_coverage('Alex', 'Alex.')['appended_terminal_period']
        try:
            quote('Alex', {'start': 0, 'end': 3, 'text': 'Alex'})
        except ValueError:
            print('Source quote boundary check passed')
            return
        raise AssertionError('bad source span accepted')
    import torch
    import transformers
    from gliner2 import AutoExtractor
    from classifier_decision import validate_answers
    model = AutoExtractor.from_pretrained(args.checkpoint).to('cuda').eval()
    model.processor.change_mode(is_training=False)
    assert all(p.device.type == 'cuda' for p in model.parameters())
    vectors = json.loads(args.schema.read_text())
    sources = list(map(json.loads, args.source.read_text().splitlines()))
    provenance = {'checkpoint': str(args.checkpoint), 'revision': args.checkpoint.name,
        'torch': torch.__version__, 'transformers': transformers.__version__, 'dtype': str(next(model.parameters()).dtype),
        'device': torch.cuda.get_device_name(), 'parameters': sum(p.numel() for p in model.parameters()),
        'span_threshold': .5, 'classification_threshold': 0, 'gliner2': '2.0.0', 'production_eligible': False}
    # Pin the installed library's preprocessing/decode path; no source clipping or duplicate encoding.
    with args.output.open('x') as output, torch.inference_mode():
        for source in sources:
            target = source['query_entities'][0]
            body = source['body']
            receipt = {'article_id': source['article_id'], 'target': target, 'backend': args.checkpoint.parent.parent.name,
                'body_sha256': hashlib.sha256(body.encode()).hexdigest(), 'status': 'error',
                'production_eligible': False, 'model_provenance': provenance}
            started = time.perf_counter()
            try:
                schema = make_schema(model, vectors, target)
                schemas, metadata = model._build_schema_dicts_and_metadata([schema])
                batch = model.processor.collate_fn_inference([(body, schemas[0])], max_len=None,
                    error_policy='raise', architecture='boundary')
                tokens = batch.input_ids.shape[-1]
                coverage = source_coverage(body, batch.original_texts[0])
                if batch.end_mappings[0][-1] != len(batch.original_texts[0].rstrip()):
                    raise ValueError('preprocessor omitted text tokens')
                if tokens > model.config.max_len:
                    raise ValueError(f'full schema/source exceeds {model.config.max_len} tokens: {tokens}')
                receipt['preflight'] = {'input_tokens': tokens, **coverage, 'schema': schemas[0]}
                batch = batch.to('cuda')
                torch.cuda.synchronize()
                torch.cuda.reset_peak_memory_stats()
                t = time.perf_counter()
                result = model._extract_from_batch(batch, .5, metadata, True, True)[0]
                torch.cuda.synchronize()
                receipt['inference_ms'] = (time.perf_counter() - t) * 1000
                receipt['peak_cuda_bytes'] = torch.cuda.max_memory_allocated()
                result = model.format_results(result, True, metadata[0].get('relation_order', []),
                                              metadata[0].get('classification_tasks', []))
                receipt['response'] = result
                for family, spec in vectors['vectors'].items():
                    labels = {x['label']: x['confidence'] for x in result[family]}
                    if set(labels) != set(spec['labels']):
                        raise ValueError(f'missing complete scores for {family}')
                    validate_answers({k: {'type': 'noul'} for k in labels},
                                     {k: {'noul': v} for k, v in labels.items()})
                receipt['status'] = 'complete'
                try:
                    receipt['proposal'] = proposal(body, result, vectors['qualifiers'])
                except (ValueError, KeyError, TypeError) as e:
                    receipt['proposal_error'] = f'{type(e).__name__}: {e}'
            except Exception as e:
                receipt['error'] = f'{type(e).__name__}: {e}'
            receipt['elapsed_ms'] = (time.perf_counter() - started) * 1000
            output.write(json.dumps(receipt, ensure_ascii=False, allow_nan=False) + '\n')
            output.flush()
            print(source['article_id'], receipt['status'], round(receipt['elapsed_ms'], 2), receipt.get('error', ''), flush=True)


if __name__ == '__main__':
    main()
