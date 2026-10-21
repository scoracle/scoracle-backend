-- Freeze the same selected spectra once, rather than scanning them per assignment.
BEGIN;

CREATE OR REPLACE FUNCTION public.coverage_tick(at_time timestamp with time zone DEFAULT clock_timestamp()) RETURNS text
    LANGUAGE plpgsql
    AS $$
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

INSERT INTO public.schema_migrations(version) VALUES ('318_coverage_freeze_set_based')
    ON CONFLICT DO NOTHING;

COMMIT;
