"""Read-only three-hour rollout capture. Raw evidence stays on its originating host."""
import collections
import datetime as dt
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import urllib.parse
import urllib.request

SINCE = '2026-11-02T23:34:50Z'
END = dt.datetime.fromisoformat('2026-11-03T03:10:00+00:00')
STAGES = {'narratives', 'vibe', 'transfers'}
ANSI = re.compile(r'\x1b\[[0-9;]*m')

def env_values(path, names):
    result = {}
    for line in path.read_text().splitlines():
        key, sep, value = line.strip().removeprefix('export ').partition('=')
        if sep and key in names:
            words = shlex.split(value, comments=True)
            if words:
                result[key] = words[0]
    return result

def metrics(lines):
    calls = collections.defaultdict(list)
    jobs = collections.Counter()
    for line in lines:
        line = ANSI.sub('', line)
        stamp = re.search(r'\d{4}-\d\d-\d\dT[\d:.]+Z', line)
        stage = re.search(r'stage=(\w+)', line)
        if not stamp or stamp[0][:19] < SINCE[:19] or not stage or stage[1] not in STAGES:
            continue
        if 'character model call completed' in line:
            fields = {k: int(v) for k, v in re.findall(r'(input_tokens|output_tokens|wall_ms)=(\d+)', line)}
            if len(fields) == 3:
                calls[stage[1]].append(fields)
        if 'work execution completed' in line:
            jobs[stage[1] + (':completed' if 'success=true' in line else ':failed')] += 1
    result = {}
    for stage, rows in calls.items():
        times = sorted(r['wall_ms'] for r in rows)
        tokens = [r['input_tokens'] for r in rows]
        result[stage] = dict(calls=len(rows), input_tokens_min=min(tokens), input_tokens_max=max(tokens),
            input_tokens_total=sum(tokens), output_tokens_total=sum(r['output_tokens'] for r in rows),
            wall_ms_total=sum(times), wall_ms_p50=times[(len(times)-1)//2],
            wall_ms_p95=times[math.ceil(len(times)*.95)-1])
    return {'model_calls': result, 'job_events': dict(jobs)}

def rejection_category(error):
    text = str(error).lower()
    for needle, category in [
        ('must begin with the full subject name', 'target_name_opening'),
        ('named claim is absent', 'unsupported_named_claim'),
        ('cited reports do not connect', 'unsupported_fact_relationship'),
        ('chronology or causality', 'unsupported_chronology_or_cause'),
        ('finding', 'finding_contract'),
        ('headline', 'headline_contract'),
        ('missing field', 'json_contract'),
        ('expected', 'json_contract'),
        ('context', 'context_budget'),
    ]:
        if needle in text:
            return category
    return 'other_validation'

def main(host):
    assert host in ('mac', 'archbox')
    now = dt.datetime.now(dt.timezone.utc)
    if now > END:
        raise SystemExit('Monitoring window ended; no new capture')
    os.umask(0o077)
    root = Path('/home/sheneveld/scoracle/scoracle-backend' if host == 'archbox' else '/Users/scotty/scoracle-worker')
    base = Path('/mnt/data/backup/scoracle/prompt-baseline-20261102' if host == 'archbox' else '/Users/scotty/scoracle-worker/monitoring/prompt-baseline-20261102')
    output = base / now.strftime('%Y%m%dT%H%M%S%fZ')
    output.mkdir(parents=True, mode=0o700)
    values = env_values(root/'.env.local', {'DATABASE_PRIVATE_URL', 'DATABASE_URL', 'COGNITION_REJECTIONS_FILE'})
    summary = {'host': host, 'captured_at': now.isoformat(), 'since': SINCE, 'private_archive': str(output)}
    if host == 'archbox':
        result = subprocess.run(['journalctl','--user','-u','scoracle-writer.service','--since',SINCE,'--no-pager','-o','cat'], capture_output=True, text=True, check=True)
        summary.update(metrics(result.stdout.splitlines()))
        units = ['scoracle-api','scoracle-cognition','scoracle-coverage','scoracle-kev','scoracle-writer-model','scoracle-writer']
        summary['services'] = {unit: subprocess.run(['systemctl','--user','is-active',unit],capture_output=True,text=True).stdout.strip() for unit in units}
        summary['gpu'] = subprocess.check_output(['nvidia-smi','--query-gpu=memory.total,memory.used,utilization.gpu','--format=csv,noheader'],text=True).strip()
        url = urllib.parse.urlparse(values.get('DATABASE_PRIVATE_URL') or values['DATABASE_URL'])
        pg = {**os.environ, 'PGHOST':url.hostname, 'PGPORT':str(url.port or 5432),
            'PGUSER':urllib.parse.unquote(url.username or ''), 'PGPASSWORD':urllib.parse.unquote(url.password or ''),
            'PGDATABASE':url.path.lstrip('/'), 'PGCONNECT_TIMEOUT':'10',
            'PGOPTIONS':'-c default_transaction_read_only=on -c statement_timeout=60000'}
        sslmode = urllib.parse.parse_qs(url.query).get('sslmode')
        if sslmode:
            pg['PGSSLMODE'] = sslmode[0]
        command = ['psql','-X','-qAt','-v','ON_ERROR_STOP=1','-v','since='+SINCE]
        with (output/'evidence.jsonl').open('x') as evidence:
            subprocess.run(command+['-f',str(Path(__file__).with_name('archive.sql'))],env=pg,stdout=evidence,check=True,timeout=120)
        summary['database'] = json.loads(subprocess.check_output(command+['-f',str(Path(__file__).with_name('summary.sql'))],env=pg,text=True,timeout=75))
        port = 8082
    else:
        with (root/'logs/cognition.log').open() as log:
            summary.update(metrics(log))
        service = subprocess.run(['launchctl','print','gui/501/com.scoracle.cognition'],capture_output=True,text=True)
        summary['writer_running'] = service.returncode == 0 and '\n\tstate = running' in service.stdout
        port = 8081
    try:
        summary['model_health'] = json.load(urllib.request.urlopen(f'http://127.0.0.1:{port}/health',timeout=5))
    except Exception as error:
        summary['model_health'] = {'error':str(error)}
    path = Path(values.get('COGNITION_REJECTIONS_FILE','logs/rejected-outputs.jsonl'))
    if not path.is_absolute():
        path = root/path
    rejects = collections.Counter()
    reasons = collections.Counter()
    malformed = 0
    undated = 0
    with path.open() as source, (output/'rejections.jsonl').open('x') as archived:
        for line in source:
            try:
                row = json.loads(line)
            except json.JSONDecodeError:
                malformed += 1
                continue
            if not isinstance(row, dict):
                malformed += 1
                continue
            if not isinstance(row.get('recorded_at'), str):
                undated += 1
                continue
            claim = row.get('claim') or {}
            if row['recorded_at'] >= SINCE and claim.get('stage') in STAGES:
                archived.write(line)
                rejects[claim['stage']] += 1
                reasons[claim['stage']+':'+rejection_category(row.get('error'))] += 1
    summary['rejected_generation_attempts'] = dict(rejects)
    summary['rejection_categories'] = dict(reasons)
    summary['archive_parse_errors'] = malformed
    summary['undated_archive_records_excluded'] = undated
    summary['archive_files'] = {p.name:{'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in output.iterdir()}
    (output/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    print(json.dumps(summary))

if __name__ == '__main__':
    main(sys.argv[1])
