import importlib.util
from pathlib import Path
import tempfile
spec=importlib.util.spec_from_file_location('capture',Path(__file__).with_name('capture.py'))
capture=importlib.util.module_from_spec(spec);spec.loader.exec_module(capture)
lines=[
 '2026-11-02T23:34:49Z stage=narratives character model call completed input_tokens=9999 output_tokens=1 wall_ms=1',
 '\x1b[32m2026-11-02T23:34:50.123Z stage=narratives character model call completed input_tokens=1000 output_tokens=50 wall_ms=2000\x1b[0m',
 '2026-11-02T23:35:00Z stage=narratives character model call completed input_tokens=1200 output_tokens=0 wall_ms=1000',
 '2026-11-02T23:35:00Z stage=narratives work execution completed success=false',
 '2026-11-02T23:35:00Z stage=vibe work execution completed success=true',
 '2026-11-02T23:35:00Z stage=classifier character model call completed input_tokens=9999 output_tokens=1 wall_ms=1',
]
m=capture.metrics(lines)
assert m['model_calls']['narratives']==dict(calls=2,input_tokens_min=1000,input_tokens_max=1200,input_tokens_total=2200,output_tokens_total=50,wall_ms_total=3000,wall_ms_p50=1000,wall_ms_p95=2000)
assert m['job_events']=={'narratives:failed':1,'vibe:completed':1}
assert capture.metrics([])=={'model_calls':{},'job_events':{}}
with tempfile.TemporaryDirectory() as directory:
 p=Path(directory)/'env';p.write_text('export KEEP="value with spaces"\nSECRET=do-not-return\nKEEP2=value # comment\n')
 assert capture.env_values(p,{'KEEP','KEEP2'})=={'KEEP':'value with spaces','KEEP2':'value'}
print('Telemetry window, ANSI parsing, zero tokens, distinct counters, empty logs and env allowlist passed')

assert capture.rejection_category('model reply rejected: named claim is absent from cited reports: Example')=='unsupported_named_claim'
assert capture.rejection_category('Insider reading must begin with the full subject name')=='target_name_opening'
assert capture.rejection_category(None)=='other_validation'
