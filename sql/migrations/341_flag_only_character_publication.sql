-- Character publication is independent of the edition's operational health.
BEGIN;
SET LOCAL lock_timeout='5s';
CREATE TABLE public.character_output_flags (
 id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 edition_date date, stage text NOT NULL, entity_type text, entity_id bigint, sport text NOT NULL,
 flag_kind text NOT NULL DEFAULT 'malformed_output', reason text NOT NULL, claim_token uuid,
 model text, answer text, product_table text, product_id bigint,
 recorded_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE UNIQUE INDEX character_output_claim_reason ON public.character_output_flags(stage,claim_token,md5(reason))
 WHERE claim_token IS NOT NULL;
CREATE INDEX character_output_flag_date ON public.character_output_flags(edition_date,stage);
-- Preserve the exact rows suppressed by the previous publication policy before changing it.
DO $$
DECLARE item record; cols text; subject text; identifier text;
BEGIN
 FOR item IN SELECT * FROM (VALUES
  ('news_summaries','narratives','body'),('vibe_scores','vibe','prompt'),
  ('stat_summaries','rating','body'),('momentum_summaries','momentum','blurb'),
  ('sigil_synthesis','sigil','reading'),('insider_scores','transfers','read'),
  ('transfer_rumors','transfers','model_summary'),('rating_history','rating',NULL),
  ('momentum_scores','momentum',NULL)) t(base,stage,body)
 LOOP
  subject:=CASE WHEN item.base='transfer_rumors' THEN 'candidate_entity_type' ELSE 'entity_type' END;
  identifier:=CASE WHEN item.base='transfer_rumors' THEN 'candidate_entity_id' ELSE 'entity_id' END;
  IF item.body IS NOT NULL THEN
   EXECUTE format('INSERT INTO character_output_flags(edition_date,stage,entity_type,entity_id,sport,
     flag_kind,reason,product_table,product_id)
     SELECT p.candidate_edition,$1,p.%I,p.%I,p.sport,''previously_hidden'',
       COALESCE(CASE WHEN a.outcome IN (''failed'',''expired'') THEN a.detail END,''edition publication rule''),$2,p.id
     FROM public.%I p LEFT JOIN coverage_assignments a ON
       (a.edition_date,a.stage,a.entity_type,a.entity_id,a.sport)=(p.candidate_edition,$1,p.%I,p.%I,p.sport)
     WHERE NULLIF(btrim(p.%I),'''') IS NOT NULL
       AND NOT EXISTS(SELECT 1 FROM public.%I visible WHERE visible.id=p.id)',
     subject,identifier,item.base,subject,identifier,item.body,'qualified_'||item.base)
   USING item.stage,item.base;
  END IF;
  SELECT string_agg(quote_ident(attname),', ' ORDER BY attnum) INTO STRICT cols
   FROM pg_attribute WHERE attrelid=format('public.%I','qualified_'||item.base)::regclass
   AND attnum>0 AND NOT attisdropped;
  EXECUTE format('CREATE OR REPLACE VIEW public.%I AS SELECT %s FROM public.%I','qualified_'||item.base,cols,item.base);
 END LOOP;
END $$;
CREATE VIEW public.character_publication_review AS
 SELECT edition_date,stage,entity_type,entity_id,sport,flag_kind,reason,product_table,product_id,recorded_at FROM character_output_flags
 UNION ALL SELECT edition_date,stage,entity_type,entity_id,sport,kind,error,NULL,NULL,failed_at
 FROM coverage_failures WHERE coverage_is_character(stage)
 UNION ALL SELECT a.edition_date,a.stage,a.entity_type,a.entity_id,a.sport,'held_source',d.reason,
 'classifier_spectra',d.spectrum_id,d.updated_at FROM coverage_assignments a
 CROSS JOIN LATERAL unnest(a.spectrum_ids||a.carried_spectrum_ids) sid
 JOIN classifier_spectrum_deliveries d ON d.spectrum_id=sid AND d.character=CASE a.stage
 WHEN 'narratives' THEN 'journalist' WHEN 'vibe' THEN 'influencer' WHEN 'rating' THEN 'scout'
 WHEN 'transfers' THEN 'insider' END WHERE d.status='held';
CREATE OR REPLACE FUNCTION public.coverage_resolve_claim(task text,subject_type text,subject_id bigint,league text,token text)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE a coverage_assignments; has_product boolean; has_record boolean; product_table text; body_column text;
BEGIN
 SELECT a0.* INTO a FROM coverage_assignments a0 JOIN coverage_schedule s USING(edition_date)
 WHERE s.enabled AND s.phase='characters' AND a0.stage=task AND a0.entity_type=subject_type
 AND a0.entity_id=subject_id AND a0.sport=league FOR UPDATE OF a0;
 IF NOT FOUND THEN RETURN; END IF;
 IF NOT EXISTS(SELECT 1 FROM pipeline_work w WHERE w.stage=task AND w.entity_type=subject_type
 AND w.entity_id=subject_id AND w.sport=league AND w.status='running' AND w.claim_token::text=token
 AND w.running_input_version IS NOT DISTINCT FROM a.input_version) THEN
 RAISE EXCEPTION 'Cannot resolve an unfenced assignment'; END IF;
 product_table:=CASE task WHEN 'narratives' THEN 'news_summaries' WHEN 'vibe' THEN 'vibe_scores'
 WHEN 'rating' THEN 'stat_summaries' WHEN 'transfers' THEN 'insider_scores'
 WHEN 'momentum' THEN 'momentum_summaries' WHEN 'sigil' THEN 'sigil_synthesis' END;
 body_column:=CASE task WHEN 'vibe' THEN 'prompt' WHEN 'transfers' THEN 'read'
 WHEN 'momentum' THEN 'blurb' WHEN 'sigil' THEN 'reading' ELSE 'body' END;
 EXECUTE format('SELECT count(*)>0,COALESCE(bool_or(length(btrim(%I))>0),false) FROM public.%I
 WHERE candidate_edition=$1 AND entity_type=$2 AND entity_id=$3 AND sport=$4',body_column,product_table)
 INTO has_record,has_product USING a.edition_date,subject_type,subject_id,league;
 IF EXISTS(SELECT 1 FROM classifier_spectrum_deliveries d WHERE d.spectrum_id=ANY(a.spectrum_ids)
    AND d.status='pending' AND d.character=CASE task WHEN 'narratives' THEN 'journalist' WHEN 'rating' THEN 'scout'
    WHEN 'vibe' THEN 'influencer' WHEN 'transfers' THEN 'insider' END) THEN
  PERFORM coverage_reconcile_sources(a,false);
  SELECT * INTO STRICT a FROM coverage_assignments WHERE (edition_date,stage,entity_type,entity_id,sport)=(a.edition_date,task,subject_type,subject_id,league);
 END IF;

 UPDATE classifier_spectrum_deliveries d SET status='held',reason='unreviewed_at_publication',updated_at=clock_timestamp()
 WHERE d.spectrum_id=ANY(a.spectrum_ids) AND d.status='pending' AND d.character=CASE task
 WHEN 'narratives' THEN 'journalist' WHEN 'vibe' THEN 'influencer' WHEN 'rating' THEN 'scout' WHEN 'transfers' THEN 'insider' END;
 UPDATE coverage_assignments SET outcome='completed',resolution=CASE WHEN has_product THEN 'candidate'
 WHEN task='rating' AND current_setting('scoracle.assignment_resolution',true)='unchanged' THEN 'unchanged'
 WHEN has_record THEN 'abstained' WHEN cardinality(a.spectrum_ids)=0 THEN 'no_new_evidence' ELSE 'abstained' END,
 detail=CASE WHEN has_product THEN 'claim-fenced product'
 WHEN task='rating' AND current_setting('scoracle.assignment_resolution',true)='unchanged' THEN 'exact prepared input unchanged'
 WHEN has_record THEN 'explicit no-material marker'
 WHEN cardinality(a.spectrum_ids)=0 THEN 'prepared assignment has no new usable evidence'
 ELSE 'validated source dispositions; no expression' END,finished_at=clock_timestamp()
 WHERE edition_date=a.edition_date AND stage=task AND entity_type=subject_type AND entity_id=subject_id AND sport=league;
END $$;
CREATE OR REPLACE FUNCTION public.coverage_claimable(task text, subject_type text, subject_id bigint, league text) RETURNS boolean
    LANGUAGE sql STABLE
    AS $$
 SELECT NOT s.enabled OR CASE s.phase
 WHEN 'coverage' THEN task='classifier'
 WHEN 'draining' THEN task='classifier' AND EXISTS(SELECT 1 FROM coverage_drain_obligations d
   WHERE d.edition_date=s.edition_date AND d.entity_type=subject_type AND d.entity_id=subject_id
   AND d.sport=league AND d.settled_at IS NULL)
 WHEN 'characters' THEN EXISTS(SELECT 1 FROM coverage_assignments a
   WHERE a.edition_date=s.edition_date AND a.stage=task AND a.entity_type=subject_type
   AND a.entity_id=subject_id AND a.sport=league AND a.outcome='pending')
 AND NOT EXISTS(SELECT 1 FROM coverage_assignments dep WHERE dep.edition_date=s.edition_date
  AND dep.stage=ANY(coverage_dependencies(task)) AND dep.stage=ANY(s.required_stages)
  AND dep.entity_type=subject_type AND dep.entity_id=subject_id AND dep.sport=league
  AND dep.outcome='pending') ELSE false END FROM coverage_schedule s
$$;
CREATE OR REPLACE FUNCTION public.coverage_tick(at_time timestamptz DEFAULT clock_timestamp()) RETURNS text
LANGUAGE plpgsql SET search_path=public,pg_temp AS $$
DECLARE s coverage_schedule; e coverage_editions; local_day date; boundary timestamptz;
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
 FROM pipeline_work w WHERE a.edition_date=e.edition_date AND a.outcome='pending' AND a.stage=ANY(s.required_stages) AND w.stage=a.stage
 AND w.entity_type=a.entity_type AND w.entity_id=a.entity_id AND w.sport=a.sport AND w.status='failed' AND w.attempts>=5
 AND w.input_version IS NOT DISTINCT FROM a.input_version;
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
    stage=ANY(s.required_stages) AND outcome NOT IN ('completed','failed','expired')) OR
 EXISTS(SELECT 1 FROM application_outbox WHERE edition_date=e.edition_date AND source_stage=ANY(s.required_stages) AND attempts<5) THEN RETURN 'characters'; END IF;
 IF EXISTS(SELECT 1 FROM coverage_failures WHERE edition_date=e.edition_date) OR
 EXISTS(SELECT 1 FROM coverage_assignments WHERE edition_date=e.edition_date
    AND stage=ANY(s.required_stages) AND outcome<>'completed') THEN
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
UPDATE coverage_publication SET revision=revision+1;
INSERT INTO public.schema_migrations(version) VALUES('341_flag_only_character_publication');
COMMIT;
