-- 239_unbound_season_floor.sql
--
-- Second verification pass on the mig 237/238 calendar (2026-09-05): NFL now
-- anchors correctly (bound fixtures, exact schedule dates), but NBA resolved
-- "season 2026, week 5" off 85 news-nominated August fixtures — summer noise;
-- the real season is ten weeks away — and FOOTBALL's anchor sits on Aug 3
-- friendlies. Both are UNBOUND sports (no import rail yet), where the news's
-- memory is the only fixture source and it starts covering a season before the
-- season starts.
--
-- The rule, completed: a BOUND season anchors from the feed's schedule at >= 30
-- fixtures (exact opening day). An UNBOUND season must look like a season —
-- >= 100 fixtures — before it may anchor at all (no real season has fewer;
-- NBA 1,230 / NFL 285 / PL 380). Until then the prior season's round-the-year
-- tail carries, which is also the truthful reading of an offseason. When a
-- sport's rail lands, bindings appear and the exact anchor takes over — the
-- calendar self-corrects the night the schedule imports.
--
-- The re-stamp becomes a durable function while we are here: this is the
-- second calendar correction to need it, and it will not be the last time the
-- grid moves (every future rail landing re-anchors its sport).

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
        SELECT COALESCE(b.season, u.season) AS season,
               date_trunc('day', COALESCE(b.opens, u.opens) AT TIME ZONE 'America/New_York')
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

-- The durable re-stamp: overwrite every card/snapshot week key from the
-- current grid, clearing stamps the grid no longer covers. Call after ANY
-- calendar rebuild that can move existing weeks (rail landings re-anchor).
CREATE FUNCTION public.restamp_card_weeks() RETURNS integer
LANGUAGE plpgsql
AS $$
DECLARE
    t text;
    v_total integer := 0;
    v_rows integer;
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
        GET DIAGNOSTICS v_rows = ROW_COUNT;
        v_total := v_total + v_rows;
        EXECUTE format($f$
            UPDATE public.%I x
               SET week_season = NULL, week_no = NULL
             WHERE x.week_no IS NOT NULL
               AND NOT EXISTS (
                   SELECT 1 FROM public.season_weeks sw
                   WHERE sw.sport = x.sport
                     AND x.generated_at >= sw.starts_at AND x.generated_at < sw.ends_at)
        $f$, t);
        GET DIAGNOSTICS v_rows = ROW_COUNT;
        v_total := v_total + v_rows;
    END LOOP;
    RETURN v_total;
END;
$$;

SELECT 'FOOTBALL' AS sport, rebuild_season_weeks('FOOTBALL') AS weeks
UNION ALL SELECT 'NBA', rebuild_season_weeks('NBA')
UNION ALL SELECT 'NFL', rebuild_season_weeks('NFL');

SELECT restamp_card_weeks() AS restamped;

COMMIT;
