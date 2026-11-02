BEGIN;
DO $$ BEGIN IF current_database() NOT LIKE 'classifier_test_coverage%' THEN RAISE EXCEPTION 'disposable database required'; END IF; END $$;
SET LOCAL statement_timeout='10s';
UPDATE coverage_schedule SET enabled=false,phase='coverage',edition_date=NULL;
TRUNCATE coverage_failures,pipeline_work,application_outbox,coverage_assignments,coverage_editions,coverage_drain_obligations;
INSERT INTO coverage_editions(edition_date,cutoff_at,drain_deadline,status) VALUES
 ('2098-01-02','2098-01-02Z','2098-01-02T00:01Z','characters');
UPDATE coverage_schedule SET enabled=true,enrolled_at='2098-01-01Z',phase='characters',edition_date='2098-01-02',
 required_stages=ARRAY['narratives','vibe','transfers'];
INSERT INTO coverage_assignments(edition_date,stage,entity_type,entity_id,sport)
SELECT '2098-01-02',stage,'team',1,'ZZ_REQUIRED' FROM unnest(ARRAY['narratives','vibe','transfers','rating','momentum','sigil']) stage;
DO $$ BEGIN
 IF NOT coverage_stage_open('narratives') OR coverage_stage_open('rating') THEN RAISE EXCEPTION 'required stage fence failed'; END IF;
 IF coverage_tick('2098-01-02T01:00Z')<>'characters' THEN RAISE EXCEPTION 'required pending work was skipped'; END IF;
END $$;
UPDATE coverage_assignments SET outcome='completed',resolution='candidate' WHERE stage IN ('narratives','vibe','transfers');
DO $$ BEGIN
 IF coverage_tick('2098-01-02T01:01Z')<>'coverage' THEN RAISE EXCEPTION 'paused characters blocked collection'; END IF;
 IF (SELECT status FROM coverage_editions WHERE edition_date='2098-01-02')<>'partial' THEN RAISE EXCEPTION 'unfinished edition falsely marked complete'; END IF;
 IF (SELECT count(*) FROM coverage_assignments WHERE stage IN ('rating','momentum','sigil') AND outcome='pending' AND resolution IS NULL)<>3 THEN RAISE EXCEPTION 'paused assignment records changed'; END IF;
 IF (SELECT edition_date FROM coverage_publication)='2098-01-02' THEN RAISE EXCEPTION 'whole-edition pointer published unfinished roles'; END IF;
 IF NOT coverage_collection_open('2098-01-02T01:02Z') THEN RAISE EXCEPTION 'collection did not reopen'; END IF;
END $$;
ROLLBACK;
