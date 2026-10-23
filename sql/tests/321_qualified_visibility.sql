BEGIN;
DO $$ BEGIN IF current_database() NOT LIKE 'classifier_test_coverage%' THEN RAISE EXCEPTION 'disposable database required'; END IF; END $$;
SET LOCAL statement_timeout='10s';
UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL,
 timezone='America/Detroit',collection_cutoff='22:00';
TRUNCATE coverage_failures,pipeline_work,application_outbox,coverage_assignments,coverage_editions,coverage_drain_obligations;
INSERT INTO sports(id,display_name,current_season) VALUES('ZZ_VISIBLE','Visibility test',2026) ON CONFLICT DO NOTHING;
INSERT INTO coverage_editions(edition_date,cutoff_at,drain_deadline,status)
VALUES('2026-10-20','2026-10-20T00:00:00-04:00','2026-10-20T00:01:00-04:00','complete'),
 ('2026-10-21','2026-10-21T00:00:00-04:00','2026-10-21T00:01:00-04:00','partial'),
 ('2026-10-22','2026-10-22T00:00:00-04:00','2026-10-22T00:01:00-04:00','characters');
INSERT INTO momentum_summaries(entity_type,entity_id,sport,season,blurb,model_version,prompt_version,reporting_date)
VALUES('team',1,'ZZ_VISIBLE',2026,'Earlier qualified card.','fixture','fixture','2026-10-19'),
 ('team',2,'ZZ_VISIBLE',2026,'Rejected candidate.','fixture','fixture','2026-10-20'),
 ('team',3,'ZZ_VISIBLE',2026,'Hidden new candidate.','fixture','fixture','2026-10-21');
UPDATE momentum_summaries SET candidate_edition=reporting_date+1 WHERE sport='ZZ_VISIBLE';
INSERT INTO momentum_summaries(entity_type,entity_id,sport,season,blurb,model_version,prompt_version,reporting_date)
VALUES('team',4,'ZZ_VISIBLE',2026,'Existing legacy surface.','fixture','fixture','2026-10-18');
UPDATE coverage_publication SET edition_date=NULL;
UPDATE coverage_schedule SET enabled=true,enrolled_at='2026-01-01Z';
DO $$ BEGIN
 IF (SELECT array_agg(entity_id ORDER BY entity_id) FROM qualified_momentum_summaries WHERE sport='ZZ_VISIBLE') <> ARRAY[4] THEN RAISE EXCEPTION 'first cutover lost legacy surface or leaked candidate'; END IF;
END $$;

DO $$ BEGIN
 IF coverage_collection_deadline('2026-03-08T21:59:00-04:00') <> '2026-03-08T22:00:00-04:00'::timestamptz
 OR coverage_collection_deadline('2026-11-01T21:59:00-05:00') <> '2026-11-01T22:00:00-05:00'::timestamptz
 THEN RAISE EXCEPTION 'RSS deadline ignored Detroit DST'; END IF;
END $$;
UPDATE coverage_publication SET edition_date='2026-10-20';
DO $$ BEGIN
 IF (SELECT array_agg(entity_id ORDER BY entity_id) FROM qualified_momentum_summaries WHERE sport='ZZ_VISIBLE') <> ARRAY[1] THEN RAISE EXCEPTION 'candidate leaked before publication'; END IF;
END $$;
INSERT INTO coverage_assignments(edition_date,stage,entity_type,entity_id,sport,outcome,resolution)
VALUES('2026-10-22','momentum','team',1,'ZZ_VISIBLE','completed','no_new_evidence');
UPDATE coverage_editions SET status='complete' WHERE edition_date='2026-10-22';
UPDATE coverage_publication SET edition_date='2026-10-22',revision=revision+1;
DO $$ BEGIN
 IF (SELECT array_agg(entity_id ORDER BY entity_id) FROM qualified_momentum_summaries WHERE sport='ZZ_VISIBLE') <> ARRAY[1,3] THEN RAISE EXCEPTION 'qualified switch or carry failed'; END IF;
 IF (SELECT reporting_date FROM qualified_momentum_summaries WHERE sport='ZZ_VISIBLE' AND entity_id=1) <> '2026-10-19'::date THEN RAISE EXCEPTION 'carry relabelled old evidence'; END IF;
END $$;
UPDATE coverage_assignments SET resolution='abstained' WHERE sport='ZZ_VISIBLE';
DO $$ BEGIN IF EXISTS(SELECT 1 FROM qualified_momentum_summaries WHERE sport='ZZ_VISIBLE' AND entity_id=1) THEN RAISE EXCEPTION 'abstention resurrected stale card'; END IF; END $$;
SELECT set_config('scoracle.reporting_date','2026-10-19',true);
DO $$ BEGIN IF NOT EXISTS(SELECT 1 FROM qualified_momentum_summaries WHERE sport='ZZ_VISIBLE' AND entity_id=1) THEN RAISE EXCEPTION 'qualified history lost'; END IF; END $$;
SELECT set_config('scoracle.reporting_date','',true);
DO $$ DECLARE before_revision bigint; BEGIN
 SELECT revision INTO before_revision FROM coverage_publication;
 DELETE FROM momentum_summaries WHERE sport='ZZ_VISIBLE' AND entity_id=3;
 IF (SELECT revision FROM coverage_publication)<=before_revision THEN RAISE EXCEPTION 'withdrawal did not invalidate caches'; END IF;
 IF EXISTS(SELECT 1 FROM qualified_momentum_summaries WHERE sport='ZZ_VISIBLE' AND entity_id=3) THEN RAISE EXCEPTION 'withdrawn product still visible'; END IF;
END $$;
ROLLBACK;
