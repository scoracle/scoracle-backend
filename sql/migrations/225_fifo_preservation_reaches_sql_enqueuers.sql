-- 225 — FIFO preservation reaches the SQL enqueuers.
--
-- The 08-16 starvation-inversion fix (commit e382a13: "pending rows keep their
-- FIFO place") landed in work::enqueue (Rust) and work.Enqueue (Go) — but never
-- in the two SQL-side enqueuers, which kept restamping a still-pending row's
-- available_at to NOW() on every conflict:
--
--   * enqueue_voices_on_packet (both arms) — the SOLE waker for narratives and
--     vibe. Arm 2's input_version is 'pk:' || slice_fingerprint, so every packet
--     recompile with a moved narratives slice reopened AND restamped pending
--     rows to the back of the line — activity-resets-position, the exact
--     inversion the fix was written for, live on the rail's busiest stages.
--   * enqueue_fixture_boxscore — same clause, lower stakes (fixture grain).
--
-- This migration re-issues both functions with the canonical conflict clause
-- from work.rs::enqueue: a still-pending row keeps its available_at; only a
-- non-pending (failed/completed-elsewhere) row restamps. Everything else in
-- both functions is byte-identical to the incumbents (mig 206/212 lineage for
-- the trigger; the boxscore function's guards unchanged).

CREATE OR REPLACE FUNCTION public.enqueue_voices_on_packet() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    -- Arm 1: tag-subscribed voices. DISTINCT because two tags can reach the same (stage,
    -- entity) pair — input_version is stage-keyed, so the rows are identical and collapse
    -- (without it, ON CONFLICT would hit "cannot affect row a second time").
    INSERT INTO public.pipeline_work
        (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
    SELECT DISTINCT s.stage, se.entity_type, se.entity_id, se.sport, 'pending',
           'pk:' || COALESCE(NEW.slice_fingerprints ->> s.stage, NEW.id::text),
           NOW(), NOW()
      FROM unnest(NEW.routing_tags) AS t(tag)
      JOIN public.stage_routing_subscriptions s ON s.tag = t.tag
      JOIN public.storyline_entities se
        ON se.storyline_id = NEW.storyline_id
       AND se.entity_type  = s.entity_type
       AND se.left_at IS NULL
    ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
        status        = 'pending',
        attempts      = 0,
        -- A still-pending row keeps its place in the FIFO (mig 225, mirrors
        -- work.rs/work.go): restamping to NOW() sent every re-noticed entity to
        -- the back of the line, starving hot entities behind quiet aged ones.
        available_at  = CASE WHEN public.pipeline_work.status = 'pending'
                             THEN public.pipeline_work.available_at
                             ELSE NOW() END,
        updated_at    = NOW(),
        last_error    = NULL,
        input_version = EXCLUDED.input_version
    WHERE public.pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
       OR public.pipeline_work.status = 'failed';

    -- Arm 2: the Journalist reads everything (§1c — an unconditional narratives fan-out per
    -- active participant, at the player/team grain the voices are keyed on) — but only once a
    -- narratives subscription exists at that grain (mig 212, D-T14 (b)). The tag column is NOT
    -- read here: this is an existence gate, not a tag join. Empty table = inert trigger =
    -- packets compile in shadow without touching pipeline_work.
    INSERT INTO public.pipeline_work
        (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
    SELECT 'narratives', se.entity_type, se.entity_id, se.sport, 'pending',
           'pk:' || COALESCE(NEW.slice_fingerprints ->> 'narratives', NEW.id::text),
           NOW(), NOW()
      FROM public.storyline_entities se
     WHERE se.storyline_id = NEW.storyline_id
       AND se.left_at IS NULL
       AND se.entity_type IN ('player','team')
       AND EXISTS (
           SELECT 1
             FROM public.stage_routing_subscriptions s
            WHERE s.stage       = 'narratives'
              AND s.entity_type = se.entity_type)
    ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
        status        = 'pending',
        attempts      = 0,
        -- FIFO preservation, as above (mig 225).
        available_at  = CASE WHEN public.pipeline_work.status = 'pending'
                             THEN public.pipeline_work.available_at
                             ELSE NOW() END,
        updated_at    = NOW(),
        last_error    = NULL,
        input_version = EXCLUDED.input_version
    WHERE public.pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
       OR public.pipeline_work.status = 'failed';

    -- The statement-level mig-133 notify trigger on pipeline_work already covers the wake-up;
    -- identical (channel, payload) notifications coalesce within a transaction, so this
    -- explicit notify is free — kept because it documents the wake-up contract at the seam.
    PERFORM pg_notify('pipeline_work_ready', '');
    RETURN NEW;
END;
$$;

CREATE OR REPLACE FUNCTION public.enqueue_fixture_boxscore(p_fixture_id integer) RETURNS boolean
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_sport text;
    v_status text;
    v_input_version text;
BEGIN
    SELECT sport, status, public.fixture_boxscore_input_version(id)
      INTO v_sport, v_status, v_input_version
      FROM public.fixtures
     WHERE id = p_fixture_id;

    IF v_sport IS NULL THEN
        RETURN false;
    END IF;
    IF v_status NOT IN ('completed', 'seeded') THEN
        RETURN false;
    END IF;
    IF v_input_version IS NULL THEN
        RETURN false;
    END IF;

    INSERT INTO public.pipeline_work
        (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
    VALUES ('fixture_boxscore', 'fixture', p_fixture_id, v_sport, 'pending',
            v_input_version, NOW(), NOW())
    ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
        status        = 'pending',
        attempts      = 0,
        -- FIFO preservation (mig 225, mirrors work.rs/work.go).
        available_at  = CASE WHEN public.pipeline_work.status = 'pending'
                             THEN public.pipeline_work.available_at
                             ELSE NOW() END,
        updated_at    = NOW(),
        last_error    = NULL,
        input_version = EXCLUDED.input_version
    WHERE public.pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
       OR public.pipeline_work.status = 'failed';

    PERFORM pg_notify('pipeline_work_ready', '');
    RETURN true;
END;
$$;
