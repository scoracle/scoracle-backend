-- 240_monday_weeks.sql
--
-- Scott, 2026-09-05: "week one [is] match week one for each sport — rather than
-- Thursday to Thursday like the NFL would have"; boundary decision: MONDAY.
-- Week 1 is the Monday-start calendar week CONTAINING opening day, so the
-- numbering aligns with the sport's match weeks while every week everywhere
-- turns on the same day: Monday 00:00 ET. (The one nuance, accepted with the
-- decision: an NFL Monday-night game and its Tuesday coverage file into the
-- FOLLOWING week — the game travels with its stories.)
--
-- Since every anchor is now Monday-aligned, all season boundaries land on week
-- multiples of each other; date_trunc('week', …) is PostgreSQL's ISO Monday.

BEGIN;

CREATE OR REPLACE FUNCTION public.rebuild_season_weeks(p_sport text) RETURNS integer
LANGUAGE plpgsql
AS $$
DECLARE
    v_rows integer := 0;
BEGIN
    DELETE FROM public.season_weeks WHERE sport = p_sport;

    WITH bound AS (
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
    anchors AS (
        -- The Monday (ET) of the week containing opening day (mig 240).
        SELECT COALESCE(b.season, u.season) AS season,
               date_trunc('week', COALESCE(b.opens, u.opens) AT TIME ZONE 'America/New_York')
                   AT TIME ZONE 'America/New_York' AS opens_at
        FROM unbound u
        FULL OUTER JOIN bound b ON b.season = u.season
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

SELECT 'FOOTBALL' AS sport, rebuild_season_weeks('FOOTBALL') AS weeks
UNION ALL SELECT 'NBA', rebuild_season_weeks('NBA')
UNION ALL SELECT 'NFL', rebuild_season_weeks('NFL');

SELECT restamp_card_weeks() AS restamped;

COMMIT;
