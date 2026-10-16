BEGIN;
-- Enrollment stays off until daily character publishers and both host handoffs qualify.
CREATE TABLE public.coverage_schedule (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    enabled boolean NOT NULL DEFAULT false,
    enrolled_at timestamptz,
    timezone text NOT NULL DEFAULT 'America/Detroit' CHECK (timezone = 'America/Detroit'),
    cutoff time NOT NULL DEFAULT '00:00',
    drain_limit interval NOT NULL DEFAULT interval '15 minutes' CHECK (drain_limit > interval '0'),
    character_limit interval NOT NULL DEFAULT interval '6 hours' CHECK (character_limit > interval '0'),
    phase text NOT NULL DEFAULT 'coverage' CHECK (phase IN ('coverage','draining','characters')),
    edition_date date,
    last_tick_at timestamptz,
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK (NOT enabled OR enrolled_at IS NOT NULL),
    CHECK ((phase = 'coverage') = (edition_date IS NULL))
);
INSERT INTO public.coverage_schedule(singleton) VALUES (true);

CREATE TABLE public.coverage_editions (
    edition_date date PRIMARY KEY,
    cutoff_at timestamptz NOT NULL UNIQUE,
    drain_deadline timestamptz NOT NULL,
    frozen_at timestamptz,
    character_deadline timestamptz,
    finished_at timestamptz,
    status text NOT NULL CHECK (status IN ('draining','characters','complete','partial')),
    reason text,
    CHECK (drain_deadline > cutoff_at)
);

-- Evidence and outcomes for jobs executed by pipeline_work, never a second worker queue.
CREATE TABLE public.coverage_assignments (
    edition_date date NOT NULL REFERENCES public.coverage_editions,
    stage text NOT NULL CHECK (stage IN ('narratives','vibe','rating','transfers','momentum','sigil')),
    entity_type text NOT NULL,
    entity_id bigint NOT NULL,
    sport text NOT NULL,
    input_version text,
    spectrum_ids bigint[] NOT NULL DEFAULT '{}',
    deferred_versions text[] NOT NULL DEFAULT '{}',
    outcome text NOT NULL DEFAULT 'pending' CHECK (outcome IN ('pending','completed','failed','expired')),
    finished_at timestamptz,
    detail text,
    PRIMARY KEY (edition_date,stage,entity_type,entity_id,sport)
);

-- Shared by claims, publication and producers; transitions take the exclusive lock.
CREATE FUNCTION public.coverage_lock() RETURNS void LANGUAGE sql VOLATILE AS $$
    SELECT pg_advisory_xact_lock_shared(1935896421, 1)
$$;

CREATE FUNCTION public.coverage_is_character(task text) RETURNS boolean
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
    SELECT task IN ('narratives','vibe','rating','transfers','momentum','sigil')
$$;

CREATE FUNCTION public.coverage_claimable(task text, subject_type text, subject_id bigint, league text)
RETURNS boolean LANGUAGE sql STABLE AS $$
    SELECT NOT s.enabled OR CASE s.phase
        WHEN 'coverage' THEN NOT public.coverage_is_character(task)
        WHEN 'draining' THEN task = 'classifier' AND clock_timestamp() < e.drain_deadline
        WHEN 'characters' THEN clock_timestamp() < e.character_deadline AND EXISTS (
            SELECT 1 FROM public.coverage_assignments a
            WHERE a.edition_date=s.edition_date AND a.stage=task
              AND a.entity_type=subject_type AND a.entity_id=subject_id AND a.sport=league
              AND a.outcome='pending'
        ) AND NOT EXISTS (
            SELECT 1 FROM public.coverage_assignments upstream
            WHERE upstream.edition_date=s.edition_date AND upstream.entity_type=subject_type
              AND upstream.entity_id=subject_id AND upstream.sport=league
              AND upstream.outcome<>'completed' AND upstream.stage=ANY(CASE task
                  WHEN 'momentum' THEN ARRAY['rating','vibe']
                  WHEN 'sigil' THEN ARRAY['narratives','rating','vibe','momentum','transfers']
                  ELSE ARRAY[]::text[] END)
        ) ELSE false END
    FROM public.coverage_schedule s
    LEFT JOIN public.coverage_editions e USING (edition_date)
$$;

CREATE FUNCTION public.coverage_stage_open(task text)
RETURNS boolean LANGUAGE sql STABLE AS $$
    SELECT NOT s.enabled OR CASE s.phase
        WHEN 'coverage' THEN NOT public.coverage_is_character(task)
        WHEN 'draining' THEN task='classifier' AND clock_timestamp()<e.drain_deadline
        WHEN 'characters' THEN public.coverage_is_character(task) AND clock_timestamp()<e.character_deadline
        ELSE false END
    FROM public.coverage_schedule s LEFT JOIN public.coverage_editions e USING (edition_date)
$$;

CREATE FUNCTION public.coverage_publishable(task text, subject_type text, subject_id bigint, league text)
RETURNS boolean LANGUAGE sql STABLE AS $$
    SELECT NOT s.enabled OR CASE
        WHEN public.coverage_is_character(task) THEN
            s.phase='characters' AND clock_timestamp() < e.character_deadline AND EXISTS (
                SELECT 1 FROM public.coverage_assignments a
                WHERE a.edition_date=s.edition_date AND a.stage=task
                  AND a.entity_type=subject_type AND a.entity_id=subject_id AND a.sport=league
                  AND a.outcome='pending')
        ELSE s.phase='coverage' OR (s.phase='draining' AND clock_timestamp() < e.drain_deadline)
        END
    FROM public.coverage_schedule s
    LEFT JOIN public.coverage_editions e USING (edition_date)
$$;

CREATE FUNCTION public.coverage_spectrum_selected(character_name text, subject_type text,
    subject_id bigint, league text, spectrum_id bigint)
RETURNS boolean LANGUAGE sql STABLE AS $$
    SELECT NOT s.enabled OR s.phase<>'characters' OR EXISTS (
        SELECT 1 FROM public.coverage_assignments a
        WHERE a.edition_date=s.edition_date AND a.stage=CASE character_name
            WHEN 'journalist' THEN 'narratives' WHEN 'influencer' THEN 'vibe'
            WHEN 'scout' THEN 'rating' WHEN 'insider' THEN 'transfers' END
          AND a.entity_type=subject_type AND a.entity_id=subject_id AND a.sport=league
          AND spectrum_id=ANY(a.spectrum_ids))
    FROM public.coverage_schedule s
$$;

CREATE FUNCTION public.coverage_offer() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE s public.coverage_schedule;
BEGIN
    PERFORM public.coverage_lock();
    SELECT * INTO s FROM public.coverage_schedule;
    IF s.enabled AND s.phase='characters' AND public.coverage_is_character(NEW.stage) THEN
        UPDATE public.coverage_assignments SET deferred_versions = CASE
            WHEN NEW.input_version IS NULL OR NEW.input_version=ANY(deferred_versions)
                THEN deferred_versions ELSE array_append(deferred_versions,NEW.input_version) END
        WHERE edition_date=s.edition_date AND stage=NEW.stage AND entity_type=NEW.entity_type
          AND entity_id=NEW.entity_id AND sport=NEW.sport;
        -- Required handoffs must not replace or reopen a frozen assignment.
        IF FOUND THEN RETURN NULL; END IF;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER coverage_offer BEFORE INSERT ON public.pipeline_work
    FOR EACH ROW EXECUTE FUNCTION public.coverage_offer();

CREATE FUNCTION public.coverage_completed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.status='running' THEN
        UPDATE public.coverage_assignments a SET outcome='completed',finished_at=clock_timestamp()
        FROM public.coverage_schedule s
        WHERE s.enabled AND s.phase='characters' AND a.edition_date=s.edition_date
          AND a.stage=OLD.stage AND a.entity_type=OLD.entity_type AND a.entity_id=OLD.entity_id
          AND a.sport=OLD.sport AND a.input_version IS NOT DISTINCT FROM OLD.running_input_version
          AND a.outcome='pending';
    END IF;
    RETURN OLD;
END;
$$;
CREATE TRIGGER coverage_completed AFTER DELETE ON public.pipeline_work
    FOR EACH ROW EXECUTE FUNCTION public.coverage_completed();

-- A tick is safe to repeat after a controller restart or on two hosts.
CREATE FUNCTION public.coverage_tick(at_time timestamptz DEFAULT clock_timestamp())
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
                (stage='classifier_acquire' AND status='running') OR
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
        WHERE public.coverage_is_character(w.stage) AND w.entity_type IN ('team','player')
        ON CONFLICT(stage,entity_type,entity_id,sport) DO NOTHING;
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
        EXISTS (SELECT 1 FROM public.application_outbox)) THEN RETURN 'characters'; END IF;
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
        AND outcome='completed' AND cardinality(deferred_versions)>0
    ON CONFLICT(stage,entity_type,entity_id,sport) DO UPDATE SET
        input_version=EXCLUDED.input_version,status='pending',attempts=0,
        claim_token=NULL,running_input_version=NULL,available_at=at_time,updated_at=at_time,last_error=NULL;
    PERFORM pg_notify('pipeline_work_ready','coverage:open');
    RETURN 'coverage';
END;
$$;

INSERT INTO public.schema_migrations(version) VALUES ('309_continuous_coverage') ON CONFLICT DO NOTHING;
COMMIT;
