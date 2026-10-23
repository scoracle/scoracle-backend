"""Preview/apply/fencing contracts; run only on an isolated TEST_DATABASE_URL."""
import json
import os
import sys
from pathlib import Path
import subprocess
import tempfile

BACKLOG = '--backlog' in sys.argv
DATABASE = os.environ['TEST_DATABASE_URL']
ROOT = Path(__file__).resolve().parents[2]


def sql(statement):
    return subprocess.check_output(['psql', DATABASE, '-X', '-Atv', 'ON_ERROR_STOP=1', '-c', statement], text=True).strip()


assert sql('SELECT current_database()').startswith('classifier_test_coverage'), 'disposable database required'
setup = (ROOT / 'sql/tests/323_headline_selection.sql').read_text().split('DO $$ BEGIN\n    IF public.coverage_select_headlines')[0]
scope = json.dumps([dict(stage='narratives', entity_type='team', entity_id=925001, sport='ZZ_SELECTION')])
command = ['psql', DATABASE, '-X', '-v', 'ON_ERROR_STOP=1', '-v', 'edition_date=2098-01-02',
           '-v', 'assignment_scope=' + scope, '-f', str(Path(__file__).with_name('recover_headline_budget.sql'))]
try:
    sql(setup + """
    UPDATE classifier_sources SET source=jsonb_set(source,'{identity_candidates}','[{"entity_type":"team","entity_id":925001}]')
      WHERE sport='ZZ_SELECTION';
    INSERT INTO harvester_query_provenance(article_id,entity_type,entity_id,sport,feed_rank)
      SELECT article_id,'team',925001,'ZZ_SELECTION',1 FROM policy_sources;
    INSERT INTO classifier_source_heads(article_id,sport,source_id)
      SELECT article_id,sport,id FROM classifier_sources WHERE sport='ZZ_SELECTION';
    UPDATE classifier_sources SET discovery_version=classifier_discovery_version(article_id,sport) WHERE sport='ZZ_SELECTION';
    UPDATE classifier_spectrum_deliveries d SET status='held',reason='daily_package_exceeds_context_budget'
      FROM classifier_spectra x JOIN policy_sources p USING(article_id)
      WHERE d.spectrum_id=x.id AND d.character='journalist' AND p.i<>0;
    UPDATE coverage_assignments SET outcome='failed',resolution='blocked',input_version='coverage:2098-01-02'
      WHERE sport='ZZ_SELECTION' AND stage='narratives';
    INSERT INTO pipeline_work(stage,entity_type,entity_id,sport,status,attempts,input_version)
      VALUES('narratives','team',925001,'ZZ_SELECTION','failed',5,'coverage:2098-01-02')
      ON CONFLICT(stage,entity_type,entity_id,sport) DO UPDATE SET status='failed',attempts=5,input_version=EXCLUDED.input_version;
    UPDATE pipeline_work SET status='failed',attempts=5,input_version='coverage:2098-01-02'
      WHERE sport='ZZ_SELECTION' AND stage='narratives';
    COMMIT;
    """)
    spam_id = sql("SELECT x.id FROM classifier_spectra x JOIN news_articles a ON a.id=x.article_id WHERE a.url='https://example.invalid/policy/55'")
    command += ['-v', 'spam_spectra={' + spam_id + '}']
    untouched = sql("SELECT jsonb_agg(to_jsonb(d) ORDER BY spectrum_id,character) FROM classifier_spectrum_deliveries d WHERE character<>'journalist'")

    def run(*extra, succeeds=True):
        result = subprocess.run(command + list(extra), capture_output=True, text=True)
        assert (result.returncode == 0) == succeeds, result.stdout + result.stderr
        return result

    if not BACKLOG:
        sql("UPDATE classifier_sources SET discovery_version='stale' WHERE sport='ZZ_SELECTION'")
        result = run(succeeds=False)
        assert 'stale or unresolved' in result.stderr, result.stdout + result.stderr
        sql("UPDATE classifier_sources SET discovery_version=classifier_discovery_version(article_id,sport) WHERE sport='ZZ_SELECTION'")
        sql("UPDATE classifier_spectrum_deliveries SET reason='unresolved_target_identity' WHERE spectrum_id=" + spam_id + " AND character='journalist'")
        assert 'separate reconciliation' in run(succeeds=False).stderr
        sql("UPDATE classifier_spectrum_deliveries SET reason='daily_package_exceeds_context_budget' WHERE spectrum_id=" + spam_id + " AND character='journalist'")
        sql("UPDATE pipeline_work SET status='running',claim_token='00000000-0000-0000-0000-000000000123' WHERE sport='ZZ_SELECTION' AND stage='narratives'")
        assert 'drain before recovery' in run(succeeds=False).stderr
        sql("UPDATE pipeline_work SET status='failed',claim_token=NULL WHERE sport='ZZ_SELECTION' AND stage='narratives'")
        assert 'Unknown assignment key' in run('-v', 'assignment_scope=' + scope.replace('925001', '925002'), succeeds=False).stderr
    sql("DELETE FROM pipeline_work WHERE sport='ZZ_SELECTION' AND stage='narratives'")
    if BACKLOG:
        command += ['-v','backlog=true']
        sql("""UPDATE classifier_sources SET discovery_version='stale' WHERE article_id=925153;
        UPDATE classifier_sources SET source=jsonb_set(source,'{identity_candidates}','[]') WHERE article_id=925154;
        UPDATE classifier_spectrum_deliveries d SET status='pending',reason=NULL FROM classifier_spectra x
          WHERE x.id=d.spectrum_id AND x.sport='ZZ_SELECTION' AND x.article_id>=925130 AND d.character='journalist';
        UPDATE classifier_spectrum_deliveries d SET status='held',reason='model_reply_rejected' FROM classifier_spectra x
          WHERE x.id=d.spectrum_id AND x.article_id=925152 AND d.character='journalist';""")
    deferred = sql("SELECT deferred_versions::text FROM coverage_assignments WHERE sport='ZZ_SELECTION' AND stage='narratives'")
    before = sql("SELECT jsonb_agg(to_jsonb(d) ORDER BY spectrum_id) FROM classifier_spectrum_deliveries d WHERE character='journalist'")
    run()
    assert sql("SELECT jsonb_agg(to_jsonb(d) ORDER BY spectrum_id) FROM classifier_spectrum_deliveries d WHERE character='journalist'") == before, 'preview mutated deliveries'
    assert BACKLOG or sql("SELECT count(*) FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id WHERE x.sport='ZZ_SELECTION' AND d.character='journalist' AND d.status='held'") == '55', 'preview mutated deliveries'
    with tempfile.TemporaryDirectory() as directory:
        backup = Path(directory) / 'before.jsonl'
        run('-v', 'apply=true', '-v', 'backup_file=' + str(backup))
        rows = [json.loads(line) for line in backup.read_text().splitlines()]
        assert len([r for r in rows if r['table'] == 'coverage_assignments']) == 1
        assert len([r for r in rows if r['table'] == 'pipeline_work']) == 0, 'missing job should be recreated'
        assert len([r for r in rows if r['table'] == 'coverage_schedule']) == 1
        assert sql("SELECT enabled FROM coverage_schedule") == 't'
        assert len([r for r in rows if r['table'] == 'classifier_spectrum_deliveries']) == 55
        assert sql("SELECT status||'|'||attempts||'|'||input_version FROM pipeline_work WHERE sport='ZZ_SELECTION' AND stage='narratives'") == 'pending|0|headline-policy-v2:coverage:2098-01-02'
        assert sql("SELECT outcome||'|'||cardinality(spectrum_ids) FROM coverage_assignments WHERE sport='ZZ_SELECTION' AND stage='narratives'") == 'pending|55'
        assert sql("SELECT count(*) FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id WHERE x.sport='ZZ_SELECTION' AND d.character='journalist' AND d.status='pending'") == '40'
        assert sql("SELECT count(*) FROM classifier_spectrum_deliveries WHERE spectrum_id=" + spam_id + " AND character='journalist' AND reason='livestream_spam'") == '1'
        assert sql("SELECT jsonb_agg(to_jsonb(d) ORDER BY spectrum_id,character) FROM classifier_spectrum_deliveries d WHERE character<>'journalist'") == untouched, 'recovery modified another character'
        assert sql("SELECT deferred_versions::text FROM coverage_assignments WHERE sport='ZZ_SELECTION' AND stage='narratives'") == deferred
        if BACKLOG:
            assert sql("SELECT count(*) FROM classifier_spectrum_deliveries WHERE reason='model_reply_rejected'") == '1'
            assert sql("SELECT count(*) FROM classifier_spectrum_deliveries WHERE reason='stale_frozen_source'") == '1'
            assert sql("SELECT count(*) FROM classifier_spectrum_deliveries WHERE reason='unresolved_target_identity'") == '1'
            sql("UPDATE coverage_assignments SET input_version='unchanged' WHERE sport='ZZ_SELECTION' AND stage='narratives'; UPDATE pipeline_work SET input_version='unchanged' WHERE sport='ZZ_SELECTION' AND stage='narratives'")
        sql("UPDATE pipeline_work SET attempts=2 WHERE sport='ZZ_SELECTION' AND stage='narratives'")
        run('-v', 'apply=true', '-v', 'backup_file=' + str(Path(directory) / 'repeat.jsonl'))
        assert sql("SELECT attempts FROM pipeline_work WHERE sport='ZZ_SELECTION' AND stage='narratives'") == '2', 'repeated recovery reset attempts'
    print(('Backlog' if BACKLOG else 'Scoped')+' recovery: preview rollback, backups, strongest-40 selection, spam, unchanged other characters, stale/identity/claim fencing and repeat no-op passed')
finally:
    sql("""
    SET session_replication_role=replica;
    DELETE FROM pipeline_work WHERE sport='ZZ_SELECTION';
    DELETE FROM coverage_assignments WHERE edition_date='2098-01-02';
    UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL;
    DELETE FROM coverage_editions WHERE edition_date='2098-01-02';
    DELETE FROM classifier_spectrum_deliveries WHERE spectrum_id IN(SELECT id FROM classifier_spectra WHERE sport='ZZ_SELECTION');
    DELETE FROM classifier_spectra WHERE sport='ZZ_SELECTION';
    DELETE FROM classifier_source_heads WHERE sport='ZZ_SELECTION';
    DELETE FROM classifier_sources WHERE sport='ZZ_SELECTION';
    DELETE FROM harvester_query_provenance WHERE sport='ZZ_SELECTION';
    DELETE FROM news_article_entities WHERE sport='ZZ_SELECTION';
    DELETE FROM news_articles WHERE id BETWEEN 925100 AND 925155;
    DELETE FROM teams WHERE sport='ZZ_SELECTION';
    DELETE FROM sports WHERE id='ZZ_SELECTION';
    """)
