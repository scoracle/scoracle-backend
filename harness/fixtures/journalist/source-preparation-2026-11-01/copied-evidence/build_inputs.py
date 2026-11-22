"""Keep the worked writer; compare copied clauses with a zero-inference full-copy control."""
import copy, hashlib, json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
BASE = ROOT.parent / 'worked-writer'
encode = lambda value: json.dumps(value, ensure_ascii=False, separators=(',', ':'))

def build():
    (ROOT / 'writer.txt').write_bytes((BASE / 'writer.txt').read_bytes())
    prompt = ROOT / 'preparer.txt'
    prompt.write_text(prompt.read_text().strip())
    for cohort, count in [('development', 29), ('validation', 11)]:
        control = f'{cohort}-control.jsonl'
        (ROOT / control).write_bytes((BASE / control).read_bytes())
        cases = list(map(json.loads, (ROOT / control).read_text().splitlines()))
        for case in cases:
            packet = json.loads(case['user_prompt'])
            for group in packet['fresh']:
                for source in group['reports']:
                    source['preparation'] = {'statements': [{'claim': source['headline']}], 'unspecified': []}
            case['user_prompt'] = encode(packet)
        (ROOT / f'{cohort}-verbatim.jsonl').write_text(''.join(json.dumps(c, ensure_ascii=False) + '\n' for c in cases))
        sources = list(map(json.loads, (BASE / f'{cohort}-sources.jsonl').read_text().splitlines()))
        assert len(sources) == count == len({s['name'] for s in sources})
        for source in sources:
            schema = json.loads(source['schema_raw'])
            old = copy.deepcopy(schema)
            props = schema['properties']['statements']['items']['properties']
            choices = props['about']['items']['enum']
            text = json.loads(source['user_prompt'])['headline']
            assert text in choices and all(value and value in text for value in choices)
            props['claim']['enum'] = choices
            schema['properties']['unspecified']['items']['enum'] = choices
            source['schema_raw'] = encode(schema)
            del props['claim']['enum']
            del schema['properties']['unspecified']['items']['enum']
            assert schema == old
        (ROOT / f'{cohort}-sources.jsonl').write_text(''.join(json.dumps(c, ensure_ascii=False) + '\n' for c in sources))
    files = ['writer.txt', 'preparer.txt', 'protocol.txt', 'build_inputs.py', 'contract.py', 'run.py']
    files += [f'{c}-{a}.jsonl' for c in ['development', 'validation'] for a in ['control', 'verbatim', 'sources']]
    (ROOT / 'manifest.json').write_text(json.dumps({'baseline': '../worked-writer', 'sha256': {
        f: hashlib.sha256((ROOT / f).read_bytes()).hexdigest() for f in files}}, indent=2) + '\n')
    print('Frozen 29/11 source inputs; claims and unknown expressions restricted to exact source spans; writer unchanged.')

if __name__ == '__main__': build()
