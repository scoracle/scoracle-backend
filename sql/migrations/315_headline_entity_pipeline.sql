BEGIN;
-- Run the coordinated worker cutover between daily editions; keep archived evidence and products.
DO $$ BEGIN
    IF EXISTS(SELECT 1 FROM public.coverage_schedule WHERE enabled AND phase<>'coverage') THEN
        RAISE EXCEPTION 'Apply the headline entity cutover during coverage with workers stopped';
    END IF;
END $$;
CREATE OR REPLACE FUNCTION public.classifier_identity_candidates(league text, headline text, body text)
RETURNS jsonb LANGUAGE sql STABLE AS $$
    WITH input AS MATERIALIZED (
        SELECT ' '||public.nrm(headline||' '||body)||' ' AS text
    ), words AS MATERIALIZED (
        SELECT DISTINCT unnest(string_to_array(input.text,' ')) AS word FROM input
    ), matching_names AS MATERIALIZED (
        SELECT s.entity_type,s.entity_id,s.norm FROM public.entity_name_surfaces s
        JOIN words ON words.word=split_part(s.norm,' ',1) CROSS JOIN input
        WHERE s.sport=league AND s.surface_kind IN ('name','alias') AND s.norm<>''
        AND strpos(input.text,' '||s.norm||' ')>0
    ), unique_names AS (
        SELECT s.* FROM matching_names s
        WHERE NOT EXISTS(SELECT 1 FROM public.entity_name_surfaces other
            WHERE other.sport=league AND other.norm=s.norm
            AND (other.entity_type,other.entity_id)<>(s.entity_type,s.entity_id))
    ), candidates AS (
        SELECT DISTINCT n.entity_type,n.entity_id,COALESCE(t.name,p.name,pp.full_name) AS name
        FROM unique_names n
        LEFT JOIN public.teams t ON n.entity_type='team' AND t.id=n.entity_id AND t.sport=league
        LEFT JOIN public.players p ON n.entity_type='player' AND p.id=n.entity_id AND p.sport=league
        LEFT JOIN public.persons pp ON n.entity_type='person' AND pp.id=n.entity_id AND pp.sport=league
    ) SELECT COALESCE(jsonb_agg(jsonb_build_object('entity_type',entity_type,'entity_id',entity_id,
        'sport',league,'name',name,'identity_method','unique_name_surface_v2')
        ORDER BY entity_type,entity_id) FILTER(WHERE name IS NOT NULL),'[]'::jsonb) FROM candidates
$$;

-- One candidate set for scoring, source fencing and revision recovery. Query hits are hypotheses.
CREATE FUNCTION public.classifier_headline_subjects(article bigint, league text)
RETURNS jsonb LANGUAGE sql STABLE AS $$
    WITH candidates AS (
        SELECT entity_type,entity_id FROM public.harvester_query_provenance WHERE article_id=article AND sport=league
        UNION SELECT entity_type,entity_id FROM public.news_article_entities WHERE article_id=article AND sport=league
        UNION SELECT c->>'entity_type',(c->>'entity_id')::integer
            FROM public.news_articles a CROSS JOIN LATERAL jsonb_array_elements(public.classifier_identity_candidates(league,a.title,'')) c
            WHERE a.id=article
    ), names AS (
        SELECT 'team' AS entity_type,id,name FROM public.teams WHERE sport=league
        UNION ALL SELECT 'player',id,name FROM public.players WHERE sport=league
        UNION ALL SELECT 'person',id,full_name FROM public.persons WHERE sport=league
    ) SELECT COALESCE(jsonb_agg(jsonb_build_object('entity_type',n.entity_type,'entity_id',n.id,'sport',league,'name',n.name)
        ORDER BY n.entity_type,n.id),'[]'::jsonb)
    FROM candidates c JOIN names n ON n.entity_type=c.entity_type AND n.id=c.entity_id
$$;

CREATE OR REPLACE FUNCTION public.classifier_discovery_version(article bigint, league text)
RETURNS text LANGUAGE sql STABLE AS $$
    SELECT CASE WHEN jsonb_array_length(c.subjects)>0 THEN
        md5(jsonb_build_array('headline-subjects-v2',a.url,a.title,COALESCE(a.source,''),extract(epoch FROM a.published_at),
            a.duplicate_of,public.classifier_identity_candidates(league,a.title,''),c.subjects)::text) END
    FROM public.news_articles a CROSS JOIN LATERAL (SELECT public.classifier_headline_subjects(article,league) AS subjects) c
    WHERE a.id=article
$$;

CREATE OR REPLACE FUNCTION public.classifier_enqueue_admission(article bigint, league text)
RETURNS boolean LANGUAGE sql VOLATILE AS $$
    WITH revision AS (SELECT public.classifier_discovery_version(article,league) AS version), queued AS (
        INSERT INTO public.pipeline_work(stage,entity_type,entity_id,sport,input_version)
        SELECT 'classifier','article',article,league,version FROM revision
        WHERE version IS NOT NULL AND NOT EXISTS (
            SELECT 1 FROM public.classifier_source_heads h JOIN public.classifier_sources s ON s.id=h.source_id
            WHERE h.article_id=article AND h.sport=league AND s.discovery_version=version)
        ON CONFLICT(stage,entity_type,entity_id,sport) DO UPDATE SET
            status='pending',attempts=0,input_version=EXCLUDED.input_version,
            available_at=CASE WHEN pipeline_work.status='pending' THEN pipeline_work.available_at ELSE now() END,
            updated_at=now(),last_error=NULL,running_input_version=NULL,claim_token=NULL
        WHERE pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version RETURNING 1
    ) SELECT EXISTS(SELECT 1 FROM queued)
$$;

CREATE OR REPLACE FUNCTION public.classifier_replay_admission(batch_limit integer DEFAULT 1000)
RETURNS bigint LANGUAGE sql VOLATILE AS $$
    WITH candidates AS MATERIALIZED (
        SELECT article_id,sport FROM public.harvester_query_provenance
        UNION SELECT article_id,sport FROM public.news_article_entities
    ), revisions AS MATERIALIZED (
        SELECT c.*,a.published_at,a.feed_rank,public.classifier_discovery_version(c.article_id,c.sport) AS version
        FROM candidates c JOIN public.news_articles a ON a.id=c.article_id WHERE length(btrim(a.title))>0
    ), batch AS MATERIALIZED (
        SELECT r.* FROM revisions r WHERE version IS NOT NULL AND NOT EXISTS(
            SELECT 1 FROM public.classifier_source_heads h JOIN public.classifier_sources s ON s.id=h.source_id
            WHERE h.article_id=r.article_id AND h.sport=r.sport AND s.discovery_version=r.version)
        AND NOT EXISTS(SELECT 1 FROM public.pipeline_work w WHERE w.stage='classifier' AND w.entity_type='article'
            AND w.entity_id=r.article_id AND w.sport=r.sport)
        ORDER BY published_at DESC NULLS LAST,feed_rank ASC NULLS LAST,article_id,sport
        LIMIT GREATEST(batch_limit-(SELECT count(*) FROM public.pipeline_work WHERE stage='classifier' AND entity_type='article'
            AND (status IN ('pending','running') OR (status='failed' AND attempts<5))),0)
    ), queued AS (
        INSERT INTO public.pipeline_work(stage,entity_type,entity_id,sport,input_version)
        SELECT 'classifier','article',article_id,sport,
            CASE WHEN published_at IS NULL OR published_at<now()-interval '48 hours' THEN 'headline-history:' ELSE '' END||version FROM batch
        ON CONFLICT(stage,entity_type,entity_id,sport) DO NOTHING RETURNING 1
    ) SELECT count(*) FROM queued
$$;
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
    revision := 'classifier-spectrum-v1:' || NEW.character || ':s' || NEW.spectrum_id;
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
DROP TRIGGER IF EXISTS classifier_spectrum_delivery_dispatch ON public.classifier_spectrum_deliveries;
CREATE TRIGGER classifier_spectrum_delivery_dispatch AFTER INSERT OR UPDATE OF status
    ON public.classifier_spectrum_deliveries FOR EACH ROW
    EXECUTE FUNCTION public.classifier_spectrum_dispatch();


-- Reporting products accept every known person role. Statistical products stay team/player only.
ALTER TABLE public.news_summaries DROP CONSTRAINT news_summaries_entity_type_check;
ALTER TABLE public.news_summaries ADD CONSTRAINT news_summaries_entity_type_check CHECK(entity_type IN ('team','player','person'));
ALTER TABLE public.vibe_scores DROP CONSTRAINT vibe_scores_entity_type_check;
ALTER TABLE public.vibe_scores ADD CONSTRAINT vibe_scores_entity_type_check CHECK(entity_type IN ('team','player','person'));
ALTER TABLE public.insider_scores DROP CONSTRAINT insider_scores_entity_type_check;
ALTER TABLE public.insider_scores ADD CONSTRAINT insider_scores_entity_type_check CHECK(entity_type IN ('team','player','person'));
ALTER TABLE public.application_outbox DROP CONSTRAINT application_outbox_entity_type_check;
ALTER TABLE public.application_outbox ADD CONSTRAINT application_outbox_entity_type_check CHECK(entity_type IN ('team','player','person'));
ALTER TABLE public.cognition_ledger DROP CONSTRAINT cognition_ledger_entity_type_check;
ALTER TABLE public.cognition_ledger ADD CONSTRAINT cognition_ledger_entity_type_check CHECK(entity_type IN ('team','player','person','article','candidate','fixture'));
CREATE OR REPLACE FUNCTION public.coverage_tick(at_time timestamptz DEFAULT clock_timestamp())
RETURNS text LANGUAGE plpgsql AS $$
DECLARE
    s public.coverage_schedule;
    e public.coverage_editions;
    local_day date;
    boundary timestamptz;
    deadline boolean;
BEGIN
    PERFORM pg_advisory_xact_lock(1935896421,1);
    SELECT * INTO s FROM public.coverage_schedule FOR UPDATE;
    IF NOT s.enabled THEN RETURN 'disabled'; END IF;
    UPDATE public.coverage_schedule SET last_tick_at=at_time;
    IF s.phase='coverage' THEN
        local_day := (at_time AT TIME ZONE s.timezone)::date;
        boundary := (local_day+s.cutoff) AT TIME ZONE s.timezone;
        IF boundary > at_time THEN
            local_day := local_day-1;
            boundary := (local_day+s.cutoff) AT TIME ZONE s.timezone;
        END IF;
        IF boundary < s.enrolled_at OR EXISTS (
            SELECT 1 FROM public.coverage_editions WHERE edition_date=local_day
        ) THEN RETURN 'coverage'; END IF;
        -- A long outage records a partial edition instead of starting an obsolete night.
        IF at_time >= boundary+s.drain_limit+s.character_limit THEN
            INSERT INTO public.coverage_editions
                (edition_date,cutoff_at,drain_deadline,finished_at,status,reason)
            VALUES (local_day,boundary,boundary+s.drain_limit,at_time,'partial','controller_missed_window');
            RETURN 'coverage';
        END IF;
        INSERT INTO public.coverage_editions(edition_date,cutoff_at,drain_deadline,status)
        VALUES(local_day,boundary,boundary+s.drain_limit,'draining');
        UPDATE public.coverage_schedule SET phase='draining',edition_date=local_day,updated_at=at_time;
        RETURN 'draining';
    END IF;

    SELECT * INTO STRICT e FROM public.coverage_editions WHERE edition_date=s.edition_date FOR UPDATE;
    IF s.phase='draining' THEN
        IF at_time >= e.cutoff_at+s.drain_limit+s.character_limit THEN
            UPDATE public.pipeline_work SET status='pending',claim_token=NULL,running_input_version=NULL,
                updated_at=at_time WHERE status='running';
            UPDATE public.coverage_editions SET status='partial',finished_at=at_time,
                reason='controller_missed_window' WHERE edition_date=e.edition_date;
            UPDATE public.coverage_schedule SET phase='coverage',edition_date=NULL,updated_at=at_time;
            PERFORM pg_notify('pipeline_work_ready','coverage:open');
            RETURN 'coverage';
        END IF;
        IF at_time < e.drain_deadline AND EXISTS (
            SELECT 1 FROM public.pipeline_work WHERE
                (stage='classifier' AND (status='running' OR
                    (status IN ('pending','failed') AND attempts<5 AND available_at<=at_time)))
        ) THEN RETURN 'draining'; END IF;
        -- Publication holds the shared transition lock. Anything left now loses its lease.
        UPDATE public.pipeline_work SET status='pending',claim_token=NULL,running_input_version=NULL,
            updated_at=at_time
        WHERE status='running';
        INSERT INTO public.pipeline_work(stage,entity_type,entity_id,sport,input_version)
        SELECT DISTINCT downstream,w.entity_type,w.entity_id,w.sport,
            CASE WHEN downstream='transfers' THEN 'classifier-spectrum-v1:daily:'||e.edition_date
                 WHEN downstream='rating' THEN 'daily-rating:'||e.edition_date
                 ELSE 'coverage:'||e.edition_date END
        FROM public.pipeline_work w CROSS JOIN
            unnest(ARRAY['narratives','vibe','rating','transfers','momentum','sigil']) downstream
        WHERE public.coverage_is_character(w.stage) AND (w.entity_type IN ('team','player') OR (w.entity_type='person' AND downstream IN ('narratives','vibe','transfers')))
        ON CONFLICT(stage,entity_type,entity_id,sport) DO NOTHING;
        UPDATE public.pipeline_work SET status='pending',attempts=0,available_at=clock_timestamp(),
            claim_token=NULL,running_input_version=NULL,last_error=NULL,updated_at=at_time
        WHERE public.coverage_is_character(stage);
        INSERT INTO public.coverage_assignments(edition_date,stage,entity_type,entity_id,sport,input_version,spectrum_ids)
        SELECT e.edition_date,w.stage,w.entity_type,w.entity_id,w.sport,w.input_version,
            COALESCE((SELECT array_agg(x.id ORDER BY x.id) FROM public.classifier_spectrum_deliveries d
                JOIN public.classifier_spectra x ON x.id=d.spectrum_id
                WHERE d.status='pending' AND x.sport=w.sport
                  AND x.target->>'entity_type'=w.entity_type
                  AND x.target->>'entity_id'=w.entity_id::text
                  AND d.character=CASE w.stage WHEN 'narratives' THEN 'journalist'
                    WHEN 'vibe' THEN 'influencer' WHEN 'rating' THEN 'scout'
                    WHEN 'transfers' THEN 'insider' END),'{}'::bigint[])
        FROM public.pipeline_work w WHERE public.coverage_is_character(w.stage);
        UPDATE public.coverage_editions SET status='characters',frozen_at=at_time,
            character_deadline=LEAST(at_time+s.character_limit,e.cutoff_at+s.drain_limit+s.character_limit)
        WHERE edition_date=e.edition_date;
        UPDATE public.coverage_schedule SET phase='characters',updated_at=at_time;
        PERFORM pg_notify('pipeline_work_ready','coverage:characters');
        RETURN 'characters';
    END IF;

    UPDATE public.coverage_assignments a SET outcome='failed',finished_at=at_time,detail=w.last_error
    FROM public.pipeline_work w
    WHERE a.edition_date=e.edition_date AND a.outcome='pending' AND w.stage=a.stage
      AND w.entity_type=a.entity_type AND w.entity_id=a.entity_id AND w.sport=a.sport
      AND w.status='failed' AND w.attempts>=5;
    deadline := at_time >= e.character_deadline;
    IF NOT deadline AND (EXISTS (SELECT 1 FROM public.coverage_assignments
            WHERE edition_date=e.edition_date AND outcome='pending') OR
        EXISTS (SELECT 1 FROM public.application_outbox WHERE edition_date=e.edition_date)) THEN RETURN 'characters'; END IF;
    UPDATE public.coverage_assignments SET outcome='expired',finished_at=at_time,detail='character_deadline'
    WHERE edition_date=e.edition_date AND outcome='pending';
    UPDATE public.pipeline_work w SET status='pending',claim_token=NULL,running_input_version=NULL,
        updated_at=at_time
    FROM public.coverage_assignments a
    WHERE a.edition_date=e.edition_date AND w.stage=a.stage AND w.entity_type=a.entity_type
      AND w.entity_id=a.entity_id AND w.sport=a.sport AND w.status='running';
    UPDATE public.coverage_editions SET finished_at=at_time,
        status=CASE WHEN deadline OR EXISTS (SELECT 1 FROM public.coverage_assignments
            WHERE edition_date=e.edition_date AND outcome IN ('failed','expired')) THEN 'partial' ELSE 'complete' END,
        reason=CASE WHEN deadline THEN 'character_deadline' ELSE NULL END
    WHERE edition_date=e.edition_date;
    UPDATE public.coverage_schedule SET phase='coverage',edition_date=NULL,updated_at=at_time;
    INSERT INTO public.pipeline_work(stage,entity_type,entity_id,sport,input_version)
    SELECT stage,entity_type,entity_id,sport,deferred_versions[cardinality(deferred_versions)]
    FROM public.coverage_assignments WHERE edition_date=e.edition_date
        AND cardinality(deferred_versions)>0
    ON CONFLICT(stage,entity_type,entity_id,sport) DO UPDATE SET
        input_version=EXCLUDED.input_version,status='pending',attempts=0,
        claim_token=NULL,running_input_version=NULL,available_at=clock_timestamp(),updated_at=at_time,last_error=NULL;
    PERFORM pg_notify('pipeline_work_ready','coverage:open');
    RETURN 'coverage';
END;
$$;


DELETE FROM public.pipeline_work WHERE stage='classifier_acquire';
ALTER TABLE public.pipeline_work ADD CONSTRAINT pipeline_work_reader_retired CHECK(stage<>'classifier_acquire');
INSERT INTO public.schema_migrations(version) VALUES ('315_headline_entity_pipeline') ON CONFLICT DO NOTHING;
COMMIT;
