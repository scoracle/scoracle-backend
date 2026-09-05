-- 242_momentum_weekly.sql
--
-- Phase B4: momentum re-windows onto the reporting calendar. "Momentum would
-- track the weekly outputs" (Scott, 2026-09-04) — both slopes become
-- week-over-week comparisons of WEEKLY AGGREGATES instead of first-vs-last
-- raw samples inside a rolling window:
--
--   * Vibe slope — the entity's average sentiment per reporting week, newest
--     week's average minus oldest, over the current week + its two
--     predecessors (the same 21-day span as before, aligned to the grid).
--     Needs two active weeks: week-over-week is not defined inside one week.
--   * Rating slope — the entity's average per-event rating per reporting week,
--     over its last momentum_week_window(sport) weeks WITH EVENTS (NBA 3 /
--     NFL 2 / FOOTBALL 4 — the mig-025 ~10% schedule restated in weeks).
--     Weeks without events DROP OUT of the window rather than zeroing it —
--     the mig 130 bye-week lesson, kept. Two active weeks minimum.
--
-- Averaging per week before differencing is the point: a weekly cycle
-- compares week-states, not whichever single game or snapshot happened to sit
-- at the window's edges. Everything downstream is untouched — same output
-- shape, same signed momentum_score, same snapshot table; the Analyst
-- inherits the window from the snapshot as always. season_bridge_window
-- stays as-is for the Scout's per-game trajectory.

BEGIN;

CREATE FUNCTION public.momentum_week_window(p_sport text) RETURNS integer
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $$
    SELECT CASE upper(p_sport)
        WHEN 'NBA' THEN 3
        WHEN 'NFL' THEN 2
        ELSE 4
    END;
$$;

COMMENT ON FUNCTION public.momentum_week_window(text) IS
    'The momentum rating lookback in REPORTING WEEKS (mig 242): NBA 3 / NFL 2 / FOOTBALL 4 — season_bridge_window''s ~10%-of-season schedule restated on the season_weeks grid. Weeks without events drop out of the window (the mig 130 bye-week rule).';

CREATE OR REPLACE FUNCTION public.refresh_momentum_scores(p_sport text DEFAULT NULL::text) RETURNS integer
    LANGUAGE plpgsql
    AS $$
DECLARE
    inserted_count INTEGER;
BEGIN
    -- Single-flight: the NOTIFY listener and the catch-up ticker can race a
    -- drain for the same sport. The loser returns NULL (NOT 0) — the Go drain
    -- leaves the dirty marker in place on NULL so the refresh is retried,
    -- never double-appended and never silently lost.
    IF NOT pg_try_advisory_xact_lock(hashtext('refresh_momentum_scores')) THEN
        RETURN NULL;
    END IF;

    WITH target_sports AS (
        SELECT id AS sport, current_season
        FROM public.sports
        WHERE p_sport IS NULL OR id = upper(p_sport)
    ),
    -- The vibe window: the current reporting week + its two predecessors (the
    -- old 21 calendar days, aligned to the grid). Sentiment flows through the
    -- offseason, so this clock never pauses with the fixture calendar.
    recent_weeks AS (
        SELECT sw.sport, sw.season, sw.week_no, sw.starts_at
        FROM public.season_weeks sw
        JOIN target_sports ts ON ts.sport = sw.sport
        WHERE sw.starts_at <= NOW()
          AND sw.ends_at > NOW() - INTERVAL '21 days'
    ),
    vibe_weekly AS (
        SELECT v.entity_type, v.entity_id, v.sport,
               rw.starts_at AS wk_start,
               avg(v.sentiment)::numeric AS wk_avg,
               count(*)::int AS n,
               min(v.generated_at) AS wmin,
               max(v.generated_at) AS wmax
        FROM public.vibe_scores v
        JOIN recent_weeks rw
          ON rw.sport = v.sport AND rw.season = v.week_season AND rw.week_no = v.week_no
        WHERE v.sentiment IS NOT NULL
        GROUP BY v.entity_type, v.entity_id, v.sport, rw.starts_at
    ),
    vibe AS (
        SELECT entity_type, entity_id, sport,
               ((array_agg(wk_avg ORDER BY wk_start DESC))[1]
                - (array_agg(wk_avg ORDER BY wk_start ASC))[1])::numeric AS vibe_slope,
               sum(n)::int AS vibe_samples,
               min(wmin) AS vibe_window_start,
               max(wmax) AS vibe_window_end
        FROM vibe_weekly
        GROUP BY entity_type, entity_id, sport
        HAVING sum(n) >= 3 AND count(*) >= 2
    ),
    -- The rating lookback: the entity's last momentum_week_window(sport)
    -- reporting weeks WITH events, averaged per week, across (current,
    -- previous) seasons so the lookback closes at a season's end and resumes
    -- at the next season's first game.
    player_week AS (
        SELECT e.player_id AS entity_id, e.sport, e.season,
               sw.starts_at AS wk_start,
               avg(e.rating_pct)::numeric AS wk_avg,
               count(*)::int AS n,
               min(f.start_time) AS wmin,
               max(f.start_time) AS wmax
        FROM public.event_box_scores e
        JOIN public.fixtures f ON f.id = e.fixture_id
        JOIN target_sports ts ON ts.sport = e.sport
        JOIN public.season_weeks sw
          ON sw.sport = e.sport AND f.start_time >= sw.starts_at AND f.start_time < sw.ends_at
        WHERE e.rating_pct IS NOT NULL
          AND e.season IN (ts.current_season, ts.current_season - 1)
        GROUP BY e.player_id, e.sport, e.season, sw.starts_at
    ),
    player_ranked AS (
        SELECT pw.*,
               row_number() OVER (PARTITION BY pw.entity_id, pw.sport ORDER BY pw.wk_start DESC) AS rn
        FROM player_week pw
    ),
    team_week AS (
        SELECT e.team_id AS entity_id, e.sport, e.season,
               sw.starts_at AS wk_start,
               avg(e.rating_pct)::numeric AS wk_avg,
               count(*)::int AS n,
               min(f.start_time) AS wmin,
               max(f.start_time) AS wmax
        FROM public.event_team_stats e
        JOIN public.fixtures f ON f.id = e.fixture_id
        JOIN target_sports ts ON ts.sport = e.sport
        JOIN public.season_weeks sw
          ON sw.sport = e.sport AND f.start_time >= sw.starts_at AND f.start_time < sw.ends_at
        WHERE e.rating_pct IS NOT NULL
          AND e.season IN (ts.current_season, ts.current_season - 1)
        GROUP BY e.team_id, e.sport, e.season, sw.starts_at
    ),
    team_ranked AS (
        SELECT tw.*,
               row_number() OVER (PARTITION BY tw.entity_id, tw.sport ORDER BY tw.wk_start DESC) AS rn
        FROM team_week tw
    ),
    player_rating AS (
        SELECT 'player'::text AS entity_type, pr.entity_id, pr.sport,
               max(pr.season) AS season,
               ((array_agg(pr.wk_avg ORDER BY pr.wk_start DESC))[1]
                - (array_agg(pr.wk_avg ORDER BY pr.wk_start ASC))[1])::numeric AS rating_slope,
               sum(pr.n)::int AS rating_samples,
               min(pr.wmin) AS rating_window_start,
               max(pr.wmax) AS rating_window_end
        FROM player_ranked pr
        WHERE pr.rn <= public.momentum_week_window(pr.sport)
        GROUP BY pr.entity_id, pr.sport
        HAVING count(*) >= 2
    ),
    team_rating AS (
        SELECT 'team'::text AS entity_type, tr.entity_id, tr.sport,
               max(tr.season) AS season,
               ((array_agg(tr.wk_avg ORDER BY tr.wk_start DESC))[1]
                - (array_agg(tr.wk_avg ORDER BY tr.wk_start ASC))[1])::numeric AS rating_slope,
               sum(tr.n)::int AS rating_samples,
               min(tr.wmin) AS rating_window_start,
               max(tr.wmax) AS rating_window_end
        FROM team_ranked tr
        WHERE tr.rn <= public.momentum_week_window(tr.sport)
        GROUP BY tr.entity_id, tr.sport
        HAVING count(*) >= 2
    ),
    rating AS (
        SELECT * FROM player_rating
        UNION ALL
        SELECT * FROM team_rating
    ),
    entity_scores AS (
        SELECT COALESCE(v.entity_type, r.entity_type) AS entity_type,
               COALESCE(v.entity_id, r.entity_id) AS entity_id,
               COALESCE(v.sport, r.sport) AS sport,
               r.season,
               v.vibe_slope, COALESCE(v.vibe_samples, 0) AS vibe_samples,
               v.vibe_window_start, v.vibe_window_end,
               r.rating_slope, COALESCE(r.rating_samples, 0) AS rating_samples,
               r.rating_window_start, r.rating_window_end
        FROM vibe v
        FULL OUTER JOIN rating r
          ON r.entity_type = v.entity_type
         AND r.entity_id = v.entity_id
         AND r.sport = v.sport
    ),
    enriched AS (
        SELECT es.sport, es.entity_type, es.entity_id,
               COALESCE(es.season, ts.current_season) AS season,
               pci.league_id, pci.team_id, pci.position,
               COALESCE(pci.position_group, public.position_group(es.sport, pci.position)) AS position_group,
               t.conference, t.division,
               es.vibe_slope, es.vibe_samples, es.vibe_window_start, es.vibe_window_end,
               es.rating_slope, es.rating_samples, es.rating_window_start, es.rating_window_end
        FROM entity_scores es
        JOIN target_sports ts ON ts.sport = es.sport
        LEFT JOIN public.player_current_identity pci
          ON pci.player_id = es.entity_id AND pci.sport = es.sport AND es.entity_type = 'player'
        LEFT JOIN public.teams t
          ON t.id = pci.team_id AND t.sport = es.sport
        WHERE es.entity_type = 'player'

        UNION ALL

        SELECT es.sport, es.entity_type, es.entity_id,
               COALESCE(es.season, ts.current_season) AS season,
               tm.league_id, tm.id AS team_id, NULL::text AS position, NULL::text AS position_group,
               tm.conference, tm.division,
               es.vibe_slope, es.vibe_samples, es.vibe_window_start, es.vibe_window_end,
               es.rating_slope, es.rating_samples, es.rating_window_start, es.rating_window_end
        FROM entity_scores es
        JOIN target_sports ts ON ts.sport = es.sport
        JOIN public.teams tm
          ON tm.id = es.entity_id AND tm.sport = es.sport
        WHERE es.entity_type = 'team'
    )
    INSERT INTO public.momentum_scores (
        sport, entity_type, entity_id, season, league_id, team_id, position, position_group,
        conference, division, vibe_slope, vibe_samples, vibe_window_start, vibe_window_end,
        rating_slope, rating_samples, rating_window_start, rating_window_end, momentum_score
    )
    SELECT sport, entity_type, entity_id, season, league_id, team_id, position, position_group,
           conference, division,
           round(vibe_slope, 3), vibe_samples, vibe_window_start, vibe_window_end,
           round(rating_slope, 3), rating_samples, rating_window_start, rating_window_end,
           -- SIGNED: the average of the present slopes, sign preserved. Falls
           -- are as much a historic datapoint as rises — this number is the
           -- durable per-snapshot momentum record, so it must not clamp
           -- downside.
           round((
               COALESCE(vibe_slope, 0) + COALESCE(rating_slope, 0)
           ) / NULLIF(
               (CASE WHEN vibe_slope IS NULL THEN 0 ELSE 1 END)
               + (CASE WHEN rating_slope IS NULL THEN 0 ELSE 1 END),
               0
           ), 3) AS momentum_score
    FROM enriched
    WHERE vibe_slope IS NOT NULL OR rating_slope IS NOT NULL;

    GET DIAGNOSTICS inserted_count = ROW_COUNT;

    RETURN inserted_count;
END;
$$;

COMMIT;
