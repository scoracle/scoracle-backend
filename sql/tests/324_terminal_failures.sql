BEGIN;
SET LOCAL statement_timeout='20s';
TRUNCATE coverage_failures,pipeline_work,application_outbox,coverage_assignments,coverage_editions,coverage_drain_obligations;
UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL;
INSERT INTO sports(id,display_name,current_season) VALUES('ZZ_SELECTION','Selection fixture',2026);
INSERT INTO teams(id,sport,name) VALUES(925001,'ZZ_SELECTION','Cedar Policy');

CREATE TEMP TABLE policy_sources AS
SELECT i,925100+i AS article_id,
    CASE WHEN i IN (0,51) THEN 'Cedar Policy announces complete development number 1'
        WHEN i=52 THEN 'CEDAR POLICY announces complete development number 2'
        WHEN i=55 THEN '[WATCHLIVE] Cedar Policy football LIVE Stream'
        WHEN i=53 THEN 'Cedar Policy denies the previously reported agreement'
        ELSE 'Cedar Policy announces complete development number '||i END AS headline
FROM generate_series(0,55) i;
INSERT INTO news_articles(id,url,url_hash,title,source,published_at)
SELECT article_id,'https://example.invalid/policy/'||i,md5('policy-'||i),headline,'Wire',now()-interval '1 hour'
FROM policy_sources;
INSERT INTO classifier_sources(article_id,sport,input_hash,body_sha256,source)
SELECT article_id,'ZZ_SELECTION',md5(i::text),md5(''),jsonb_build_object(
    'article_id',article_id,'title',headline,'source','Wire','evidence_scope','headline',
    'published_at',now()-interval '1 hour','identity_candidates',
    CASE WHEN i=54 THEN '[]'::jsonb ELSE '[{"entity_type":"team","entity_id":925001}]'::jsonb END)
FROM policy_sources;
INSERT INTO classifier_spectra(source_id,article_id,sport,target,model,criteria_version,
    scrub_version,clean_text,clean_sha256,spectrum,gate,request_hash,receipt)
SELECT s.id,p.article_id,'ZZ_SELECTION','{"entity_type":"team","entity_id":925001,"sport":"ZZ_SELECTION"}',
    'fixture','classification-criteria-v10-availability','rss-headline-spectrum-v1',p.headline,md5(p.headline),
    CASE WHEN p.i IN (53,55) THEN '{"subject":100,"emotion":100,"movement":100,"availability":100}'::jsonb
    ELSE jsonb_build_object('subject',55+p.i%46,'emotion',100-p.i,'movement',60+p.i*7%40,'availability',85+p.i%15) END,
    '{"coverage":"complete","admitted":true,"characters":{"journalist":true,"influencer":true,"insider":true,"scout":true}}',
    md5(p.i::text),jsonb_build_object('request',jsonb_build_object('state',jsonb_build_object('article',jsonb_build_object('headline',p.headline))),
        'response',jsonb_build_object('model','fixture'))
FROM policy_sources p JOIN classifier_sources s USING(article_id);
INSERT INTO classifier_spectrum_deliveries(spectrum_id,character,status)
SELECT x.id,character,CASE WHEN p.i=0 THEN 'used' ELSE 'pending' END
FROM classifier_spectra x JOIN policy_sources p USING(article_id)
CROSS JOIN unnest(ARRAY['journalist','influencer','insider','scout']) character;
INSERT INTO coverage_editions(edition_date,cutoff_at,drain_deadline,frozen_at,status)
VALUES('2098-01-02',now()-interval '1 minute',now(),now(),'characters');
UPDATE coverage_schedule SET enabled=true,enrolled_at=now()-interval '1 day',phase='characters',edition_date='2098-01-02';
INSERT INTO coverage_assignments(edition_date,stage,entity_type,entity_id,sport,spectrum_ids)
SELECT '2098-01-02',stage,'team',925001,'ZZ_SELECTION',array_agg(x.id)
FROM classifier_spectra x JOIN policy_sources p USING(article_id)
CROSS JOIN unnest(ARRAY['narratives','vibe','transfers','rating']) stage
WHERE p.i<>0 GROUP BY stage;

UPDATE coverage_assignments SET outcome='completed',resolution='abstained';
UPDATE coverage_assignments SET outcome='failed',resolution='blocked',detail='model reply rejected'
WHERE sport='ZZ_SELECTION' AND stage='narratives';
UPDATE classifier_spectrum_deliveries d SET status='held',reason='unresolved_target_identity'
FROM classifier_spectra x JOIN policy_sources p USING(article_id)
WHERE d.spectrum_id=x.id AND d.character='journalist' AND p.i=54;
DELETE FROM pipeline_work WHERE stage='narratives' AND sport='ZZ_SELECTION';
SELECT coverage_tick();
DO $$ BEGIN
 IF (SELECT phase FROM coverage_schedule)<>'coverage' OR (SELECT status FROM coverage_editions)<>'partial'
 THEN RAISE EXCEPTION 'Consumed failed job blocked collection or published an incomplete edition'; END IF;
 IF (SELECT count(*) FROM coverage_failures WHERE kind='held_assignment' AND attempts IS NULL)<>1
 THEN RAISE EXCEPTION 'Consumed failure receipt missing or invented an attempt count'; END IF;
 IF EXISTS(SELECT 1 FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id
   JOIN policy_sources p USING(article_id) WHERE d.character='journalist' AND p.i BETWEEN 1 AND 55 AND d.status<>'held')
 THEN RAISE EXCEPTION 'Failed frozen obligations silently disappeared'; END IF;
 IF NOT EXISTS(SELECT 1 FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id
   JOIN policy_sources p USING(article_id) WHERE d.character='journalist' AND p.i=54 AND d.reason='unresolved_target_identity')
 THEN RAISE EXCEPTION 'Existing identity diagnosis was overwritten'; END IF;
 IF EXISTS(SELECT 1 FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id
   WHERE x.sport='ZZ_SELECTION' AND d.character<>'journalist' AND d.status<>'pending' AND x.article_id<>925100)
 THEN RAISE EXCEPTION 'Other characters were changed'; END IF;
END $$;
ROLLBACK;
