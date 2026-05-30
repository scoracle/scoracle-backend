-- 025_cold_start_guard.sql
--
-- Cold-start guard for early-season composites (Delta 1 of the 2026-05-30
-- composite enhancements proposal). When an entity has played only a
-- handful of games in the current season, their season_composite_score
-- is built from a tiny sample of season-aggregated stats and is volatile
-- — a one-game wonder can top the early-season leaderboard.
--
-- Fix: insert a Layer-2.5 step in recalculate_event_percentiles that
-- linearly blends the freshly-computed season_composite_score with a
-- prior-season anchor over the first ~10% of season games. Phase-out is
-- continuous and proportional per sport:
--
--   NBA      8 games  (10% of 82)
--   NFL      2 games  (10% of 17)
--   FOOTBALL 4 games  (10% of 38)
--
--   alpha = MAX(0, (window - games)/window)            -- decays 1 → 0
--   blended = alpha * prior + (1 - alpha) * current
--
-- Effect: the early-season leaderboard opens as last season's standings
-- and morphs continuously into this season's by the window boundary. No
-- jump at any game count.
--
-- Prior-anchor fallback chain (most-specific → most-general):
--   1. The entity's own prev-season season_composite_score (same league for
--      football; league_id changes flip to step 2).
--   2. The prev-season cohort average for sport+position (players) or
--      sport (teams).
--   3. 50.0 — first season in the DB / no prior cohort data.
--
-- Lives entirely inside the existing within-season write footprint —
-- previous seasons are read-only references, consistent with v1's
-- "frozen history" invariant.
--
-- The blend runs AFTER Layer 2 (season_composite_score) and BEFORE
-- Layer 3 (season_composite_rank), so the in-season rank is computed
-- from the blended composite. Layer 4 (all-time rank, separate fn) also
-- reads the blended value naturally on its next nightly run.

BEGIN;

CREATE OR REPLACE FUNCTION recalculate_event_percentiles(
    p_sport TEXT,
    p_season INTEGER
)
RETURNS TABLE (player_events_updated INTEGER, team_events_updated INTEGER) AS $$
DECLARE
    v_player_events INTEGER := 0;
    v_team_events INTEGER := 0;
    v_window INTEGER := CASE p_sport
        WHEN 'NBA'      THEN 8
        WHEN 'NFL'      THEN 2
        WHEN 'FOOTBALL' THEN 4
        ELSE 10
    END;
BEGIN
    -- ============================================================
    -- Reset stale event state for the (sport, season).
    -- ============================================================
    UPDATE event_box_scores SET composite_score = NULL, percentiles = '{}'::jsonb
        WHERE sport = p_sport AND season = p_season AND composite_score IS NOT NULL;
    UPDATE event_team_stats SET composite_score = NULL, percentiles = '{}'::jsonb
        WHERE sport = p_sport AND season = p_season AND composite_score IS NOT NULL;

    -- ============================================================
    -- PLAYER EVENTS  (Layer 1 — sparkline source)
    -- ============================================================
    WITH eligible AS (
        SELECT key_name, is_inverse, unit FROM stat_definitions
        WHERE sport = p_sport AND entity_type = 'player' AND is_percentile_eligible = true
    ),
    expanded AS (
        SELECT e.id AS event_id, COALESCE(e.position, ps.position, 'Unknown') AS position,
               ek.key_name AS stat_key, ek.is_inverse, (e.stats->>ek.key_name)::numeric AS stat_value
        FROM event_box_scores e CROSS JOIN eligible ek
        LEFT JOIN player_stats ps ON ps.player_id = e.player_id AND ps.sport = e.sport
              AND ps.season = e.season AND COALESCE(ps.league_id, 0) = COALESCE(e.league_id, 0)
        WHERE e.sport = p_sport AND e.season = p_season
          AND e.stats ? ek.key_name AND jsonb_typeof(e.stats -> ek.key_name) = 'number'
          AND (e.stats->>ek.key_name)::numeric != 0
          AND (ek.unit <> 'rate_pct' OR (e.stats->>ek.key_name)::numeric BETWEEN 0 AND 100)
    ),
    ranked AS (
        SELECT event_id, position, stat_key,
            CASE WHEN is_inverse
                THEN ROUND((1.0 - percent_rank() OVER (PARTITION BY position, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
                ELSE ROUND((percent_rank() OVER (PARTITION BY position, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
            END AS percentile,
            COUNT(*) OVER (PARTITION BY position, stat_key) AS sample_size
        FROM expanded
    ),
    per_event AS (
        SELECT event_id, MAX(position) AS position_group, jsonb_object_agg(stat_key, percentile) AS pct_only,
               MAX(sample_size) AS sample_size, ROUND(AVG(percentile), 1) AS raw_composite
        FROM ranked GROUP BY event_id
    ),
    normalized AS (
        SELECT event_id, position_group, pct_only, sample_size,
               ROUND((percent_rank() OVER (PARTITION BY position_group ORDER BY raw_composite ASC))::numeric * 100, 1) AS composite_score
        FROM per_event WHERE raw_composite IS NOT NULL
    )
    UPDATE event_box_scores ebs
        SET percentiles = nm.pct_only || jsonb_build_object('_position_group', nm.position_group, '_sample_size', nm.sample_size),
            composite_score = nm.composite_score
        FROM normalized nm WHERE ebs.id = nm.event_id;
    GET DIAGNOSTICS v_player_events = ROW_COUNT;

    -- ============================================================
    -- TEAM EVENTS  (Layer 1)
    -- ============================================================
    WITH eligible AS (
        SELECT key_name, is_inverse, unit FROM stat_definitions
        WHERE sport = p_sport AND entity_type = 'team' AND is_percentile_eligible = true
    ),
    expanded AS (
        SELECT e.id AS event_id, ek.key_name AS stat_key, ek.is_inverse, (e.stats->>ek.key_name)::numeric AS stat_value
        FROM event_team_stats e CROSS JOIN eligible ek
        WHERE e.sport = p_sport AND e.season = p_season
          AND e.stats ? ek.key_name AND jsonb_typeof(e.stats -> ek.key_name) = 'number'
          AND (e.stats->>ek.key_name)::numeric != 0
          AND (ek.unit <> 'rate_pct' OR (e.stats->>ek.key_name)::numeric BETWEEN 0 AND 100)
    ),
    ranked AS (
        SELECT event_id, stat_key,
            CASE WHEN is_inverse
                THEN ROUND((1.0 - percent_rank() OVER (PARTITION BY stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
                ELSE ROUND((percent_rank() OVER (PARTITION BY stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
            END AS percentile,
            COUNT(*) OVER (PARTITION BY stat_key) AS sample_size
        FROM expanded
    ),
    per_event AS (
        SELECT event_id, jsonb_object_agg(stat_key, percentile) AS pct_only,
               MAX(sample_size) AS sample_size, ROUND(AVG(percentile), 1) AS raw_composite
        FROM ranked GROUP BY event_id
    ),
    normalized AS (
        SELECT event_id, pct_only, sample_size,
               ROUND((percent_rank() OVER (ORDER BY raw_composite ASC))::numeric * 100, 1) AS composite_score
        FROM per_event WHERE raw_composite IS NOT NULL
    )
    UPDATE event_team_stats ets
        SET percentiles = nm.pct_only || jsonb_build_object('_sample_size', nm.sample_size),
            composite_score = nm.composite_score
        FROM normalized nm WHERE ets.id = nm.event_id;
    GET DIAGNOSTICS v_team_events = ROW_COUNT;

    -- ============================================================
    -- Layer 2: season_composite_score = AVG of season per-stat percentiles
    -- ============================================================
    UPDATE player_stats SET season_composite_score = NULL
        WHERE sport = p_sport AND season = p_season AND season_composite_score IS NOT NULL;
    UPDATE team_stats SET season_composite_score = NULL
        WHERE sport = p_sport AND season = p_season AND season_composite_score IS NOT NULL;

    UPDATE player_stats ps SET season_composite_score = sub.avg_pct
        FROM (
            SELECT ps2.player_id, ps2.league_id, ROUND(AVG((p.value)::numeric)::numeric, 1) AS avg_pct
            FROM player_stats ps2
            CROSS JOIN LATERAL jsonb_each(ps2.percentiles) AS p(key, value)
            JOIN stat_definitions sd ON sd.sport = ps2.sport AND sd.entity_type = 'player' AND sd.key_name = p.key
            WHERE ps2.sport = p_sport AND ps2.season = p_season
              AND ps2.percentiles IS NOT NULL AND ps2.percentiles <> '{}'::jsonb
              AND jsonb_typeof(p.value) = 'number' AND p.key NOT LIKE '\_%'
              AND sd.is_percentile_eligible = true
            GROUP BY ps2.player_id, ps2.league_id HAVING COUNT(*) > 0
        ) sub
        WHERE ps.player_id = sub.player_id AND ps.sport = p_sport AND ps.season = p_season
          AND COALESCE(ps.league_id, 0) = COALESCE(sub.league_id, 0);

    UPDATE team_stats ts SET season_composite_score = sub.avg_pct
        FROM (
            SELECT ts2.team_id, ts2.league_id, ROUND(AVG((p.value)::numeric)::numeric, 1) AS avg_pct
            FROM team_stats ts2
            CROSS JOIN LATERAL jsonb_each(ts2.percentiles) AS p(key, value)
            JOIN stat_definitions sd ON sd.sport = ts2.sport AND sd.entity_type = 'team' AND sd.key_name = p.key
            WHERE ts2.sport = p_sport AND ts2.season = p_season
              AND ts2.percentiles IS NOT NULL AND ts2.percentiles <> '{}'::jsonb
              AND jsonb_typeof(p.value) = 'number' AND p.key NOT LIKE '\_%'
              AND sd.is_percentile_eligible = true
            GROUP BY ts2.team_id, ts2.league_id HAVING COUNT(*) > 0
        ) sub
        WHERE ts.team_id = sub.team_id AND ts.sport = p_sport AND ts.season = p_season
          AND COALESCE(ts.league_id, 0) = COALESCE(sub.league_id, 0);

    -- ============================================================
    -- Layer 2.5: COLD-START GUARD (NEW)
    -- Linear blend with prior-season anchor over the first v_window
    -- games. Players + teams symmetrically.
    -- ============================================================
    WITH cold_start_players AS (
        SELECT
            ps.player_id, ps.league_id,
            (SELECT COUNT(*) FROM event_box_scores e
             WHERE e.player_id = ps.player_id AND e.sport = ps.sport AND e.season = ps.season
               AND e.composite_score IS NOT NULL) AS games,
            COALESCE(
                -- Same entity, prev season, same league scope
                (SELECT prev.season_composite_score FROM player_stats prev
                 WHERE prev.player_id = ps.player_id AND prev.sport = ps.sport
                   AND prev.season = ps.season - 1
                   AND COALESCE(prev.league_id, 0) = COALESCE(ps.league_id, 0)
                   AND prev.season_composite_score IS NOT NULL
                 LIMIT 1),
                -- Else cohort prev-season average (sport + position)
                (SELECT AVG(prev.season_composite_score) FROM player_stats prev
                 WHERE prev.sport = ps.sport AND prev.season = ps.season - 1
                   AND COALESCE(prev.position, 'Unknown') = COALESCE(ps.position, 'Unknown')
                   AND prev.season_composite_score IS NOT NULL),
                -- Else 50 (first season in DB / no prior cohort)
                50.0
            ) AS prior_anchor,
            ps.season_composite_score AS current_score
        FROM player_stats ps
        WHERE ps.sport = p_sport AND ps.season = p_season
          AND ps.season_composite_score IS NOT NULL
    )
    UPDATE player_stats ps SET season_composite_score = ROUND((
        CASE WHEN cs.games >= v_window THEN cs.current_score
             ELSE (v_window - cs.games)::numeric / v_window * cs.prior_anchor
                  + cs.games::numeric          / v_window * cs.current_score
        END
    )::numeric, 1)
    FROM cold_start_players cs
    WHERE ps.player_id = cs.player_id AND ps.sport = p_sport AND ps.season = p_season
      AND COALESCE(ps.league_id, 0) = COALESCE(cs.league_id, 0)
      AND cs.games < v_window;  -- only touch rows where the guard actually changes the value

    WITH cold_start_teams AS (
        SELECT
            ts.team_id, ts.league_id,
            (SELECT COUNT(*) FROM event_team_stats e
             WHERE e.team_id = ts.team_id AND e.sport = ts.sport AND e.season = ts.season
               AND e.composite_score IS NOT NULL) AS games,
            COALESCE(
                (SELECT prev.season_composite_score FROM team_stats prev
                 WHERE prev.team_id = ts.team_id AND prev.sport = ts.sport
                   AND prev.season = ts.season - 1
                   AND COALESCE(prev.league_id, 0) = COALESCE(ts.league_id, 0)
                   AND prev.season_composite_score IS NOT NULL
                 LIMIT 1),
                (SELECT AVG(prev.season_composite_score) FROM team_stats prev
                 WHERE prev.sport = ts.sport AND prev.season = ts.season - 1
                   AND prev.season_composite_score IS NOT NULL),
                50.0
            ) AS prior_anchor,
            ts.season_composite_score AS current_score
        FROM team_stats ts
        WHERE ts.sport = p_sport AND ts.season = p_season
          AND ts.season_composite_score IS NOT NULL
    )
    UPDATE team_stats ts SET season_composite_score = ROUND((
        CASE WHEN cs.games >= v_window THEN cs.current_score
             ELSE (v_window - cs.games)::numeric / v_window * cs.prior_anchor
                  + cs.games::numeric          / v_window * cs.current_score
        END
    )::numeric, 1)
    FROM cold_start_teams cs
    WHERE ts.team_id = cs.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = COALESCE(cs.league_id, 0)
      AND cs.games < v_window;

    -- ============================================================
    -- Layer 3: season_composite_rank (within current season).
    -- Reads the COLD-START-BLENDED season_composite_score per above.
    -- ============================================================
    UPDATE player_stats SET season_composite_rank = NULL
        WHERE sport = p_sport AND season = p_season AND season_composite_rank IS NOT NULL;
    UPDATE team_stats SET season_composite_rank = NULL
        WHERE sport = p_sport AND season = p_season AND season_composite_rank IS NOT NULL;

    UPDATE player_stats ps SET season_composite_rank = r.rnk
        FROM (
            SELECT player_id, league_id,
                   ROUND((percent_rank() OVER (PARTITION BY COALESCE(position, 'Unknown') ORDER BY season_composite_score ASC))::numeric * 100, 1) AS rnk
            FROM player_stats WHERE sport = p_sport AND season = p_season AND season_composite_score IS NOT NULL
        ) r
        WHERE ps.player_id = r.player_id AND ps.sport = p_sport AND ps.season = p_season
          AND COALESCE(ps.league_id, 0) = COALESCE(r.league_id, 0);

    UPDATE team_stats ts SET season_composite_rank = r.rnk
        FROM (
            SELECT team_id, league_id,
                   ROUND((percent_rank() OVER (ORDER BY season_composite_score ASC))::numeric * 100, 1) AS rnk
            FROM team_stats WHERE sport = p_sport AND season = p_season AND season_composite_score IS NOT NULL
        ) r
        WHERE ts.team_id = r.team_id AND ts.sport = p_sport AND ts.season = p_season
          AND COALESCE(ts.league_id, 0) = COALESCE(r.league_id, 0);

    -- Layer 4 (all-time rank) lives in recalculate_alltime_ranks; runs
    -- nightly via the maintenance worker and reads the blended composite
    -- naturally on its next pass.

    RETURN QUERY SELECT v_player_events, v_team_events;
END;
$$ LANGUAGE plpgsql;

COMMIT;
