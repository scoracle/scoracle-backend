from pathlib import Path
import subprocess,json,tempfile
import os
url=os.environ['TEST_DATABASE_URL']
root=Path(__file__).resolve().parents[4]
def sql(s):return subprocess.check_output(['psql',url,'-X','-Atv','ON_ERROR_STOP=1'],input=s,text=True).strip()
assert sql('SELECT current_database()').startswith('classifier_test_coverage'), 'disposable database required'
setup=(root/'sql/tests/323_headline_selection.sql').read_text().split('DO $$ BEGIN\n    IF public.coverage_select_headlines')[0]
sql(setup+"""
 UPDATE classifier_sources SET source=jsonb_set(source,'{identity_candidates}','[{"entity_type":"team","entity_id":925001}]') WHERE sport='ZZ_SELECTION';
 INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport,feed_rank) SELECT article_id,'team',925001,'ZZ_SELECTION',1 FROM policy_sources;
 INSERT INTO classifier_source_heads(article_id,sport,source_id) SELECT article_id,sport,id FROM classifier_sources WHERE sport='ZZ_SELECTION';
 UPDATE classifier_sources SET discovery_version=classifier_discovery_version(article_id,sport) WHERE sport='ZZ_SELECTION';
 UPDATE classifier_spectrum_deliveries d SET status='held',reason='model_reply_rejected' FROM classifier_spectra x WHERE x.id=d.spectrum_id AND x.sport='ZZ_SELECTION' AND d.status='pending';
 DELETE FROM pipeline_work WHERE sport='ZZ_SELECTION';
 UPDATE coverage_assignments SET outcome='failed',resolution='blocked',input_version=CASE stage WHEN 'transfers' THEN 'classifier-spectrum-v1:fixture' ELSE 'fixture' END WHERE sport='ZZ_SELECTION';
 COMMIT;""")
scope=json.dumps([dict(stage=x,entity_type='team',entity_id=925001,sport='ZZ_SELECTION') for x in ['narratives','vibe','transfers']])
cmd=['psql',url,'-X','-v','ON_ERROR_STOP=1','-v','edition_date=2098-01-02','-v','assignment_scope='+scope,'-v','spam_spectra={}','-f',str(root/'rust/docs/releases/2026-11-02-paragraphs/retry_changed_prompts.sql')]
def run(*args,ok=True):
 r=subprocess.run(cmd+list(args),text=True,capture_output=True);assert (r.returncode==0)==ok,r.stdout+r.stderr;return r.stdout
before=sql("SELECT jsonb_agg(to_jsonb(d) ORDER BY spectrum_id,character) FROM classifier_spectrum_deliveries d")
run();assert sql("SELECT jsonb_agg(to_jsonb(d) ORDER BY spectrum_id,character) FROM classifier_spectrum_deliveries d")==before
sql("UPDATE classifier_sources SET discovery_version='stale' WHERE sport='ZZ_SELECTION'")
assert 'recovered_assignments' in run() and '\n                     0' in run()
sql("UPDATE classifier_sources SET discovery_version=classifier_discovery_version(article_id,sport) WHERE sport='ZZ_SELECTION'")
sql("UPDATE coverage_assignments SET prepared_input='{}' WHERE sport='ZZ_SELECTION' AND stage='vibe'")
run(ok=False)
sql("UPDATE coverage_assignments SET prepared_input=NULL WHERE sport='ZZ_SELECTION'")
sql("BEGIN; UPDATE coverage_schedule SET enabled=false; INSERT INTO news_summaries(entity_type,entity_id,sport,trigger_type,body,reporting_date) VALUES('team',925001,'ZZ_SELECTION','manual','Existing candidate','2098-01-01'); UPDATE news_summaries SET candidate_edition='2098-01-02' WHERE sport='ZZ_SELECTION'; UPDATE coverage_schedule SET enabled=true; COMMIT;")
run(ok=False)
sql("DELETE FROM news_summaries WHERE sport='ZZ_SELECTION'")
sql("UPDATE classifier_spectrum_deliveries SET reason='unresolved_target_identity' WHERE character='influencer' AND status='held'")
with tempfile.TemporaryDirectory() as d:
 run('-v','apply=true','-v','backup_file='+d+'/before.jsonl')
 assert sql("SELECT count(*) FROM coverage_assignments WHERE sport='ZZ_SELECTION' AND outcome='pending'")=='2'
 assert sql("SELECT count(*) FROM classifier_spectrum_deliveries WHERE character='influencer' AND reason='unresolved_target_identity'")=='55'
 assert sql("SELECT count(*) FROM classifier_spectrum_deliveries WHERE character='insider' AND status='pending'")=='10'
 assert sql("SELECT count(*) FROM pipeline_work WHERE sport='ZZ_SELECTION' AND status='pending' AND attempts=0")=='2'
 assert sql("SELECT input_version FROM pipeline_work WHERE sport='ZZ_SELECTION' AND stage='transfers'").startswith('classifier-spectrum-v1:')
 assert len(Path(d+'/before.jsonl').read_text().splitlines())>2
 run(ok=False)
print('Preview rollback, stale-source exclusion, prepared/product fences, held identities, top-ten selection, backup and repeat refusal passed')
