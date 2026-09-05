-- 241_week_seal.sql
--
-- The culmination pass (Phase B3): "narratives and vibes and transfers all build
-- during the week, and then culminate and wrap up" (Scott, 2026-09-04).
--
-- The shape avoids back-dating entirely. The closing pass runs in the week's
-- FINAL SIX HOURS (Sunday evening ET, every sport, every week — the boundary is
-- Monday 00:00 since mig 240): every seat that spoke during the week is
-- re-enqueued once with a seal input_version, takes one fresh look at the FULL
-- week, and its generation files inside the closing week naturally — because it
-- happens inside it. Junction-internal content debounce then does the honest
-- thing for free: a seat whose material didn't move since its last generation
-- skips (its latest generation already IS the wrap-up — "a sealed week's latest
-- generation is the wrap"), and only seats with un-narrated late material spend
-- a model call. An empty week encloses nothing and seals as a no-op.
--
-- At the boundary the week seals: week_seals.sealed_at stamps, the /weeks
-- payload flips `sealed`, and (Phase C) sealed weeks become cache-forever.
-- History seals in the same sweep — every fully-elapsed week is, by
-- definition, closed.
--
-- Driven hourly by the Desk (worker.rs, same slot as the dormancy sweep —
-- deterministic SQL, zero model calls in the check itself).

BEGIN;

ALTER TABLE public.week_seals
    ALTER COLUMN sealed_at DROP NOT NULL,
    ALTER COLUMN sealed_at DROP DEFAULT,
    ADD COLUMN closing_enqueued_at timestamptz;

COMMENT ON COLUMN public.week_seals.closing_enqueued_at IS
    'When the closing pass (final 6h of the week) enqueued the week''s active seats for their wrap look. NULL for historical weeks sealed retroactively and for empty weeks.';

CREATE FUNCTION public.seal_weeks(p_sport text)
RETURNS TABLE(closing_enqueued integer, weeks_sealed integer)
LANGUAGE plpgsql
AS $$
DECLARE
    v_enqueued integer := 0;
    v_sealed integer := 0;
    v_week RECORD;
BEGIN
    -- (1) The closing pass: the current week, inside its final six hours, not
    -- yet passed. One pass per week — closing_enqueued_at is the idempotence key.
    SELECT sw.season, sw.week_no, sw.starts_at, sw.ends_at
      INTO v_week
      FROM public.season_weeks sw
     WHERE sw.sport = p_sport
       AND NOW() >= sw.ends_at - interval '6 hours'
       AND NOW() < sw.ends_at
       AND NOT EXISTS (
           SELECT 1 FROM public.week_seals ws
           WHERE ws.sport = p_sport AND ws.week_season = sw.season
             AND ws.week_no = sw.week_no AND ws.closing_enqueued_at IS NOT NULL)
     LIMIT 1;

    IF FOUND THEN
        WITH seats AS (
            -- Every (stage, entity) that generated inside the closing week. The
            -- stage names mirror the seat→table map the archive uses.
            SELECT 'narratives' AS stage, entity_type, entity_id FROM public.news_summaries
             WHERE sport = p_sport AND week_season = v_week.season AND week_no = v_week.week_no
            UNION
            SELECT 'vibe', entity_type, entity_id FROM public.vibe_scores
             WHERE sport = p_sport AND week_season = v_week.season AND week_no = v_week.week_no
            UNION
            SELECT 'transfers', entity_type, entity_id FROM public.insider_scores
             WHERE sport = p_sport AND week_season = v_week.season AND week_no = v_week.week_no
            UNION
            SELECT 'momentum', entity_type, entity_id FROM public.momentum_summaries
             WHERE sport = p_sport AND week_season = v_week.season AND week_no = v_week.week_no
            UNION
            SELECT 'sigil', entity_type, entity_id FROM public.sigil_synthesis
             WHERE sport = p_sport AND week_season = v_week.season AND week_no = v_week.week_no
            UNION
            SELECT 'rating', entity_type, entity_id FROM public.stat_summaries
             WHERE sport = p_sport AND week_season = v_week.season AND week_no = v_week.week_no
        ),
        enq AS (
            INSERT INTO public.pipeline_work
                (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
            SELECT s.stage, s.entity_type, s.entity_id, p_sport, 'pending',
                   'seal:' || v_week.season || '-' || v_week.week_no, NOW(), NOW()
              FROM seats s
             WHERE s.entity_type IN ('player', 'team')
            ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
                status        = 'pending',
                attempts      = 0,
                available_at  = CASE WHEN public.pipeline_work.status = 'pending'
                                     THEN public.pipeline_work.available_at
                                     ELSE NOW() END,
                updated_at    = NOW(),
                last_error    = NULL,
                input_version = EXCLUDED.input_version
            WHERE public.pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
               OR public.pipeline_work.status = 'failed'
            RETURNING entity_id
        )
        SELECT COUNT(*)::integer INTO v_enqueued FROM enq;

        INSERT INTO public.week_seals (sport, week_season, week_no, closing_enqueued_at, entities_resealed)
        VALUES (p_sport, v_week.season, v_week.week_no, NOW(), v_enqueued)
        ON CONFLICT (sport, week_season, week_no) DO UPDATE SET
            closing_enqueued_at = COALESCE(public.week_seals.closing_enqueued_at, NOW()),
            entities_resealed   = EXCLUDED.entities_resealed;

        IF v_enqueued > 0 THEN
            PERFORM pg_notify('pipeline_work_ready', '');
        END IF;
    END IF;

    -- (2) The boundary: every fully-elapsed week seals. History and empty weeks
    -- included — closed is closed.
    WITH sealed AS (
        INSERT INTO public.week_seals (sport, week_season, week_no, sealed_at)
        SELECT sw.sport, sw.season, sw.week_no, NOW()
          FROM public.season_weeks sw
         WHERE sw.sport = p_sport AND sw.ends_at <= NOW()
        ON CONFLICT (sport, week_season, week_no) DO UPDATE SET
            sealed_at = COALESCE(public.week_seals.sealed_at, NOW())
        WHERE public.week_seals.sealed_at IS NULL
        RETURNING 1
    )
    SELECT COUNT(*)::integer INTO v_sealed FROM sealed;

    RETURN QUERY SELECT v_enqueued, v_sealed;
END;
$$;

COMMENT ON FUNCTION public.seal_weeks(text) IS
    'The B3 seal, hourly from the Desk: inside a week''s final 6 hours, re-enqueue every seat that generated that week (seal:SEASON-WK input_version; content debounce skips the unchanged); at the boundary, stamp sealed_at for every elapsed week. Idempotent on both phases.';

COMMIT;
