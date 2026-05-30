-- 026_player_absolute_rank.sql
--
-- Cross-position "absolute" leaderboard for players (Delta 2 of the
-- 2026-05-30 composite enhancements proposal). Adds two new player-only
-- columns and computes them as percent_rank of season_composite_score
-- WITHOUT the position partition:
--
--   player_stats.season_composite_rank_absolute          NEW
--     In-season, cross-position. "Best player overall this season."
--     Uniform [0, 100] within (sport, season).
--
--   player_stats.season_composite_rank_alltime_absolute  NEW
--     Across all seasons, cross-position. "Best player-season overall
--     in the DB." Uniform [0, 100] within (sport).
--
-- Crucially: we percent_rank the EXISTING season_composite_score (which is
-- itself built from position-relative percentiles via layer 2). We do NOT
-- re-percentile raw stats cross-position — that would reintroduce the
-- volume/usage archetype bias position-partitioning was added to avoid.
-- The result is "most dominant relative to their own position," but
-- ranked cross-position — an absolute board that stays position-fair in
-- its inputs.
--
-- Teams are unchanged: their existing ranks are already sport-wide (no
-- position partition exists), so this is players-only.
--
-- Functions modified:
--   recalculate_event_percentiles  — adds Layer 3 absolute step (in-season)
--   recalculate_alltime_ranks      — adds Layer 4 absolute step (all-time;
--                                    same season-scope semantics as the
--                                    existing position-partitioned rank)

BEGIN;

ALTER TABLE player_stats
    ADD COLUMN IF NOT EXISTS season_composite_rank_absolute NUMERIC,
    ADD COLUMN IF NOT EXISTS season_composite_rank_alltime_absolute NUMERIC;

-- ---------------------------------------------------------------------------
-- recalculate_event_percentiles — adds Layer 3 absolute right after Layer 3.
-- Body otherwise identical to migration 025.
-- ---------------------------------------------------------------------------
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
    UPDATE event_box_scores SET composite_score = NULL, percentiles = '{}'::jsonb
        WHERE sport = p_sport AND season = p_season AND composite_score IS NOT NULL;
    UPDATE event_team_stats SET composite_score = NULL, percentiles = '{}'::jsonb
        WHERE sport = p_sport AND season = p_season AND composite_score IS NOT NULL;

    -- PLAYER EVENTS (Layer 1)
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

    -- TEAM EVENTS (Layer 1)
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

    -- Layer 2: season_composite_score
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

    -- Layer 2.5: Cold-start guard (migration 025)
    WITH cold_start_players AS (
        SELECT ps.player_id, ps.league_id,
            (SELECT COUNT(*) FROM event_box_scores e
             WHERE e.player_id = ps.player_id AND e.sport = ps.sport AND e.season = ps.season
               AND e.composite_score IS NOT NULL) AS games,
            COALESCE(
                (SELECT prev.season_composite_score FROM player_stats prev
                 WHERE prev.player_id = ps.player_id AND prev.sport = ps.sport
                   AND prev.season = ps.season - 1
                   AND COALESCE(prev.league_id, 0) = COALESCE(ps.league_id, 0)
                   AND prev.season_composite_score IS NOT NULL LIMIT 1),
                (SELECT AVG(prev.season_composite_score) FROM player_stats prev
                 WHERE prev.sport = ps.sport AND prev.season = ps.season - 1
                   AND COALESCE(prev.position, 'Unknown') = COALESCE(ps.position, 'Unknown')
                   AND prev.season_composite_score IS NOT NULL),
                50.0) AS prior_anchor,
            ps.season_composite_score AS current_score
        FROM player_stats ps
        WHERE ps.sport = p_sport AND ps.season = p_season AND ps.season_composite_score IS NOT NULL
    )
    UPDATE player_stats ps SET season_composite_score = ROUND((
        (v_window - cs.games)::numeric / v_window * cs.prior_anchor
      + cs.games::numeric              / v_window * cs.current_score
    )::numeric, 1)
    FROM cold_start_players cs
    WHERE ps.player_id = cs.player_id AND ps.sport = p_sport AND ps.season = p_season
      AND COALESCE(ps.league_id, 0) = COALESCE(cs.league_id, 0) AND cs.games < v_window;

    WITH cold_start_teams AS (
        SELECT ts.team_id, ts.league_id,
            (SELECT COUNT(*) FROM event_team_stats e
             WHERE e.team_id = ts.team_id AND e.sport = ts.sport AND e.season = ts.season
               AND e.composite_score IS NOT NULL) AS games,
            COALESCE(
                (SELECT prev.season_composite_score FROM team_stats prev
                 WHERE prev.team_id = ts.team_id AND prev.sport = ts.sport
                   AND prev.season = ts.season - 1
                   AND COALESCE(prev.league_id, 0) = COALESCE(ts.league_id, 0)
                   AND prev.season_composite_score IS NOT NULL LIMIT 1),
                (SELECT AVG(prev.season_composite_score) FROM team_stats prev
                 WHERE prev.sport = ts.sport AND prev.season = ts.season - 1
                   AND prev.season_composite_score IS NOT NULL),
                50.0) AS prior_anchor,
            ts.season_composite_score AS current_score
        FROM team_stats ts
        WHERE ts.sport = p_sport AND ts.season = p_season AND ts.season_composite_score IS NOT NULL
    )
    UPDATE team_stats ts SET season_composite_score = ROUND((
        (v_window - cs.games)::numeric / v_window * cs.prior_anchor
      + cs.games::numeric              / v_window * cs.current_score
    )::numeric, 1)
    FROM cold_start_teams cs
    WHERE ts.team_id = cs.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = COALESCE(cs.league_id, 0) AND cs.games < v_window;

    -- Layer 3: season_composite_rank (within-position for players)
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

    -- ============================================================
    -- Layer 3 ABSOLUTE: cross-position rank for players (NEW, mig 026)
    -- No PARTITION BY position — ranks players across ALL positions in
    -- the (sport, season). Teams have no equivalent (already sport-wide).
    -- ============================================================
    UPDATE player_stats SET season_composite_rank_absolute = NULL
        WHERE sport = p_sport AND season = p_season AND season_composite_rank_absolute IS NOT NULL;

    UPDATE player_stats ps SET season_composite_rank_absolute = r.rnk
        FROM (
            SELECT player_id, league_id,
                   ROUND((percent_rank() OVER (ORDER BY season_composite_score ASC))::numeric * 100, 1) AS rnk
            FROM player_stats
            WHERE sport = p_sport AND season = p_season AND season_composite_score IS NOT NULL
        ) r
        WHERE ps.player_id = r.player_id AND ps.sport = p_sport AND ps.season = p_season
          AND COALESCE(ps.league_id, 0) = COALESCE(r.league_id, 0);

    RETURN QUERY SELECT v_player_events, v_team_events;
END;
$$ LANGUAGE plpgsql;

-- ---------------------------------------------------------------------------
-- recalculate_alltime_ranks — adds Layer 4 absolute. Same season-scope
-- semantics (NULL → full re-baseline, integer → that season only). Body
-- otherwise identical to migration 024.
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION recalculate_alltime_ranks(
    p_sport TEXT,
    p_season INTEGER DEFAULT NULL
)
RETURNS TABLE (players_updated INTEGER, teams_updated INTEGER) AS $$
DECLARE
    v_players INTEGER := 0;
    v_teams INTEGER := 0;
BEGIN
    -- Null out rows that lost their composite.
    UPDATE player_stats SET season_composite_rank_alltime = NULL, season_composite_rank_alltime_absolute = NULL
        WHERE sport = p_sport AND season_composite_score IS NULL
          AND (season_composite_rank_alltime IS NOT NULL OR season_composite_rank_alltime_absolute IS NOT NULL)
          AND (p_season IS NULL OR season = p_season);
    UPDATE team_stats SET season_composite_rank_alltime = NULL
        WHERE sport = p_sport AND season_composite_score IS NULL
          AND season_composite_rank_alltime IS NOT NULL
          AND (p_season IS NULL OR season = p_season);

    -- Players: position-partitioned all-time rank (Layer 4)
    UPDATE player_stats ps SET season_composite_rank_alltime = r.rnk
        FROM (
            SELECT player_id, season, league_id,
                   ROUND((percent_rank() OVER (
                       PARTITION BY COALESCE(position, 'Unknown')
                       ORDER BY season_composite_score ASC
                   ))::numeric * 100, 1) AS rnk
            FROM player_stats
            WHERE sport = p_sport AND season_composite_score IS NOT NULL
        ) r
        WHERE ps.player_id = r.player_id AND ps.sport = p_sport AND ps.season = r.season
          AND COALESCE(ps.league_id, 0) = COALESCE(r.league_id, 0)
          AND (p_season IS NULL OR ps.season = p_season);
    GET DIAGNOSTICS v_players = ROW_COUNT;

    -- Players: cross-position absolute all-time rank (Layer 4 absolute, NEW)
    UPDATE player_stats ps SET season_composite_rank_alltime_absolute = r.rnk
        FROM (
            SELECT player_id, season, league_id,
                   ROUND((percent_rank() OVER (ORDER BY season_composite_score ASC))::numeric * 100, 1) AS rnk
            FROM player_stats
            WHERE sport = p_sport AND season_composite_score IS NOT NULL
        ) r
        WHERE ps.player_id = r.player_id AND ps.sport = p_sport AND ps.season = r.season
          AND COALESCE(ps.league_id, 0) = COALESCE(r.league_id, 0)
          AND (p_season IS NULL OR ps.season = p_season);

    -- Teams: sport-wide all-time rank (unchanged from mig 024)
    UPDATE team_stats ts SET season_composite_rank_alltime = r.rnk
        FROM (
            SELECT team_id, season, league_id,
                   ROUND((percent_rank() OVER (ORDER BY season_composite_score ASC))::numeric * 100, 1) AS rnk
            FROM team_stats
            WHERE sport = p_sport AND season_composite_score IS NOT NULL
        ) r
        WHERE ts.team_id = r.team_id AND ts.sport = p_sport AND ts.season = r.season
          AND COALESCE(ts.league_id, 0) = COALESCE(r.league_id, 0)
          AND (p_season IS NULL OR ts.season = p_season);
    GET DIAGNOSTICS v_teams = ROW_COUNT;

    RETURN QUERY SELECT v_players, v_teams;
END;
$$ LANGUAGE plpgsql;

COMMIT;
