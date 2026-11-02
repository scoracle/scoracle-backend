BEGIN;
DO $$ BEGIN IF current_database() NOT LIKE 'classifier_test_coverage%' THEN RAISE EXCEPTION 'disposable database required'; END IF; END $$;
SET LOCAL statement_timeout='10s';
UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL;
TRUNCATE coverage_failures,pipeline_work,application_outbox,coverage_assignments,coverage_editions,coverage_drain_obligations;
INSERT INTO sports(id,display_name,current_season) VALUES('ZZ_PROGRESS','Progressive test',2026) ON CONFLICT DO NOTHING;
INSERT INTO coverage_editions(edition_date,cutoff_at,drain_deadline,status) VALUES
 ('2026-10-21','2026-10-21Z','2026-10-21T00:01Z','partial'),
 ('2026-10-22','2026-10-22Z','2026-10-22T00:01Z','characters');
INSERT INTO news_summaries(entity_type,entity_id,sport,trigger_type,body,reporting_date)
VALUES('team',1,'ZZ_PROGRESS','manual','Accepted recap','2026-10-20');
INSERT INTO vibe_scores(entity_type,entity_id,sport,trigger_type,model_version,prompt_version,sentiment,reporting_date)
VALUES('team',1,'ZZ_PROGRESS','manual','test','test',25,'2026-10-20');
INSERT INTO insider_scores(entity_type,entity_id,sport,score,prompt_version,reporting_date)
VALUES('team',1,'ZZ_PROGRESS',50,'test','2026-10-20');
UPDATE news_summaries SET candidate_edition='2026-10-21' WHERE sport='ZZ_PROGRESS';
UPDATE vibe_scores SET candidate_edition='2026-10-21' WHERE sport='ZZ_PROGRESS';
UPDATE insider_scores SET candidate_edition='2026-10-21' WHERE sport='ZZ_PROGRESS';
UPDATE coverage_publication SET edition_date=NULL;
UPDATE coverage_schedule SET enabled=true,enrolled_at='2026-01-01Z';
INSERT INTO coverage_assignments(edition_date,stage,entity_type,entity_id,sport)
SELECT '2026-10-21',stage,'team',1,'ZZ_PROGRESS' FROM unnest(ARRAY['narratives','vibe','transfers']) stage;
DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM qualified_news_summaries WHERE sport='ZZ_PROGRESS') OR
 EXISTS(SELECT 1 FROM qualified_vibe_scores WHERE sport='ZZ_PROGRESS') OR
 EXISTS(SELECT 1 FROM qualified_insider_scores WHERE sport='ZZ_PROGRESS') THEN RAISE EXCEPTION 'pending candidate leaked'; END IF;
END $$;
UPDATE coverage_assignments SET outcome='failed',resolution='blocked' WHERE sport='ZZ_PROGRESS';
DO $$ BEGIN IF coverage_character_visible('2026-10-21','narratives','team',1,'ZZ_PROGRESS') THEN RAISE EXCEPTION 'failed candidate leaked'; END IF; END $$;
DO $$ DECLARE before_revision bigint; BEGIN
 SELECT revision INTO before_revision FROM coverage_publication;
 UPDATE coverage_assignments SET outcome='completed',resolution='candidate' WHERE sport='ZZ_PROGRESS';
 IF (SELECT revision FROM coverage_publication)<=before_revision THEN RAISE EXCEPTION 'completion did not invalidate cache'; END IF;
 IF (SELECT count(*) FROM qualified_news_summaries WHERE sport='ZZ_PROGRESS')<>1 OR
 (SELECT count(*) FROM qualified_vibe_scores WHERE sport='ZZ_PROGRESS')<>1 OR
 (SELECT count(*) FROM qualified_insider_scores WHERE sport='ZZ_PROGRESS')<>1 THEN RAISE EXCEPTION 'independent completed cards hidden'; END IF;
END $$;
INSERT INTO coverage_assignments(edition_date,stage,entity_type,entity_id,sport,outcome,resolution)
SELECT '2026-10-22',stage,'team',1,'ZZ_PROGRESS','completed','no_new_evidence' FROM unnest(ARRAY['narratives','vibe','transfers']) stage;
DO $$ BEGIN IF NOT coverage_character_visible('2026-10-21','narratives','team',1,'ZZ_PROGRESS') THEN RAISE EXCEPTION 'no-news carry lost'; END IF; END $$;
UPDATE coverage_assignments SET resolution='abstained' WHERE edition_date='2026-10-22' AND sport='ZZ_PROGRESS';
DO $$ BEGIN IF EXISTS(SELECT 1 FROM qualified_news_summaries WHERE sport='ZZ_PROGRESS') THEN RAISE EXCEPTION 'abstention resurrected previous card'; END IF; END $$;
SELECT set_config('scoracle.reporting_date','2026-10-20',true);
DO $$ BEGIN IF NOT EXISTS(SELECT 1 FROM qualified_news_summaries WHERE sport='ZZ_PROGRESS') THEN RAISE EXCEPTION 'dated history lost'; END IF; END $$;
DELETE FROM news_summaries WHERE sport='ZZ_PROGRESS';
DO $$ BEGIN IF EXISTS(SELECT 1 FROM qualified_news_summaries WHERE sport='ZZ_PROGRESS') THEN RAISE EXCEPTION 'deleted card visible'; END IF; END $$;
ROLLBACK;
