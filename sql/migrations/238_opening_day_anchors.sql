-- 238_opening_day_anchors.sql
--
-- Mig 237's calendar, corrected on first contact with production. Verification
-- (2026-09-05, minutes after apply) showed every sport resolving "current" to
-- season 2026 week 5 — an early-August anchor. Two corruptions, one cause:
-- MIN(start_time) over ALL of a season's fixtures anchors on whatever junk the
-- pre-authority eras left behind (news-nominated NFL preseason games, vendor
-- stubs), and a season with a handful of junk rows anchors a season that has
-- not begun. NFL's real 2026 opening day is Sept 10; the anchor said Aug 6.
--
-- The fix restates the authority rule the import rail established:
--
--   1. A season anchors only with a real fixture population (>= 30 — a junk
--      cluster never mints a season; every true season has hundreds).
--   2. When a season has schedule-authoritative fixtures (an import binding in
--      entity_external_ids: nflverse today, nba/fpl as those rails land), the
--      opening day is MIN over THOSE — the feed's schedule, not the news's
--      memory of preseason. Unbound seasons (FOOTBALL/NBA history) fall back
--      to MIN over all, which is what their eras have.
--
-- The week stamps laid down by mig 237's backfill were computed on the corrupt
-- grid, so after rebuilding the calendar every stamp is re-laid from scratch.

BEGIN;

CREATE OR REPLACE FUNCTION public.rebuild_season_weeks(p_sport text) RETURNS integer
LANGUAGE plpgsql
AS $$
DECLARE
    v_rows integer := 0;
BEGIN
    WITH bound AS (
        SELECT f.season,
               MIN(f.start_time) AS opens,
               COUNT(*) AS n
        FROM public.fixtures f
        WHERE f.sport = p_sport
          AND EXISTS (SELECT 1 FROM public.entity_external_ids x
                      WHERE x.entity_type = 'fixture' AND x.entity_id = f.id)
        GROUP BY f.season
        HAVING COUNT(*) >= 30
    ),
    any_min AS (
        SELECT f.season, MIN(f.start_time) AS opens, COUNT(*) AS n
        FROM public.fixtures f
        WHERE f.sport = p_sport
        GROUP BY f.season
        HAVING COUNT(*) >= 30
    ),
    anchors AS (
        SELECT a.season,
               date_trunc('day', COALESCE(b.opens, a.opens) AT TIME ZONE 'America/New_York')
                   AT TIME ZONE 'America/New_York' AS opens_at
        FROM any_min a
        LEFT JOIN bound b ON b.season = a.season
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

SELECT 'FOOTBALL' AS sport, rebuild_season_weeks('FOOTBALL') AS weeks
UNION ALL SELECT 'NBA', rebuild_season_weeks('NBA')
UNION ALL SELECT 'NFL', rebuild_season_weeks('NFL');

-- Re-lay every stamp on the corrected grid (overwrite, not fill-if-null: the
-- mig 237 backfill wrote keys from the corrupt calendar).
DO $$
DECLARE
    t text;
BEGIN
    FOREACH t IN ARRAY ARRAY[
        'news_summaries', 'vibe_scores', 'insider_scores', 'transfer_rumors',
        'momentum_summaries', 'stat_summaries', 'sigil_synthesis', 'oracle_readings',
        'rating_history', 'momentum_scores'
    ] LOOP
        EXECUTE format($f$
            UPDATE public.%I x
               SET week_season = sw.season, week_no = sw.week_no
              FROM public.season_weeks sw
             WHERE sw.sport = x.sport
               AND x.generated_at >= sw.starts_at AND x.generated_at < sw.ends_at
               AND (x.week_season IS DISTINCT FROM sw.season
                    OR x.week_no IS DISTINCT FROM sw.week_no)
        $f$, t);
        -- Rows the corrected grid no longer covers (pre-anchor history) must
        -- not keep stamps from the corrupt one.
        EXECUTE format($f$
            UPDATE public.%I x
               SET week_season = NULL, week_no = NULL
             WHERE x.week_no IS NOT NULL
               AND NOT EXISTS (
                   SELECT 1 FROM public.season_weeks sw
                   WHERE sw.sport = x.sport
                     AND x.generated_at >= sw.starts_at AND x.generated_at < sw.ends_at)
        $f$, t);
    END LOOP;
END;
$$;

COMMIT;
