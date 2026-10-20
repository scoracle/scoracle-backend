"""Assemble the historical interrupted replay without replacing either pass."""
from pathlib import Path
from collections import Counter
from datetime import datetime
import json, shutil, hashlib, sys
base = Path(sys.argv[1])
recovery = base / 'recovery'
out = base / 'combined'
out.mkdir()
(out / 'results').mkdir()

def read(p):
    return json.loads(p.read_text())

def rows(p):
    return [json.loads(s) for s in p.read_text().splitlines()]
a = rows(base / 'results/events.jsonl')
b = rows(recovery / 'results/events.jsonl')
count = Counter(((e['entity_type'], e['entity_id']) for e in a))
kept = [e for e in a if count[e['entity_type'], e['entity_id']] == 6]
discarded = [e for e in a if count[e['entity_type'], e['entity_id']] < 6]
assert len(kept) == 452 * 6 and len(b) == 12 * 6
assert read(recovery / 'results/summary.json')['read_only']
assert read(base / 'results/run.json')['read_only']
for origin, events in [(base, kept), (recovery, b)]:
    for kind, ident in {(e['entity_type'], e['entity_id']) for e in events}:
        name = f'{kind}-{ident}'
        shutil.copytree(origin / 'results' / name, out / 'results' / name)
start = datetime.fromisoformat((base / 'test-started-at.txt').read_text().strip())
resume_start = datetime.fromisoformat((recovery / 'test-started-at.txt').read_text().strip())
restored = read(recovery / 'monitor-restoration.json')
end = datetime.fromisoformat(restored['at'])
assert restored['exit_code'] == 0 and restored['stdout'].strip() == 'active'
first_trace = rows(base / 'slots.jsonl')
second_trace = rows(recovery / 'slots.jsonl')
offset = (resume_start - start).total_seconds()
for e in b:
    e['run_elapsed_seconds'] += offset
merged = kept + b
(out / 'results/events.jsonl').write_text(''.join((json.dumps(e) + '\n' for e in merged)))
for e in second_trace:
    e['seconds'] += offset
(out / 'slots.jsonl').write_text(''.join((json.dumps(e) + '\n' for e in first_trace + second_trace)))
for file in ['roster.json', 'run.json']:
    shutil.copyfile(base / 'results' / file, out / 'results' / file)
shutil.copyfile(base / 'driver.rs', out / 'driver.rs')
shutil.copyfile(recovery / 'monitor-restoration.json', out / 'monitor-restoration.json')
active = first_trace[-1]['seconds'] + read(recovery / 'results/summary.json')['total_seconds']
summary = {'assembled_from_two_passes': True, 'subjects': 464, 'stage_results': len(merged), 'preparation_seconds': read(base / 'results/preparation.json')['seconds'] + read(recovery / 'results/preparation.json')['seconds'], 'active_replay_seconds': active, 'total_seconds': (end - start).total_seconds(), 'intermission_and_restoration_seconds': (end - start).total_seconds() - active, 'model_calls': sum((e['model_calls'] for e in merged)), 'read_only': True, 'publishes': False, 'interruption': {'reason': 'Progress sampler read a partially written JSONL tail and terminated its child; sampler now reads newline-complete records only.', 'completed_subjects_before_interruption': 452, 'rerun_subjects': 12, 'excluded_partial_stage_receipts': len(discarded), 'extra_recorded_model_responses': sum((e['model_calls'] for e in discarded)), 'unreceipted_inflight_attempts': 'Up to four at cancellation; exact count unknown.', 'recovery_driver_sha256': hashlib.sha256((recovery / 'driver.rs').read_bytes()).hexdigest(), 'sources': {'first_pass': str(base), 'recovery': str(recovery)}}}
(out / 'results/summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary, indent=2))
