-- Run only against the disposable coverage database; all fixtures roll back.
BEGIN;
DO $$ BEGIN
    IF current_database() NOT LIKE 'classifier_test_coverage%' THEN
        RAISE EXCEPTION 'requires a disposable classifier_test_coverage database';
    END IF;
END $$;
SET LOCAL statement_timeout='10s';
UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL;
TRUNCATE pipeline_work,application_outbox,coverage_assignments,coverage_editions;
INSERT INTO sports(id,display_name,current_season) VALUES ('ZZ_FREEZE_A','Freeze A',2026),('ZZ_FREEZE_B','Freeze B',2026);
INSERT INTO news_articles(id,url,url_hash,title,source,published_at)
VALUES (9318001,'https://example.invalid/freeze',md5('freeze'),'Freeze fixture','Fixture Wire',now());
INSERT INTO classifier_sources(article_id,sport,input_hash,body_sha256,source)
SELECT 9318001,sport,'freeze','freeze','{"evidence_scope":"headline","title":"Freeze fixture"}'
FROM unnest(ARRAY['ZZ_FREEZE_A','ZZ_FREEZE_B']) sport;
INSERT INTO classifier_spectra(source_id,article_id,sport,target,model,criteria_version,
    scrub_version,clean_text,clean_sha256,spectrum,gate,request_hash,receipt)
SELECT s.id,s.article_id,s.sport,jsonb_build_object('entity_type',kind,'entity_id',n),
    'fixture','classification-criteria-v9-headlines','fixture','Freeze fixture','fixture','{}',
    '{"coverage":"complete","characters":{"journalist":true,"insider":true,"influencer":true}}',
    n::text||kind,
    '{"request":{"state":{"article":{"headline":"Freeze fixture"}}},"response":{"model":"fixture"}}'
FROM classifier_sources s CROSS JOIN generate_series(1,500) n
CROSS JOIN unnest(ARRAY['team','player']) kind WHERE s.article_id=9318001;
INSERT INTO classifier_spectrum_deliveries(spectrum_id,character,status)
SELECT x.id,c,CASE WHEN (x.target->>'entity_id')::int%3=0 THEN 'used' ELSE 'pending' END
FROM classifier_spectra x CROSS JOIN unnest(ARRAY['journalist','insider','influencer']) c
WHERE x.article_id=9318001;
-- A subject with no pending evidence still needs all six empty assignments.
INSERT INTO pipeline_work(stage,entity_type,entity_id,sport,input_version)
SELECT 'rating',target->>'entity_type',(target->>'entity_id')::bigint,sport,'measurements'
FROM classifier_spectra WHERE article_id=9318001 ON CONFLICT DO NOTHING;
INSERT INTO pipeline_work(stage,entity_type,entity_id,sport,input_version)
VALUES ('rating','team',999,'ZZ_FREEZE_A','no-news');
UPDATE coverage_schedule SET enabled=true,enrolled_at=clock_timestamp()-interval '1 day',
    phase='draining',edition_date=current_date;
INSERT INTO coverage_editions(edition_date,cutoff_at,drain_deadline,status)
VALUES(current_date,clock_timestamp()-interval '16 minutes',clock_timestamp()-interval '1 minute','draining');
SELECT coverage_tick();
DO $$ BEGIN
    IF (SELECT count(*) FROM coverage_assignments) <> 12006 THEN
        RAISE EXCEPTION 'missing assignments at production scale';
    END IF;
    IF EXISTS (
        SELECT 1 FROM coverage_assignments a WHERE a.spectrum_ids IS DISTINCT FROM
            COALESCE((SELECT array_agg(x.id ORDER BY x.id)
                FROM classifier_spectrum_deliveries d JOIN classifier_spectra x ON x.id=d.spectrum_id
                WHERE d.status='pending' AND x.sport=a.sport
                AND x.target->>'entity_type'=a.entity_type AND x.target->>'entity_id'=a.entity_id::text
                AND d.character=CASE a.stage WHEN 'narratives' THEN 'journalist'
                    WHEN 'vibe' THEN 'influencer' WHEN 'rating' THEN 'scout'
                    WHEN 'transfers' THEN 'insider' END),'{}'::bigint[])
    ) THEN RAISE EXCEPTION 'frozen evidence differs from original selector'; END IF;
END $$;
ROLLBACK;
