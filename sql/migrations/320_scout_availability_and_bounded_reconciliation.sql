BEGIN;
ALTER TABLE public.classifier_spectrum_deliveries DROP CONSTRAINT classifier_no_scout_pending;
CREATE OR REPLACE FUNCTION public.classifier_spectrum_dispatch()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    target_type text;
    target_id integer;
    league text;
    work_stage text;
    revision text;
BEGIN
    IF NEW.status <> 'pending' THEN RETURN NEW; END IF;
    work_stage := CASE NEW.character
        WHEN 'journalist' THEN 'narratives'
        WHEN 'insider' THEN 'transfers'
        WHEN 'scout' THEN 'rating'
        WHEN 'influencer' THEN 'vibe'
        ELSE NULL END;
    IF work_stage IS NULL THEN
        RAISE EXCEPTION 'Classifier spectrum character is not wired: %', NEW.character;
    END IF;
    SELECT x.target->>'entity_type', (x.target->>'entity_id')::integer, x.sport
      INTO target_type, target_id, league
      FROM public.classifier_spectra x WHERE x.id = NEW.spectrum_id;
    IF target_type IS NULL OR target_type NOT IN ('team','player','person') OR target_id IS NULL OR target_id <= 0 THEN
        RAISE EXCEPTION 'Classifier spectrum delivery requires a canonical team/player/person target';
    END IF;
    IF NEW.character='scout' AND target_type NOT IN ('team','player') THEN RAISE EXCEPTION 'Scout requires team or player'; END IF;
    revision := CASE WHEN NEW.character='scout' THEN 'classifier-availability-v1:' ELSE 'classifier-spectrum-v1:' END || NEW.character || ':s' || NEW.spectrum_id;
    INSERT INTO public.pipeline_work(stage,entity_type,entity_id,sport,input_version)
    VALUES(work_stage,target_type,target_id,league,revision)
    ON CONFLICT(stage,entity_type,entity_id,sport) DO UPDATE SET
        status='pending',attempts=0,input_version=EXCLUDED.input_version,
        available_at=CASE WHEN pipeline_work.status='pending' THEN pipeline_work.available_at ELSE now() END,
        updated_at=now(),last_error=NULL,running_input_version=NULL,claim_token=NULL
    WHERE pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version;
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION public.classifier_article_ready() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE measured record;
BEGIN
    IF NEW.status<>'pending' THEN RETURN NEW; END IF;
    SELECT x.criteria_version,x.gate,x.clean_text,s.source INTO STRICT measured
        FROM public.classifier_spectra x JOIN public.classifier_sources s ON s.id=x.source_id
        WHERE x.id=NEW.spectrum_id;
    IF measured.criteria_version IN ('classification-criteria-v9-headlines','classification-criteria-v10-availability') THEN
        IF NOT COALESCE((measured.gate->'characters'->>NEW.character)::boolean,false)
            OR measured.source->>'evidence_scope' IS DISTINCT FROM 'headline'
            OR length(btrim(measured.clean_text))=0
            OR measured.clean_text IS DISTINCT FROM measured.source->>'title' THEN
            RAISE EXCEPTION 'Headline delivery requires an admitted character and retained headline';
        END IF;
    ELSIF measured.criteria_version IN ('classification-criteria-v7-headline','classification-criteria-v8-admission') AND
        (length(btrim(COALESCE(measured.source->>'body','')))=0
         OR NOT COALESCE((measured.gate->'characters'->>NEW.character)::boolean,false)) THEN
        RAISE EXCEPTION 'Historical delivery requires an admitted character and retained article';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TABLE public.classifier_reconciliation_cursor (
 singleton boolean PRIMARY KEY DEFAULT true CHECK(singleton),article_id bigint NOT NULL DEFAULT 0,sport text NOT NULL DEFAULT ''
);
INSERT INTO public.classifier_reconciliation_cursor DEFAULT VALUES;
CREATE OR REPLACE FUNCTION public.classifier_replay_admission(batch_limit integer DEFAULT 1000)
RETURNS bigint LANGUAGE plpgsql AS $$
DECLARE cursor_row public.classifier_reconciliation_cursor; candidate record; scanned integer:=0; queued bigint:=0;
BEGIN
 IF batch_limit NOT BETWEEN 1 AND 1000 THEN RAISE EXCEPTION 'batch must be 1..1000'; END IF;
 PERFORM public.coverage_lock();
 IF NOT public.coverage_collection_open() THEN RETURN 0; END IF;
 SELECT * INTO cursor_row FROM classifier_reconciliation_cursor FOR UPDATE;
 FOR candidate IN
  WITH query_sources AS MATERIALIZED (
   SELECT DISTINCT article_id,sport FROM harvester_query_provenance
   WHERE (article_id,sport)>(cursor_row.article_id,cursor_row.sport) ORDER BY article_id,sport LIMIT batch_limit
  ), named_sources AS MATERIALIZED (
   SELECT DISTINCT article_id,sport FROM news_article_entities
   WHERE (article_id,sport)>(cursor_row.article_id,cursor_row.sport) ORDER BY article_id,sport LIMIT batch_limit
  ) SELECT * FROM (SELECT * FROM query_sources UNION SELECT * FROM named_sources) candidates
    ORDER BY article_id,sport LIMIT batch_limit
 LOOP
  scanned:=scanned+1;
  IF public.classifier_enqueue_admission(candidate.article_id,candidate.sport) THEN
   queued:=queued+1;
   UPDATE pipeline_work w SET input_version='headline-history:'||w.input_version
   FROM news_articles a WHERE w.stage='classifier' AND w.entity_type='article'
    AND w.entity_id=candidate.article_id AND w.sport=candidate.sport AND a.id=w.entity_id
    AND (a.published_at IS NULL OR a.published_at<now()-interval '48 hours')
    AND w.status='pending' AND w.input_version NOT LIKE 'headline-history:%';
  END IF;
  UPDATE classifier_reconciliation_cursor SET article_id=candidate.article_id,sport=candidate.sport;
 END LOOP;
 IF scanned=0 THEN UPDATE classifier_reconciliation_cursor SET article_id=0,sport=''; END IF;
 RETURN queued;
END $$;
INSERT INTO schema_migrations(version) VALUES('320_scout_availability_and_bounded_reconciliation');
COMMIT;
