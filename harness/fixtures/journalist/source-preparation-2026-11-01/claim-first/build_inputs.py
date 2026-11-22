"""Hold instructions/data fixed; emit the source claim before interpretive labels."""
import hashlib, json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
BASE = ROOT.parent / 'semantic-preparer'

def build():
    unchanged = ['writer.txt', 'preparer.txt', 'development-control.jsonl',
                 'validation-control.jsonl', 'run.py', 'screen-selection.json']
    for name in unchanged:
        (ROOT / name).write_bytes((BASE / name).read_bytes())
    sources = ['development-sources.jsonl', 'validation-sources.jsonl']
    for name in sources:
        rows = list(map(json.loads, (BASE / name).read_text().splitlines()))
        for row in rows:
            original = row['schema_raw']
            schema = json.loads(original)
            item = schema['properties']['statements']['items']
            order = ['claim', 'qualifications', 'about', 'speaker', 'kind']
            item['properties'] = {k: item['properties'][k] for k in order}
            # Required membership and every constraint remain unchanged.
            assert schema == json.loads(original)
            row['schema_raw'] = json.dumps(schema, ensure_ascii=False, separators=(',', ':'))
            assert row['schema_raw'] != original
        (ROOT / name).write_text(''.join(json.dumps(c, ensure_ascii=False) + '\n' for c in rows))
        before = list(map(json.loads, (BASE / name).read_text().splitlines()))
        assert len(rows) == len(before)
        for a, b in zip(rows, before, strict=True):
            assert json.loads(a.pop('schema_raw')) == json.loads(b.pop('schema_raw')) and a == b
    files = unchanged + sources + ['protocol.txt', 'build_inputs.py']
    (ROOT / 'manifest.json').write_text(json.dumps({
        'baseline': '../semantic-preparer',
        'sha256': {n: hashlib.sha256((ROOT / n).read_bytes()).hexdigest() for n in files}
    }, indent=2) + '\n')
    print('Frozen schema-order-only change: claim, qualifications, about, speaker, kind; prompts and source values unchanged.')

if __name__ == '__main__':
    build()
