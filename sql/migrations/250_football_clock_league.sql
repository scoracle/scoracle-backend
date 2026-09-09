-- 250_football_clock_league.sql
--
-- Scott, 2026-09-08: "The weeks in the NavRail are all off. We're only 3 weeks
-- into the Premier League season, but it's showing as week 6." Verified against
-- the live API the same day: GET /api/v1/football/weeks answers
-- `current: {season: 2026, week: 6}` with week 1 anchored at Mon 2026-08-03.
--
-- Not a frontend bug and not a Monday-boundary bug — an ANCHOR bug, and the
-- last one in a family (237 → 238 → 239 → 240) that kept correcting *which
-- fixtures* may anchor a season while never asking *which competition* the
-- season belongs to.
--
-- FOOTBALL is not a league, it is five of them: Premier League, Bundesliga,
-- Ligue 1, Serie A, La Liga (shared.sql, all is_benchmark). Mig 240 anchors
-- week 1 on the Monday of MIN(start_time) across the sport's whole fixture
-- population, so the EARLIEST-opening league silently sets week 1 for every
-- entity in every one of the five. Football's continental calendar staggers
-- those openers across three or four weeks, so by the time the Premier League
-- kicks off, the sport-wide clock is already several weeks old — exactly the
-- three-week gap Scott is reading on the rail.
--
-- The rule (Scott's decision, 2026-09-08): a sport may nominate ONE league as
-- its clock, and FOOTBALL nominates the Premier League. Week 1 is the Monday
-- (ET) of the week containing THAT league's opening day; the other four ride
-- the same grid. NBA and NFL nominate nothing and are untouched — they are
-- single-competition sports, where the sport-wide MIN already IS the league's
-- opening day.
--
-- The nuance, accepted with the decision (and it is the same shape as mig
-- 240's Monday-night nuance): a Bundesliga match played in the fortnight
-- before the Premier League opens files into the PRIOR season's tail weeks.
-- The grid runs round-the-year and never has a hole, so those weeks exist and
-- are numbered; they just belong to the season that is ending rather than the
-- one that has not started. A reader looking at a Bundesliga player in early
-- August sees late-season weeks, which is what that player's own competition
-- is actually in.
--
-- The clock lives in DATA, not in this function: `sports.clock_league_id`. The
-- next multi-league sport nominates its clock with an UPDATE, not a migration
-- that rewrites plpgsql.
--
-- Additive (new nullable column + CREATE OR REPLACE): apply BEFORE the release,
-- per RUNBOOK §2.

BEGIN;

ALTER TABLE public.sports
    ADD COLUMN IF NOT EXISTS clock_league_id integer REFERENCES public.leagues(id);

COMMENT ON COLUMN public.sports.clock_league_id IS
    'The league whose opening day anchors this sport''s reporting calendar (mig 250). NULL for single-competition sports (NBA, NFL), where the sport-wide first fixture already is the league''s opener. Set for multi-league sports so a staggered continental calendar cannot let the earliest league start everyone''s week 1.';

UPDATE public.sports SET clock_league_id = 8 WHERE id = 'FOOTBALL';

CREATE OR REPLACE FUNCTION public.rebuild_season_weeks(p_sport text) RETURNS integer
LANGUAGE plpgsql
AS $$
DECLARE
    v_rows integer := 0;
    v_clock integer;
BEGIN
    SELECT clock_league_id INTO v_clock FROM public.sports WHERE id = p_sport;

    DELETE FROM public.season_weeks WHERE sport = p_sport;

    WITH clock AS (
        -- The nominated league's own opening day (mig 250). Same >= 30 real-
        -- population floor as `bound`: a junk cluster never mints a season, and
        -- a league with a partial fixture import falls through to the old
        -- rules rather than anchoring the sport on three friendlies.
        SELECT f.season, MIN(f.start_time) AS opens
        FROM public.fixtures f
        WHERE f.sport = p_sport
          AND v_clock IS NOT NULL
          AND f.league_id = v_clock
        GROUP BY f.season
        HAVING COUNT(*) >= 30
    ),
    bound AS (
        SELECT f.season, MIN(f.start_time) AS opens
        FROM public.fixtures f
        WHERE f.sport = p_sport
          AND EXISTS (SELECT 1 FROM public.entity_external_ids x
                      WHERE x.entity_type = 'fixture' AND x.entity_id = f.id)
        GROUP BY f.season
        HAVING COUNT(*) >= 30
    ),
    unbound AS (
        SELECT f.season, MIN(f.start_time) AS opens
        FROM public.fixtures f
        WHERE f.sport = p_sport
        GROUP BY f.season
        HAVING COUNT(*) >= 100
    ),
    -- Every season any rule can speak for. The FULL OUTER JOIN chain mig 240
    -- used doesn't extend to a third source without the join keys going
    -- ambiguous, so the season universe is built once and the three rules are
    -- LEFT JOINed onto it in priority order.
    seasons AS (
        SELECT season FROM clock
        UNION SELECT season FROM bound
        UNION SELECT season FROM unbound
    ),
    anchors AS (
        -- The Monday (ET) of the week containing opening day (mig 240),
        -- where "opening day" is now the CLOCK league's, falling back to the
        -- schedule-authoritative population and then to all fixtures.
        SELECT s.season,
               date_trunc('week',
                   COALESCE(c.opens, b.opens, u.opens) AT TIME ZONE 'America/New_York')
                   AT TIME ZONE 'America/New_York' AS opens_at
        FROM seasons s
        LEFT JOIN clock   c ON c.season = s.season
        LEFT JOIN bound   b ON b.season = s.season
        LEFT JOIN unbound u ON u.season = s.season
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
    )
    INSERT INTO public.season_weeks (sport, season, week_no, starts_at, ends_at)
    SELECT p_sport, season, week_no, starts_at, starts_at + interval '7 days'
    FROM weeks;
    GET DIAGNOSTICS v_rows = ROW_COUNT;
    RETURN v_rows;
END;
$$;

COMMENT ON TABLE public.season_weeks IS
    'The reporting calendar. Week 1 = the Monday 00:00 ET of the week containing opening day (mig 240), where opening day is the sport''s clock league''s first fixture when it nominates one (sports.clock_league_id, mig 250) and its first fixture overall otherwise; 7-day blocks round-the-year until the next season re-anchors. Derived data — rebuilt nightly by rebuild_season_weeks(); never hand-edited.';

-- Every stamp in the DB was laid on the old grid, so the grid and the stamps
-- are rebuilt together, exactly as migs 238/239/240 did.
SELECT 'FOOTBALL' AS sport, rebuild_season_weeks('FOOTBALL') AS weeks
UNION ALL SELECT 'NBA', rebuild_season_weeks('NBA')
UNION ALL SELECT 'NFL', rebuild_season_weeks('NFL');

SELECT restamp_card_weeks() AS restamped;

-- Verification (expect week 1 to open on the Monday of the PL's first
-- fixture, and `current` to be the week we are actually in):
--   SELECT season, week_no, starts_at FROM season_weeks
--    WHERE sport = 'FOOTBALL' AND season = 2026 AND week_no = 1;
--   SELECT MIN(start_time) FROM fixtures
--    WHERE sport = 'FOOTBALL' AND season = 2026 AND league_id = 8;

COMMIT;
