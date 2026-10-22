-- Proposed late-drain/carry-forward policy; production cutover requires agreement.
BEGIN;
ALTER TABLE coverage_schedule ADD COLUMN collection_cutoff time NOT NULL DEFAULT '22:00';
ALTER TABLE coverage_editions ADD COLUMN collection_closed_at timestamptz;
ALTER TABLE coverage_editions ADD COLUMN missed_boundaries integer NOT NULL DEFAULT 0;
ALTER TABLE coverage_assignments ADD COLUMN resolution text CHECK (resolution IN
    ('candidate','no_new_evidence','unchanged','abstained','blocked'));
CREATE TABLE coverage_publication (
    singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),
    edition_date date, revision bigint NOT NULL DEFAULT 0
);
INSERT INTO coverage_publication(singleton) VALUES(true);
CREATE TABLE coverage_drain_obligations (
    edition_date date NOT NULL, entity_type text NOT NULL, entity_id bigint NOT NULL,
    sport text NOT NULL,input_version text,settled_at timestamptz,
    PRIMARY KEY(edition_date,entity_type,entity_id,sport)
);
CREATE OR REPLACE FUNCTION coverage_collection_open(at_time timestamptz DEFAULT clock_timestamp())
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT NOT enabled OR (phase='coverage' AND
    ((at_time AT TIME ZONE timezone)::time < collection_cutoff OR EXISTS(SELECT 1 FROM coverage_editions WHERE finished_at >= ((at_time AT TIME ZONE timezone)::date+collection_cutoff) AT TIME ZONE timezone)) AND
    NOT EXISTS(SELECT 1 FROM coverage_editions e WHERE e.status IN ('draining','characters')))
 FROM coverage_schedule
$$;
CREATE FUNCTION coverage_collection_deadline(at_time timestamptz DEFAULT clock_timestamp())
RETURNS timestamptz LANGUAGE sql STABLE AS $$
 SELECT CASE WHEN enabled THEN ((at_time AT TIME ZONE timezone)::date+
 CASE WHEN (at_time AT TIME ZONE timezone)::time>=collection_cutoff THEN 1 ELSE 0 END+collection_cutoff)
 AT TIME ZONE timezone END FROM coverage_schedule
$$;
CREATE FUNCTION coverage_dependencies(task text) RETURNS text[] LANGUAGE sql IMMUTABLE AS $$
 SELECT CASE task WHEN 'momentum' THEN ARRAY['rating','vibe']
 WHEN 'sigil' THEN ARRAY['narratives','rating','vibe','momentum','transfers'] ELSE ARRAY[]::text[] END
$$;
CREATE OR REPLACE FUNCTION coverage_stage_open(task text) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT NOT enabled OR CASE phase WHEN 'coverage' THEN task='classifier'
 WHEN 'draining' THEN task='classifier' WHEN 'characters' THEN coverage_is_character(task) ELSE false END
 FROM coverage_schedule
$$;
CREATE OR REPLACE FUNCTION coverage_claimable(task text,subject_type text,subject_id bigint,league text)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT NOT s.enabled OR CASE s.phase
 WHEN 'coverage' THEN task='classifier'
 WHEN 'draining' THEN task='classifier' AND EXISTS(SELECT 1 FROM coverage_drain_obligations d
   WHERE d.edition_date=s.edition_date AND d.entity_type=subject_type AND d.entity_id=subject_id
   AND d.sport=league AND d.settled_at IS NULL)
 WHEN 'characters' THEN EXISTS(SELECT 1 FROM coverage_assignments a
   WHERE a.edition_date=s.edition_date AND a.stage=task AND a.entity_type=subject_type
   AND a.entity_id=subject_id AND a.sport=league AND a.outcome='pending')
 AND NOT EXISTS(SELECT 1 FROM unnest(coverage_dependencies(task)) dep
   WHERE NOT EXISTS(SELECT 1 FROM coverage_assignments a WHERE a.edition_date=s.edition_date
     AND a.stage=dep AND a.entity_type=subject_type AND a.entity_id=subject_id AND a.sport=league
     AND a.outcome='completed' AND a.resolution<>'blocked')) ELSE false END FROM coverage_schedule s
$$;
CREATE OR REPLACE FUNCTION coverage_publishable(task text,subject_type text,subject_id bigint,league text)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT NOT enabled OR coverage_claimable(task,subject_type,subject_id,league) FROM coverage_schedule
$$;
-- Only a claim-fenced publisher can resolve a frozen obligation. DELETE alone cannot.
CREATE OR REPLACE FUNCTION coverage_completed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF OLD.stage='classifier' AND OLD.claim_token::text=current_setting('scoracle.claim_token',true) THEN
  UPDATE coverage_drain_obligations d SET settled_at=clock_timestamp() FROM coverage_schedule s
  WHERE s.enabled AND s.phase='draining' AND d.edition_date=s.edition_date
   AND d.entity_type=OLD.entity_type AND d.entity_id=OLD.entity_id AND d.sport=OLD.sport
   AND d.input_version IS NOT DISTINCT FROM OLD.running_input_version;
 END IF;
 RETURN OLD;
END $$;
CREATE FUNCTION coverage_resolve_claim(task text,subject_type text,subject_id bigint,league text,token text)
RETURNS void LANGUAGE plpgsql AS $$
DECLARE a coverage_assignments; has_product boolean; has_record boolean; held boolean; pending boolean; product_table text; body_column text;
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
 SELECT COALESCE(bool_or(d.status='held'),false),COALESCE(bool_or(d.status='pending'),false)
 INTO held,pending FROM classifier_spectrum_deliveries d WHERE d.spectrum_id=ANY(a.spectrum_ids)
 AND d.character=CASE task WHEN 'narratives' THEN 'journalist' WHEN 'rating' THEN 'scout'
 WHEN 'vibe' THEN 'influencer' WHEN 'transfers' THEN 'insider' END;
 IF pending THEN RAISE EXCEPTION 'Cannot resolve undelivered frozen sources'; END IF;
 UPDATE coverage_assignments SET outcome=CASE WHEN held THEN 'failed' ELSE 'completed' END,
 resolution=CASE WHEN held THEN 'blocked' WHEN has_product THEN 'candidate'
 WHEN task='rating' AND current_setting('scoracle.assignment_resolution',true)='unchanged' THEN 'unchanged'
 WHEN has_record THEN 'abstained' WHEN cardinality(a.spectrum_ids)=0 THEN 'no_new_evidence' ELSE 'abstained' END,
 detail=CASE WHEN held THEN 'held source requires explicit resolution'
 WHEN has_product THEN 'claim-fenced validated product'
 WHEN task='rating' AND current_setting('scoracle.assignment_resolution',true)='unchanged' THEN 'exact prepared input unchanged'
 WHEN has_record THEN 'explicit no-material marker'
 WHEN cardinality(a.spectrum_ids)=0 THEN 'prepared assignment has no new usable evidence'
 ELSE 'validated source dispositions; no expression' END,finished_at=clock_timestamp()
 WHERE edition_date=a.edition_date AND stage=task AND entity_type=subject_type AND entity_id=subject_id AND sport=league;
END $$;
CREATE OR REPLACE FUNCTION coverage_tick(at_time timestamptz DEFAULT clock_timestamp()) RETURNS text
LANGUAGE plpgsql AS $$
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
 AND w.entity_type=a.entity_type AND w.entity_id=a.entity_id AND w.sport=a.sport AND w.status='failed' AND w.attempts>=5;
 IF EXISTS(SELECT 1 FROM coverage_assignments WHERE edition_date=e.edition_date AND
    (outcome<>'completed' OR resolution IS NULL OR resolution='blocked')) OR
 EXISTS(SELECT 1 FROM application_outbox WHERE edition_date=e.edition_date) THEN RETURN 'characters'; END IF;
 UPDATE coverage_editions SET status='complete',finished_at=at_time WHERE edition_date=e.edition_date;
 UPDATE coverage_publication SET edition_date=e.edition_date,revision=revision+1;
 UPDATE coverage_schedule SET phase='coverage',edition_date=NULL,updated_at=at_time;
 INSERT INTO pipeline_work(stage,entity_type,entity_id,sport,input_version)
 SELECT stage,entity_type,entity_id,sport,deferred_versions[cardinality(deferred_versions)] FROM coverage_assignments
 WHERE edition_date=e.edition_date AND cardinality(deferred_versions)>0
 ON CONFLICT(stage,entity_type,entity_id,sport) DO UPDATE SET input_version=EXCLUDED.input_version,
 status='pending',attempts=0,claim_token=NULL,running_input_version=NULL,available_at=at_time,updated_at=at_time,last_error=NULL;
 PERFORM pg_notify('pipeline_work_ready','coverage:open'); RETURN 'coverage';
END $$;
-- Keep the already-served legacy surface until the first qualified edition; never label it qualified.
ALTER TABLE transfer_rumors ADD COLUMN candidate_entity_type text, ADD COLUMN candidate_entity_id bigint;
CREATE FUNCTION stamp_candidate_edition() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 SELECT edition_date INTO NEW.candidate_edition FROM coverage_schedule WHERE enabled AND phase='characters';
 IF TG_TABLE_NAME='transfer_rumors' THEN
  SELECT w.entity_type,w.entity_id INTO NEW.candidate_entity_type,NEW.candidate_entity_id
  FROM pipeline_work w WHERE w.claim_token::text=current_setting('scoracle.claim_token',true) AND w.status='running';
 END IF;
 RETURN NEW;
END $$;
CREATE FUNCTION invalidate_coverage_publication() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 UPDATE coverage_publication SET revision=revision+1;
 RETURN NULL;
END $$;
CREATE FUNCTION published_edition() RETURNS date LANGUAGE sql STABLE AS $$
 SELECT COALESCE(NULLIF(current_setting('scoracle.published_edition',true),'' )::date,edition_date)
 FROM coverage_publication
$$;
CREATE FUNCTION coverage_carry_allowed(candidate date, task text, subject_type text, subject_id bigint, league text)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT NULLIF(current_setting('scoracle.reporting_date',true),'') IS NOT NULL OR NOT EXISTS(
 SELECT 1 FROM coverage_assignments a JOIN coverage_editions e USING(edition_date)
 WHERE a.edition_date>candidate AND a.edition_date<=published_edition() AND e.status='complete'
 AND a.stage=task AND a.entity_type=subject_type AND a.entity_id=subject_id AND a.sport=league
 AND a.resolution NOT IN ('no_new_evidence','unchanged','candidate'))
$$;
DO $$ DECLARE t text; task text; identity text; BEGIN
 FOREACH t IN ARRAY ARRAY['news_summaries','vibe_scores','stat_summaries','insider_scores',
 'transfer_rumors','momentum_summaries','sigil_synthesis','rating_history','momentum_scores'] LOOP
 EXECUTE format('ALTER TABLE public.%I ADD COLUMN candidate_edition date',t);
 EXECUTE format('CREATE TRIGGER stamp_candidate BEFORE INSERT ON public.%I FOR EACH ROW EXECUTE FUNCTION stamp_candidate_edition()',t);
 EXECUTE format('CREATE TRIGGER invalidate_publication AFTER UPDATE OR DELETE ON public.%I FOR EACH STATEMENT EXECUTE FUNCTION invalidate_coverage_publication()',t);
 task:=CASE t WHEN 'news_summaries' THEN 'narratives' WHEN 'vibe_scores' THEN 'vibe'
 WHEN 'stat_summaries' THEN 'rating' WHEN 'rating_history' THEN 'rating'
 WHEN 'insider_scores' THEN 'transfers' WHEN 'transfer_rumors' THEN 'transfers'
 WHEN 'sigil_synthesis' THEN 'sigil' ELSE 'momentum' END;
 identity:=CASE WHEN t='transfer_rumors' THEN 'p.candidate_entity_type,p.candidate_entity_id' ELSE 'p.entity_type,p.entity_id' END;
 EXECUTE format('CREATE VIEW public.qualified_%I AS SELECT p.* FROM public.%I p WHERE
  ((NOT (SELECT enabled FROM coverage_schedule) OR published_edition() IS NULL) AND p.candidate_edition IS NULL) OR
  (p.candidate_edition <= published_edition() AND EXISTS(SELECT 1 FROM coverage_editions e
     WHERE e.edition_date=p.candidate_edition AND e.status=''complete'') AND
  coverage_carry_allowed(p.candidate_edition,%L,%s,p.sport))',t,t,task,identity);
 END LOOP;
END $$;
CREATE FUNCTION coverage_peer_visible(candidate date) RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT NOT s.enabled OR candidate=s.edition_date OR (candidate<=published_edition() AND
 EXISTS(SELECT 1 FROM coverage_editions e WHERE e.edition_date=candidate AND e.status='complete'))
 FROM coverage_schedule s
$$;
DO $$ DECLARE definition text; BEGIN
 SELECT pg_get_functiondef('public.refresh_momentum_scores(text)'::regprocedure) INTO definition;
 definition:=replace(definition,'FROM stat_summaries s','FROM (SELECT * FROM stat_summaries WHERE coverage_peer_visible(candidate_edition)) s');
 definition:=replace(definition,'FROM vibe_scores s','FROM (SELECT * FROM vibe_scores WHERE coverage_peer_visible(candidate_edition)) s');
 EXECUTE definition;
END $$;
INSERT INTO schema_migrations(version) VALUES('321_durable_qualified_editions') ON CONFLICT DO NOTHING;
COMMIT;
