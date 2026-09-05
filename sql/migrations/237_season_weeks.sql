-- 237_season_weeks.sql
--
-- The week becomes the product's clock (PLAN-weekly-fantasy-rail Phase B; Scott,
-- 2026-09-04): "we're going to go off of the actual 7 day week. Each sport will
-- have a different week one… a new, round-the-year reporting cycle."
--
-- Three pieces:
--
--   1. `season_weeks` — the calendar. Week 1 opens 00:00 America/New_York on the
--      sport's opening day (the season's first fixture — and we now OWN fixtures,
--      the schedule import is their authority); strict 7-day blocks, running
--      round-the-year until the next season's opening day re-anchors week 1.
--      Rebuilt idempotently from fixtures by `rebuild_season_weeks`, so a new
--      season's calendar appears the night its schedule lands. One timezone
--      everywhere: ET, matching cron — this retires the ET/UTC/server-local
--      day-boundary patchwork by giving every consumer one table to key on.
--
--   2. Week stamps. Every voice-product table (and both rating snapshots) gains
--      (week_season, week_no), filled by a BEFORE INSERT trigger from
--      generated_at — derived in DB code once, never computed by a junction or a
--      model (directing doctrine). Named apart from the existing `season` columns
--      (sport-season of the SUBJECT) though they agree by construction; the week
--      key says which SHELF the generation files on. History is backfilled, so
--      every past card is browsable by real week immediately.
--
--   3. `week_seals` — the closure ledger for the culmination pass (the Desk's
--      seal task, Rust side): one row per (sport, week) that has been closed out.
--      Cards keep evolving inside an open week; a sealed week is immutable and
--      its latest generation IS the wrap-up.

BEGIN;

-- (1) The calendar ----------------------------------------------------------

CREATE TABLE public.season_weeks (
    sport     text NOT NULL,
    season    integer NOT NULL,
    week_no   integer NOT NULL CHECK (week_no BETWEEN 1 AND 60),
    starts_at timestamptz NOT NULL,
    ends_at   timestamptz NOT NULL,
    PRIMARY KEY (sport, season, week_no),
    CHECK (ends_at = starts_at + interval '7 days')
);

CREATE INDEX idx_season_weeks_lookup ON public.season_weeks (sport, starts_at);

COMMENT ON TABLE public.season_weeks IS
    'The reporting calendar (mig 237): week 1 = 00:00 ET on the sport''s opening day (first fixture of the season), 7-day blocks round-the-year until the next season re-anchors. Derived data — rebuilt nightly by rebuild_season_weeks(); never hand-edited.';

CREATE FUNCTION public.rebuild_season_weeks(p_sport text) RETURNS integer
LANGUAGE plpgsql
AS $$
DECLARE
    v_rows integer := 0;
BEGIN
    -- Anchors: one per season with a real fixture population (>= 5 dodges junk
    -- nominations); opening day at 00:00 ET. The latest season extends 53 weeks
    -- so "current week" always resolves during the round-the-year stretch; an
    -- earlier season runs exactly until its successor's anchor.
    WITH anchors AS (
        SELECT season,
               date_trunc('day', MIN(start_time) AT TIME ZONE 'America/New_York')
                   AT TIME ZONE 'America/New_York' AS opens_at
        FROM public.fixtures
        WHERE sport = p_sport
        GROUP BY season
        HAVING COUNT(*) >= 5
    ),
    spans AS (
        SELECT season, opens_at,
               COALESCE(LEAD(opens_at) OVER (ORDER BY season),
                        opens_at + interval '53 weeks') AS closes_at
        FROM anchors
    ),
    weeks AS (
        SELECT s.season,
               gs.n::integer AS week_no,
               s.opens_at + (gs.n - 1) * interval '7 days' AS starts_at
        FROM spans s
        CROSS JOIN LATERAL generate_series(1, 60) gs(n)
        WHERE s.opens_at + (gs.n - 1) * interval '7 days' < s.closes_at
    ),
    replaced AS (
        DELETE FROM public.season_weeks WHERE sport = p_sport
    )
    INSERT INTO public.season_weeks (sport, season, week_no, starts_at, ends_at)
    SELECT p_sport, season, week_no, starts_at, starts_at + interval '7 days'
    FROM weeks;
    GET DIAGNOSTICS v_rows = ROW_COUNT;
    RETURN v_rows;
END;
$$;

CREATE FUNCTION public.week_of(p_sport text, p_ts timestamptz)
RETURNS TABLE(week_season integer, week_no integer)
LANGUAGE sql STABLE
AS $$
    SELECT sw.season, sw.week_no
    FROM public.season_weeks sw
    WHERE sw.sport = p_sport AND p_ts >= sw.starts_at AND p_ts < sw.ends_at
    ORDER BY sw.season DESC, sw.week_no DESC
    LIMIT 1;
$$;

SELECT 'FOOTBALL' AS sport, rebuild_season_weeks('FOOTBALL') AS weeks
UNION ALL SELECT 'NBA', rebuild_season_weeks('NBA')
UNION ALL SELECT 'NFL', rebuild_season_weeks('NFL');

-- (2) Week stamps -----------------------------------------------------------

CREATE FUNCTION public.stamp_card_week() RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.week_no IS NULL THEN
        SELECT w.week_season, w.week_no INTO NEW.week_season, NEW.week_no
        FROM public.week_of(NEW.sport, COALESCE(NEW.generated_at, NOW())) w;
    END IF;
    RETURN NEW;
END;
$$;

DO $$
DECLARE
    t text;
BEGIN
    FOREACH t IN ARRAY ARRAY[
        'news_summaries', 'vibe_scores', 'insider_scores', 'transfer_rumors',
        'momentum_summaries', 'stat_summaries', 'sigil_synthesis', 'oracle_readings',
        'rating_history', 'momentum_scores'
    ] LOOP
        EXECUTE format('ALTER TABLE public.%I ADD COLUMN week_season integer, ADD COLUMN week_no integer', t);
        EXECUTE format('CREATE TRIGGER stamp_week_on_insert BEFORE INSERT ON public.%I FOR EACH ROW EXECUTE FUNCTION public.stamp_card_week()', t);
        -- Backfill history onto the new grid, one set-based pass per table.
        EXECUTE format($f$
            UPDATE public.%I x
               SET week_season = sw.season, week_no = sw.week_no
              FROM public.season_weeks sw
             WHERE sw.sport = x.sport
               AND x.generated_at >= sw.starts_at AND x.generated_at < sw.ends_at
               AND x.week_no IS NULL
        $f$, t);
        EXECUTE format('CREATE INDEX idx_%s_week ON public.%I (sport, week_season, week_no)', t, t);
    END LOOP;
END;
$$;

-- (3) The closure ledger ----------------------------------------------------

CREATE TABLE public.week_seals (
    sport       text NOT NULL,
    week_season integer NOT NULL,
    week_no     integer NOT NULL,
    sealed_at   timestamptz NOT NULL DEFAULT NOW(),
    entities_resealed integer NOT NULL DEFAULT 0,
    PRIMARY KEY (sport, week_season, week_no)
);

COMMENT ON TABLE public.week_seals IS
    'The culmination ledger (mig 237): one row per closed (sport, week). Written by the Desk''s seal task after it enqueues the wrap-up pass; idempotence key for the hourly check. An empty week seals as a no-op — nothing generated, nothing to close, the week renders empty by design.';

COMMIT;
