#!/usr/bin/env python3
"""Check real headline-worker watchdog SQL in an empty disposable database."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

url = os.environ['TEST_DATABASE_URL']
def sql(statement):
    return subprocess.check_output(['psql',url,'-X','-v','ON_ERROR_STOP=1','-Atc',statement],text=True).strip()
assert sql('SELECT current_database()').startswith('classifier_test')
assert sql('SELECT count(*) FROM classifier_sources') == '0', 'empty migrated fixture database required'
with tempfile.TemporaryDirectory(prefix='headline-watchdog-') as directory:
    target=Path(directory)/'scripts/hosting/cron-watchdog.sh'
    target.parent.mkdir(parents=True)
    shutil.copy2(Path(__file__).with_name('cron-watchdog.sh'),target)
    def run(mode,code,expected,stages='classifier,narratives'):
        result=subprocess.run(['bash',str(target),url],env={**os.environ,'WATCHDOG_MODE':mode,'COGNITION_STAGES':stages},capture_output=True,text=True)
        assert result.returncode==code and expected in result.stdout,result.stdout+result.stderr
        return result.stdout
    try:
        sql("INSERT INTO news_articles(id,url_hash,url,title,fetched_at) VALUES(9991041,'watchdog-fixture','https://example.invalid/watchdog','Fixture',now()); INSERT INTO pipeline_work(stage,entity_type,entity_id,sport) VALUES('classifier','article',9991041,'NBA')")
        assert 'drain_alive' not in run('paused',0,'all checks OK')
        run('classifier',1,'ALARM drain_alive:classifier:')
        sql("INSERT INTO classifier_sources(article_id,sport,input_hash,body_sha256,source) VALUES(9991041,'NBA','watchdog','watchdog','{}'); INSERT INTO classifier_measurements(source_id,article_id,sport,request_hash,status,receipt) SELECT id,9991041,'NBA','failed','error','{}' FROM classifier_sources WHERE article_id=9991041")
        run('classifier',1,'ALARM drain_alive:classifier:')
        run('classifier',0,'all checks OK',stages='')
        sql("UPDATE pipeline_work SET status='running' WHERE entity_id=9991041; DELETE FROM pipeline_work WHERE entity_id=9991041; INSERT INTO pipeline_work(stage,entity_type,entity_id,sport) VALUES('classifier','article',9991041,'NBA'),('narratives','team',9991041,'NBA')")
        masked=run('classifier',1,'ALARM drain_alive:narratives:')
        assert 'OK drain_alive:classifier:' in masked
        run('classifier',0,'OK drain_alive:classifier:',stages=' Classifier ')
        sql("UPDATE pipeline_work SET status='failed',attempts=5 WHERE entity_id=9991041 AND stage='classifier'")
        run('classifier',1,'INFO dead_letters: 1 active-stage rows at attempt cap; 0 missing failure records')
        sql("DELETE FROM pipeline_work_failures WHERE entity_id=9991041 AND stage='classifier'")
        run('classifier',1,'ALARM dead_letters: 1 active-stage rows at attempt cap; 1 missing failure records')
        run('paused',0,'all checks OK')
        sql("INSERT INTO coverage_editions(edition_date,cutoff_at,drain_deadline,status,finished_at,reason) VALUES('2099-02-01',now(),now()+interval '1 minute','partial',now(),'completed_with_failures')")
        run('paused',1,'ALARM last_character_edition: 2099-02-01 partial')
        sql("INSERT INTO coverage_editions(edition_date,cutoff_at,drain_deadline,status,finished_at) VALUES('2099-02-02',now()+interval '1 day',now()+interval '2 days','complete',now())")
        run('paused',0,'OK last_character_edition: 2099-02-02 complete')
    finally:
        sql("DELETE FROM coverage_editions WHERE edition_date IN ('2099-02-01','2099-02-02')")
        sql("DELETE FROM pipeline_work WHERE entity_id=9991041; DELETE FROM news_articles WHERE id=9991041; DELETE FROM pipeline_runs WHERE job='watchdog'; DELETE FROM pipeline_stage_progress WHERE stage IN ('classifier','narratives'); DELETE FROM pipeline_work_failures WHERE entity_id=9991041 AND stage='classifier'")
print('Paused backlog, isolated stage progress, error receipts and dead letters verified.')
