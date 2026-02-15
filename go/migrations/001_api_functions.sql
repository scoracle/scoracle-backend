-- Migration 001: API response functions for Go API layer
-- These functions return complete JSON responses, making Go a pure transport layer.
-- No struct scanning, no marshaling — Postgres serializes, Go passes bytes through.

-- ============================================================================
-- Player Profile — complete JSON response
-- ============================================================================

CREATE OR REPLACE FUNCTION api_player_profile(p_id INT, p_sport TEXT)
RETURNS JSON AS $$
    SELECT row_to_json(v)
    FROM v_player_profile v
    WHERE v.id = p_id AND v.sport_id = p_sport;
$$ LANGUAGE sql STABLE;

-- ============================================================================
-- Team Profile — complete JSON response
-- ============================================================================

CREATE OR REPLACE FUNCTION api_team_profile(p_id INT, p_sport TEXT)
RETURNS JSON AS $$
    SELECT row_to_json(v)
    FROM v_team_profile v
    WHERE v.id = p_id AND v.sport_id = p_sport;
$$ LANGUAGE sql STABLE;

-- ============================================================================
-- Entity Stats — complete JSON response with percentile metadata extracted
-- ============================================================================

CREATE OR REPLACE FUNCTION api_entity_stats(
    p_type TEXT,
    p_id INT,
    p_sport TEXT,
    p_season INT,
    p_league_id INT DEFAULT 0
) RETURNS JSON AS $$
DECLARE
    v_result JSON;
BEGIN
    IF p_type = 'player' THEN
        SELECT json_build_object(
            'entity_id', s.player_id,
            'entity_type', 'player',
            'sport', s.sport,
            'season', s.season,
            'stats', s.stats,
            'percentiles', s.percentiles - '_position_group' - '_sample_size',
            'percentile_metadata', CASE
                WHEN s.percentiles IS NOT NULL
                    AND s.percentiles->>'_position_group' IS NOT NULL THEN
                    json_build_object(
                        'position_group', s.percentiles->>'_position_group',
                        'sample_size', COALESCE((s.percentiles->>'_sample_size')::int, 0)
                    )
                END
        ) INTO v_result
        FROM player_stats s
        WHERE s.player_id = p_id
          AND s.sport = p_sport
          AND s.season = p_season
          AND s.league_id = p_league_id;
    ELSE
        SELECT json_build_object(
            'entity_id', s.team_id,
            'entity_type', 'team',
            'sport', s.sport,
            'season', s.season,
            'stats', s.stats,
            'percentiles', s.percentiles - '_position_group' - '_sample_size',
            'percentile_metadata', CASE
                WHEN s.percentiles IS NOT NULL
                    AND s.percentiles->>'_sample_size' IS NOT NULL THEN
                    json_build_object(
                        'sample_size', COALESCE((s.percentiles->>'_sample_size')::int, 0)
                    )
                END
        ) INTO v_result
        FROM team_stats s
        WHERE s.team_id = p_id
          AND s.sport = p_sport
          AND s.season = p_season
          AND s.league_id = p_league_id;
    END IF;

    RETURN v_result;
END;
$$ LANGUAGE plpgsql STABLE;

-- ============================================================================
-- Available Seasons — complete JSON response
-- ============================================================================

CREATE OR REPLACE FUNCTION api_available_seasons(
    p_type TEXT,
    p_id INT,
    p_sport TEXT
) RETURNS JSON AS $$
DECLARE
    v_seasons JSON;
BEGIN
    IF p_type = 'player' THEN
        SELECT json_agg(DISTINCT season ORDER BY season DESC) INTO v_seasons
        FROM player_stats
        WHERE player_id = p_id AND sport = p_sport;
    ELSE
        SELECT json_agg(DISTINCT season ORDER BY season DESC) INTO v_seasons
        FROM team_stats
        WHERE team_id = p_id AND sport = p_sport;
    END IF;

    RETURN json_build_object(
        'entity_id', p_id,
        'entity_type', p_type,
        'sport', p_sport,
        'seasons', COALESCE(v_seasons, '[]'::json)
    );
END;
$$ LANGUAGE plpgsql STABLE;

-- ============================================================================
-- Bootstrap — Materialized View for autofill entity database
-- ============================================================================
-- Pre-computes the entity list for frontend autocomplete.
-- Refresh after seeding: REFRESH MATERIALIZED VIEW CONCURRENTLY mv_autofill_entities;

CREATE MATERIALIZED VIEW IF NOT EXISTS mv_autofill_entities AS

    -- NBA/NFL players
    SELECT
        p.id,
        'player'::text AS type,
        p.sport,
        p.name,
        p.position,
        p.detailed_position,
        t.short_code AS team_abbr,
        t.name AS team_name,
        NULL::int AS league_id,
        NULL::text AS league_name,
        p.meta
    FROM players p
    LEFT JOIN teams t ON t.id = p.team_id AND t.sport = p.sport
    WHERE p.sport IN ('NBA', 'NFL')

    UNION ALL

    -- Football players (resolve league from latest player_stats)
    SELECT DISTINCT ON (p.id)
        p.id,
        'player'::text AS type,
        p.sport,
        p.name,
        p.position,
        p.detailed_position,
        t.short_code AS team_abbr,
        t.name AS team_name,
        ps.league_id,
        l.name AS league_name,
        p.meta
    FROM players p
    LEFT JOIN teams t ON t.id = p.team_id AND t.sport = p.sport
    LEFT JOIN player_stats ps ON ps.player_id = p.id AND ps.sport = p.sport
    LEFT JOIN leagues l ON l.id = ps.league_id
    WHERE p.sport = 'FOOTBALL'
    ORDER BY p.id, ps.season DESC NULLS LAST

    UNION ALL

    -- NBA/NFL teams
    SELECT
        t.id,
        'team'::text AS type,
        t.sport,
        t.name,
        t.conference AS position,
        t.division AS detailed_position,
        t.short_code AS team_abbr,
        NULL::text AS team_name,
        NULL::int AS league_id,
        NULL::text AS league_name,
        t.meta
    FROM teams t
    WHERE t.sport IN ('NBA', 'NFL')

    UNION ALL

    -- Football teams (resolve league from latest team_stats)
    SELECT DISTINCT ON (t.id)
        t.id,
        'team'::text AS type,
        t.sport,
        t.name,
        NULL::text AS position,
        NULL::text AS detailed_position,
        t.short_code AS team_abbr,
        NULL::text AS team_name,
        ts.league_id,
        l.name AS league_name,
        t.meta
    FROM teams t
    LEFT JOIN team_stats ts ON ts.team_id = t.id AND ts.sport = t.sport
    LEFT JOIN leagues l ON l.id = ts.league_id
    WHERE t.sport = 'FOOTBALL'
    ORDER BY t.id, ts.season DESC NULLS LAST

WITH DATA;

-- Unique index required for CONCURRENTLY refresh
CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_autofill_pk
    ON mv_autofill_entities (id, type, sport);

CREATE INDEX IF NOT EXISTS idx_mv_autofill_sport
    ON mv_autofill_entities (sport);
