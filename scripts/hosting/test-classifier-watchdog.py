#!/usr/bin/env python3
"""Check real headline-worker watchdog SQL in an empty disposable database."""
import json
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
    shutil.copy2(Path(__file__).with_name('watchdog.sql'),target.parent/'watchdog.sql')
    def live():
        result = subprocess.run(['bash',str(target),'--report',url],check=True,capture_output=True,text=True,
            env={**os.environ,'WATCHDOG_MODE':'classifier','COGNITION_STAGES':'classifier,narratives','WATCHDOG_ALERT_URL':''})
        return json.loads(result.stdout)
    def run(mode,code,expected,stages='classifier,narratives'):
        result=subprocess.run(['bash',str(target),url],env={**os.environ,'WATCHDOG_ALERT_URL':'','WATCHDOG_MODE':mode,'COGNITION_STAGES':stages},capture_output=True,text=True)
        assert result.returncode==code and expected in result.stdout,result.stdout+result.stderr
        return result.stdout
    try:
        sql("INSERT INTO news_articles(id,url_hash,url,title,fetched_at) VALUES(9991041,'watchdog-fixture','https://example.invalid/watchdog','Fixture',now()); INSERT INTO pipeline_work(stage,entity_type,entity_id,sport) VALUES('classifier','article',9991041,'NBA')")
        sql("""INSERT INTO character_output_flags(stage,sport,claim_token,reason,recorded_at)
            VALUES('momentum','NBA',gen_random_uuid(),'missing headline',now()-interval '3 days')""")
        sql("""INSERT INTO character_output_flags(stage,sport,claim_token,reason)
            VALUES ('momentum','NBA',gen_random_uuid(),'blank blurb')""")
        initial = live()
        assert initial['classifier']['queue_now'] == {'ready':1,'waiting':0,'running':0,'parked':0}
        assert initial['characters_24h']['momentum']['parser_reasons'] == {'blank blurb':1}
        assert [f['reason'] for f in initial['recent_parser_flags']] == ['blank blurb']
        assert any(c['name']=='drain_alive:classifier' and c['status']=='ALARM' for c in initial['checks'])
        assert sql("SELECT count(*) FROM pipeline_runs WHERE job='watchdog'") == '0'
        assert sql("SELECT count(*) FROM character_output_flags WHERE watchdog_run_id IS NOT NULL") == '0'
        paused = run('paused',0,'all checks OK')
        assert 'drain_alive' not in paused
        report = json.loads(paused.split('INFO character_outputs: ')[1].splitlines()[0])
        assert set(report) == {'momentum','vibe','narratives','sigil','transfers','rating'}
        assert report['momentum']['reasons'] == {'missing headline':1,'blank blurb':1}
        assert report['momentum']['new_flagged_claims'] == 2
        run('classifier',1,'ALARM drain_alive:classifier:')
        sql("INSERT INTO classifier_sources(article_id,sport,input_hash,body_sha256,source) VALUES(9991041,'NBA','watchdog','watchdog','{}'); INSERT INTO classifier_measurements(source_id,article_id,sport,request_hash,status,receipt) SELECT id,9991041,'NBA','failed','error','{}' FROM classifier_sources WHERE article_id=9991041")
        sql("""INSERT INTO classifier_measurements(source_id,article_id,sport,request_hash,status,receipt,created_at)
            SELECT id,9991041,'NBA','old','error','{"error":"old error"}',now()-interval '25 hours'
            FROM classifier_sources WHERE article_id=9991041;
            INSERT INTO classifier_spectra(source_id,article_id,sport,target,model,criteria_version,scrub_version,
                clean_text,clean_sha256,spectrum,gate,request_hash,receipt)
            SELECT id,9991041,'NBA','{}','fixture','fixture','fixture','Fixture','fixture','{}',
                '{"coverage":"complete","admitted":false}','fixture',
                '{"request":{"state":{"body":"Fixture"}},"response":{"model":"fixture"}}'
            FROM classifier_sources WHERE article_id=9991041""")
        activity = live()['classifier']['activity_24h']
        assert activity['failed_measurements']==1 and activity['top_measurement_errors'][0]['attempts']==1
        assert activity['target_decisions']==activity['articles_classified']==activity['declined']==1
        assert activity['admitted']==activity['exhausted_jobs']==0
        run('classifier',1,'ALARM drain_alive:classifier:')
        run('classifier',0,'all checks OK',stages='')
        sql("UPDATE pipeline_work SET status='running' WHERE entity_id=9991041; DELETE FROM pipeline_work WHERE entity_id=9991041; INSERT INTO pipeline_work(stage,entity_type,entity_id,sport) VALUES('classifier','article',9991041,'NBA'),('narratives','team',9991041,'NBA')")
        masked=run('classifier',1,'ALARM drain_alive:narratives:')
        assert 'OK drain_alive:classifier:' in masked
        run('classifier',0,'OK drain_alive:classifier:',stages=' Classifier ')
        sql("UPDATE pipeline_work SET status='failed',attempts=5,last_error='fixture timeout' WHERE entity_id=9991041 AND stage='classifier'")
        run('classifier',1,'INFO dead_letters: 1 active-stage rows at attempt cap; 0 missing failure records')
        parked = live()['classifier']
        assert parked['queue_now']['parked']==1 and parked['queue_now']['ready']==0
        assert parked['recent_queue_errors'][0]['reason']=='fixture timeout'
        assert parked['activity_24h']['exhausted_jobs']==1
        assert parked['activity_24h']['recent_exhausted_jobs'][0]['article_id']==9991041
        sql("DELETE FROM pipeline_work_failures WHERE entity_id=9991041 AND stage='classifier'")
        run('classifier',1,'ALARM dead_letters: 1 active-stage rows at attempt cap; 1 missing failure records')
        run('paused',0,'all checks OK')
        sql("INSERT INTO coverage_editions(edition_date,cutoff_at,drain_deadline,status,finished_at,reason) VALUES('2099-02-01',now(),now()+interval '1 minute','partial',now(),'completed_with_failures')")
        run('paused',1,'ALARM last_character_edition: 2099-02-01 partial')
        sql("INSERT INTO coverage_editions(edition_date,cutoff_at,drain_deadline,status,finished_at) VALUES('2099-02-02',now()+interval '1 day',now()+interval '2 days','complete',now())")
        run('paused',0,'OK last_character_edition: 2099-02-02 complete')
    finally:
        sql("DELETE FROM character_output_flags WHERE reason IN ('missing headline','blank blurb') AND stage='momentum' AND sport='NBA'")
        sql("DELETE FROM coverage_editions WHERE edition_date IN ('2099-02-01','2099-02-02')")
        sql("DELETE FROM pipeline_work WHERE entity_id=9991041; DELETE FROM news_articles WHERE id=9991041; DELETE FROM pipeline_runs WHERE job='watchdog'; DELETE FROM pipeline_stage_progress WHERE stage IN ('classifier','narratives'); DELETE FROM pipeline_work_failures WHERE entity_id=9991041 AND stage='classifier'")
print('Paused backlog, isolated stage progress, error receipts and dead letters verified.')
