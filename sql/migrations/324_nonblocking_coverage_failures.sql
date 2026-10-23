BEGIN;
-- Terminal failures remain reportable without holding acquisition or publishing a partial edition.
CREATE TABLE public.coverage_failures (
    edition_date date NOT NULL REFERENCES public.coverage_editions(edition_date) ON DELETE CASCADE,
    kind text NOT NULL CHECK(kind IN ('decision_exhausted','character_exhausted','held_assignment','dependency_failed','handoff_exhausted')),
    stage text NOT NULL,
    entity_type text NOT NULL,
    entity_id bigint NOT NULL,
    sport text NOT NULL,
    input_version text,
    attempts integer,
    error text NOT NULL,
    receipt jsonb NOT NULL,
    failed_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY(edition_date,kind,stage,entity_type,entity_id,sport)
);
COMMENT ON TABLE public.coverage_failures IS 'Durable terminal edition failures, including consumed jobs, source holds, failed dependencies and handoffs. Partial editions never change the qualified publication pointer.';

COMMENT ON COLUMN public.coverage_drain_obligations.settled_at IS 'Terminal drain time: claim-fenced completion or an archived decision_exhausted receipt in coverage_failures.';

-- Retired packet fan-out must obey the same retry budget as Go and Rust producers.
DO $$ DECLARE definition text; BEGIN
 SELECT pg_get_functiondef('public.enqueue_voices_on_packet()'::regprocedure) INTO definition;
 definition:=replace(definition,E'\n       OR public.pipeline_work.status = ''failed''','');
 EXECUTE definition;
END $$;

CREATE OR REPLACE FUNCTION coverage_tick(at_time timestamptz DEFAULT clock_timestamp()) RETURNS text
LANGUAGE plpgsql AS $$
DECLARE s coverage_schedule; e coverage_editions; local_day date; boundary timestamptz; changed bigint;
BEGIN
 PERFORM pg_advisory_xact_lock(1935896421,1);
 SELECT * INTO s FROM coverage_schedule FOR UPDATE;
 IF NOT s.enabled THEN RETURN 'disabled'; END IF;
 UPDATE coverage_schedule SET last_tick_at=at_time;
 IF s.phase='coverage' THEN
  local_day := (at_time AT TIME ZONE s.timezone)::date;
  boundary := (local_day+s.collection_cutoff) AT TIME ZONE s.timezone;
  IF boundary>at_time THEN local_day:=local_day-1; boundary:=(local_day+s.collection_cutoff) AT TIME ZONE s.timezone; END IF;
  -- After an overrun, collect until the next normal boundary, without a stack of missed editions.
  IF boundary<s.enrolled_at OR EXISTS(SELECT 1 FROM coverage_editions WHERE edition_date=local_day+1)
   OR EXISTS(SELECT 1 FROM coverage_editions WHERE finished_at>=boundary) THEN RETURN 'coverage'; END IF;
  INSERT INTO coverage_editions(edition_date,cutoff_at,drain_deadline,collection_closed_at,status)
  VALUES(local_day+1,(local_day+1)::timestamp AT TIME ZONE s.timezone,
    (local_day+1)::timestamp AT TIME ZONE s.timezone+interval '1 microsecond',boundary,'draining');
  INSERT INTO coverage_drain_obligations(edition_date,entity_type,entity_id,sport,input_version)
  SELECT local_day+1,entity_type,entity_id,sport,input_version FROM pipeline_work WHERE stage='classifier';
  UPDATE coverage_schedule SET phase='draining',edition_date=local_day+1,updated_at=at_time;
  RETURN 'draining';
 END IF;
 SELECT * INTO STRICT e FROM coverage_editions WHERE edition_date=s.edition_date FOR UPDATE;
 UPDATE coverage_editions SET missed_boundaries=GREATEST(0,(at_time AT TIME ZONE s.timezone)::date-e.edition_date)
 WHERE edition_date=e.edition_date;
 IF s.phase='draining' THEN
  INSERT INTO coverage_failures(edition_date,kind,stage,entity_type,entity_id,sport,input_version,attempts,error,receipt)
  SELECT d.edition_date,'decision_exhausted',w.stage,w.entity_type,w.entity_id,w.sport,w.input_version,w.attempts,
    COALESCE(w.last_error,'decision attempts exhausted'),to_jsonb(w)
  FROM coverage_drain_obligations d JOIN pipeline_work w USING(entity_type,entity_id,sport)
  WHERE d.edition_date=e.edition_date AND d.settled_at IS NULL AND w.stage='classifier'
    AND w.input_version IS NOT DISTINCT FROM d.input_version AND w.status='failed' AND w.attempts>=5
  ON CONFLICT DO NOTHING;
  UPDATE coverage_drain_obligations d SET settled_at=at_time
  WHERE d.edition_date=e.edition_date AND d.settled_at IS NULL AND EXISTS(
    SELECT 1 FROM coverage_failures f WHERE f.edition_date=d.edition_date AND f.kind='decision_exhausted'
      AND f.entity_type=d.entity_type AND f.entity_id=d.entity_id AND f.sport=d.sport
      AND f.input_version IS NOT DISTINCT FROM d.input_version);
  IF at_time<e.cutoff_at OR EXISTS(SELECT 1 FROM coverage_drain_obligations
     WHERE edition_date=e.edition_date AND settled_at IS NULL) THEN RETURN 'draining'; END IF;
        INSERT INTO public.pipeline_work(stage,entity_type,entity_id,sport,input_version)
        SELECT DISTINCT downstream,w.entity_type,w.entity_id,w.sport,
            CASE WHEN downstream='transfers' THEN 'classifier-spectrum-v1:daily:'||e.edition_date
                 WHEN downstream='rating' THEN 'daily-rating:'||e.edition_date
                 ELSE 'coverage:'||e.edition_date END
        FROM public.pipeline_work w CROSS JOIN
            unnest(ARRAY['narratives','vibe','rating','transfers','momentum','sigil']) downstream
        WHERE public.coverage_is_character(w.stage) AND (w.entity_type IN ('team','player') OR (w.entity_type='person' AND downstream IN ('narratives','vibe','transfers')))
        ON CONFLICT(stage,entity_type,entity_id,sport) DO NOTHING;
        WITH pending AS MATERIALIZED (
            SELECT x.sport,x.target->>'entity_type' AS entity_type,
                x.target->>'entity_id' AS entity_id,d.character,
                array_agg(x.id ORDER BY x.id) AS spectrum_ids
            FROM public.classifier_spectrum_deliveries d
            JOIN public.classifier_spectra x ON x.id=d.spectrum_id
            WHERE d.status='pending'
            GROUP BY x.sport,x.target->>'entity_type',x.target->>'entity_id',d.character
        )
        INSERT INTO public.coverage_assignments(edition_date,stage,entity_type,entity_id,sport,input_version,spectrum_ids)
        SELECT e.edition_date,w.stage,w.entity_type,w.entity_id,w.sport,w.input_version,
            COALESCE(d.spectrum_ids,'{}'::bigint[])
        FROM public.pipeline_work w LEFT JOIN pending d
          ON d.sport=w.sport AND d.entity_type=w.entity_type AND d.entity_id=w.entity_id::text
          AND d.character=CASE w.stage WHEN 'narratives' THEN 'journalist'
              WHEN 'vibe' THEN 'influencer' WHEN 'rating' THEN 'scout'
              WHEN 'transfers' THEN 'insider' END
        WHERE public.coverage_is_character(w.stage);

  -- Empty news has an explicit dated outcome, without a queue claim or model call.
  UPDATE coverage_assignments a SET outcome='completed',resolution='no_new_evidence',
      detail='no pending headline evidence',finished_at=at_time
  FROM pipeline_work w WHERE a.edition_date=e.edition_date AND a.stage IN ('narratives','vibe','transfers')
   AND cardinality(a.spectrum_ids)=0 AND a.stage=w.stage AND a.entity_type=w.entity_type
   AND a.entity_id=w.entity_id AND a.sport=w.sport AND w.status='pending' AND w.attempts=0;
  DELETE FROM pipeline_work w USING coverage_assignments a WHERE a.edition_date=e.edition_date
   AND a.resolution='no_new_evidence' AND a.stage=w.stage AND a.entity_type=w.entity_type
   AND a.entity_id=w.entity_id AND a.sport=w.sport;
  UPDATE coverage_editions SET status='characters',frozen_at=at_time,character_deadline=NULL,
   reason=CASE WHEN at_time>e.cutoff_at+interval '1 minute' THEN 'late_drain' END WHERE edition_date=e.edition_date;
  UPDATE coverage_schedule SET phase='characters',updated_at=at_time;
  PERFORM pg_notify('pipeline_work_ready','coverage:characters'); RETURN 'characters';
 END IF;
 UPDATE coverage_assignments a SET outcome='failed',resolution='blocked',finished_at=at_time,detail=w.last_error
 FROM pipeline_work w WHERE a.edition_date=e.edition_date AND a.outcome='pending' AND w.stage=a.stage
 AND w.entity_type=a.entity_type AND w.entity_id=a.entity_id AND w.sport=a.sport AND w.status='failed' AND w.attempts>=5
 AND w.input_version IS NOT DISTINCT FROM a.input_version;
 -- Dependency failures are terminal too; an unclaimable child must not hold collection closed.
 LOOP
  UPDATE coverage_assignments a SET outcome='failed',resolution='blocked',finished_at=at_time,
    detail='failed prerequisite: '||(SELECT string_agg(dep.stage,', ' ORDER BY dep.stage)
      FROM coverage_assignments dep WHERE dep.edition_date=a.edition_date AND dep.entity_type=a.entity_type
        AND dep.entity_id=a.entity_id AND dep.sport=a.sport AND dep.stage=ANY(coverage_dependencies(a.stage))
        AND dep.outcome IN ('failed','expired'))
  WHERE a.edition_date=e.edition_date AND a.outcome='pending' AND EXISTS(
    SELECT 1 FROM coverage_assignments dep WHERE dep.edition_date=a.edition_date AND dep.entity_type=a.entity_type
      AND dep.entity_id=a.entity_id AND dep.sport=a.sport AND dep.stage=ANY(coverage_dependencies(a.stage))
      AND dep.outcome IN ('failed','expired')) AND NOT EXISTS(
    SELECT 1 FROM pipeline_work w WHERE w.stage=a.stage AND w.entity_type=a.entity_type
      AND w.entity_id=a.entity_id AND w.sport=a.sport AND w.status='running');
  GET DIAGNOSTICS changed=ROW_COUNT;
  EXIT WHEN changed=0;
 END LOOP;
 INSERT INTO coverage_failures(edition_date,kind,stage,entity_type,entity_id,sport,input_version,attempts,error,receipt)
 SELECT a.edition_date,CASE WHEN a.detail LIKE 'failed prerequisite:%' THEN 'dependency_failed'
    WHEN w.status='failed' AND w.attempts>=5 THEN 'character_exhausted' ELSE 'held_assignment' END,
    a.stage,a.entity_type,a.entity_id,a.sport,a.input_version,w.attempts,
    COALESCE(a.detail,'assignment failed without a diagnostic'),
    jsonb_build_object('spectrum_ids',a.spectrum_ids,'work',to_jsonb(w),'resolution',a.resolution)
 FROM coverage_assignments a LEFT JOIN pipeline_work w USING(stage,entity_type,entity_id,sport)
 WHERE a.edition_date=e.edition_date AND a.outcome IN ('failed','expired')
 ON CONFLICT DO NOTHING;
 INSERT INTO coverage_failures(edition_date,kind,stage,entity_type,entity_id,sport,input_version,attempts,error,receipt)
 SELECT o.edition_date,'handoff_exhausted',o.source_stage,o.entity_type,o.entity_id,o.sport,
    max(o.source_input_version),max(o.attempts),string_agg(COALESCE(o.last_error,'handoff attempts exhausted'),'; '),
    jsonb_build_object('events',jsonb_agg(to_jsonb(o) ORDER BY o.id))
 FROM application_outbox o WHERE o.edition_date=e.edition_date AND o.attempts>=5
 GROUP BY o.edition_date,o.source_stage,o.entity_type,o.entity_id,o.sport ON CONFLICT DO NOTHING;
 UPDATE classifier_spectrum_deliveries d SET status='held',reason='character_assignment_failed',updated_at=at_time,
    product_ref=d.product_ref||jsonb_build_object('failure_edition',a.edition_date,'failure_stage',a.stage,'error',a.detail)
 FROM coverage_assignments a WHERE a.edition_date=e.edition_date AND a.outcome IN ('failed','expired')
   AND d.spectrum_id=ANY(a.spectrum_ids) AND d.status='pending'
   AND d.character=CASE a.stage WHEN 'narratives' THEN 'journalist' WHEN 'rating' THEN 'scout'
     WHEN 'vibe' THEN 'influencer' WHEN 'transfers' THEN 'insider' END;
 DELETE FROM pipeline_work w USING coverage_assignments a WHERE a.edition_date=e.edition_date
   AND a.detail LIKE 'failed prerequisite:%' AND a.outcome='failed' AND w.stage=a.stage
   AND w.entity_type=a.entity_type AND w.entity_id=a.entity_id AND w.sport=a.sport
   AND w.status<>'running' AND w.input_version IS NOT DISTINCT FROM a.input_version;
 IF EXISTS(SELECT 1 FROM coverage_assignments WHERE edition_date=e.edition_date AND
    outcome NOT IN ('completed','failed','expired')) OR
 EXISTS(SELECT 1 FROM application_outbox WHERE edition_date=e.edition_date AND attempts<5) THEN RETURN 'characters'; END IF;
 IF EXISTS(SELECT 1 FROM coverage_failures WHERE edition_date=e.edition_date) THEN
  UPDATE coverage_editions SET status='partial',finished_at=at_time,reason='completed_with_failures'
  WHERE edition_date=e.edition_date;
 ELSE
  UPDATE coverage_editions SET status='complete',finished_at=at_time WHERE edition_date=e.edition_date;
  UPDATE coverage_publication SET edition_date=e.edition_date,revision=revision+1;
 END IF;
 UPDATE coverage_schedule SET phase='coverage',edition_date=NULL,updated_at=at_time;
 INSERT INTO pipeline_work(stage,entity_type,entity_id,sport,input_version)
 SELECT stage,entity_type,entity_id,sport,deferred_versions[cardinality(deferred_versions)] FROM coverage_assignments
 WHERE edition_date=e.edition_date AND cardinality(deferred_versions)>0
 ON CONFLICT(stage,entity_type,entity_id,sport) DO UPDATE SET input_version=EXCLUDED.input_version,
 status='pending',attempts=0,claim_token=NULL,running_input_version=NULL,available_at=at_time,updated_at=at_time,last_error=NULL
 WHERE pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version;
 PERFORM pg_notify('pipeline_work_ready','coverage:open'); RETURN 'coverage';
END $$;
INSERT INTO schema_migrations(version) VALUES('324_nonblocking_coverage_failures') ON CONFLICT DO NOTHING;
COMMIT;
