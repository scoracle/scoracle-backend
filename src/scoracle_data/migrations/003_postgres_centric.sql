-- Scoracle Data — Postgres-Centric Migration v5.2
-- Created: 2026-02-10
-- Purpose: Move data logic into Postgres. Python becomes a thin orchestration
--          layer (seeding + endpoints). Postgres owns all data shaping, ranking,
--          and derived computations. Any future API framework (Go, etc.) queries
--          the same views and functions without reimplementing business logic.
--
-- Changes:
--   1. Profile views: v_player_profile, v_team_profile
--   2. Stat leaders function: fn_stat_leaders (with LATERAL + ROW_NUMBER)
--   3. Standings function: fn_standings (unified FOOTBALL/NBA/NFL + win_pct)
--   4. Football derived stats trigger (per-90, accuracy rates)
--   5. Updated get_pending_fixtures to include external_id + max_retries filter

-- ============================================================================
-- 1. PROFILE VIEWS
-- ============================================================================
-- Replace Python-side dict manipulation (popping keys, conditional nesting)
-- with Postgres json_build_object(). Nulls pass through — frontend handles display.

CREATE OR REPLACE VIEW v_player_profile AS
SELECT
    p.id,
    p.sport AS sport_id,
    p.name,
    p.first_name,
    p.last_name,
    p.position,
    p.detailed_position,
    p.nationality,
    p.date_of_birth::text AS date_of_birth,
    p.height_cm,
    p.weight_kg,
    p.photo_url,
    p.team_id,
    p.league_id,
    p.meta,
    CASE WHEN t.id IS NOT NULL THEN json_build_object(
        'id', t.id,
        'name', t.name,
        'abbreviation', t.short_code,
        'logo_url', t.logo_url,
        'country', t.country,
        'city', t.city,
        'conference', t.conference,
        'division', t.division
    ) END AS team,
    CASE WHEN l.id IS NOT NULL THEN json_build_object(
        'id', l.id,
        'name', l.name,
        'country', l.country,
        'logo_url', l.logo_url
    ) END AS league
FROM players p
LEFT JOIN teams t ON t.id = p.team_id AND t.sport = p.sport
LEFT JOIN leagues l ON l.id = p.league_id;

CREATE OR REPLACE VIEW v_team_profile AS
SELECT
    t.id,
    t.sport AS sport_id,
    t.name,
    t.short_code,
    t.logo_url,
    t.country,
    t.city,
    t.founded,
    t.league_id,
    t.conference,
    t.division,
    t.venue_name,
    t.venue_capacity,
    t.meta,
    CASE WHEN l.id IS NOT NULL THEN json_build_object(
        'id', l.id,
        'name', l.name,
        'country', l.country,
        'logo_url', l.logo_url
    ) END AS league
FROM teams t
LEFT JOIN leagues l ON l.id = t.league_id;

-- ============================================================================
-- 2. STAT LEADERS FUNCTION
-- ============================================================================
-- Encapsulates the stat leaders query with LATERAL join (extract JSONB value
-- once) and ROW_NUMBER for ranking. Any client calls this identically.
--
-- LANGUAGE sql allows the query planner to inline the function body.

CREATE OR REPLACE FUNCTION fn_stat_leaders(
    p_sport TEXT,
    p_season INTEGER,
    p_stat_name TEXT,
    p_limit INTEGER DEFAULT 25,
    p_position TEXT DEFAULT NULL,
    p_league_id INTEGER DEFAULT 0
)
RETURNS TABLE (
    rank BIGINT,
    player_id INTEGER,
    name TEXT,
    position TEXT,
    team_name TEXT,
    stat_value NUMERIC
) AS $$
    SELECT
        ROW_NUMBER() OVER (ORDER BY stat.val DESC) AS rank,
        p.id AS player_id,
        p.name,
        p.position,
        t.name AS team_name,
        stat.val AS stat_value
    FROM player_stats s
    CROSS JOIN LATERAL (
        SELECT (s.stats->>p_stat_name)::NUMERIC AS val
    ) stat
    JOIN players p ON s.player_id = p.id AND s.sport = p.sport
    LEFT JOIN teams t ON s.team_id = t.id AND s.sport = t.sport
    WHERE s.sport = p_sport
      AND s.season = p_season
      AND s.league_id = p_league_id
      AND stat.val IS NOT NULL
      AND (p_position IS NULL OR p.position = p_position)
    ORDER BY stat.val DESC
    LIMIT p_limit;
$$ LANGUAGE sql STABLE;

-- ============================================================================
-- 3. STANDINGS FUNCTION
-- ============================================================================
-- Unified standings for all sports. Sport-conditional ORDER BY. Includes
-- win_pct for NBA/NFL so consumers don't compute it themselves.

CREATE OR REPLACE FUNCTION fn_standings(
    p_sport TEXT,
    p_season INTEGER,
    p_league_id INTEGER DEFAULT 0,
    p_conference TEXT DEFAULT NULL
)
RETURNS TABLE (
    rank BIGINT,
    id INTEGER,
    name TEXT,
    logo_url TEXT,
    conference TEXT,
    division TEXT,
    league_name TEXT,
    stats JSONB,
    win_pct NUMERIC
) AS $$
    SELECT
        ROW_NUMBER() OVER (
            ORDER BY
                CASE WHEN p_sport = 'FOOTBALL'
                    THEN (s.stats->>'points')::INTEGER END DESC NULLS LAST,
                CASE WHEN p_sport = 'FOOTBALL'
                    THEN (s.stats->>'goal_difference')::INTEGER END DESC NULLS LAST,
                CASE WHEN p_sport = 'FOOTBALL'
                    THEN (s.stats->>'goals_for')::INTEGER END DESC NULLS LAST,
                CASE WHEN p_sport != 'FOOTBALL'
                    THEN (s.stats->>'wins')::INTEGER END DESC NULLS LAST
        ) AS rank,
        t.id,
        t.name,
        t.logo_url,
        t.conference,
        t.division,
        l.name AS league_name,
        s.stats,
        CASE WHEN p_sport != 'FOOTBALL'
            THEN ROUND(
                (s.stats->>'wins')::NUMERIC /
                NULLIF((s.stats->>'wins')::INTEGER + (s.stats->>'losses')::INTEGER, 0),
                3
            )
        END AS win_pct
    FROM team_stats s
    JOIN teams t ON s.team_id = t.id AND s.sport = t.sport
    LEFT JOIN leagues l ON s.league_id = l.id
    WHERE s.sport = p_sport
      AND s.season = p_season
      AND s.league_id = p_league_id
      AND (p_conference IS NULL OR t.conference = p_conference)
    ORDER BY rank;
$$ LANGUAGE sql STABLE;

-- ============================================================================
-- 4. FOOTBALL DERIVED STATS TRIGGER
-- ============================================================================
-- Computes per-90 metrics and accuracy rates on INSERT/UPDATE.
-- Fires only for FOOTBALL rows, so NBA/NFL writes have zero overhead.
-- Seeders write raw stats; Postgres handles derived values.

CREATE OR REPLACE FUNCTION compute_football_derived_stats()
RETURNS TRIGGER AS $$
DECLARE
    minutes   NUMERIC;
    goals     NUMERIC;
    assists   NUMERIC;
    shots_t   NUMERIC;
    shots_on  NUMERIC;
    passes_t  NUMERIC;
    passes_a  NUMERIC;
    duels_t   NUMERIC;
    duels_w   NUMERIC;
    dribbles_a NUMERIC;
    dribbles_s NUMERIC;
BEGIN
    -- Extract raw values once
    minutes    := (NEW.stats->>'minutes_played')::NUMERIC;
    goals      := (NEW.stats->>'goals')::NUMERIC;
    assists    := (NEW.stats->>'assists')::NUMERIC;
    shots_t    := (NEW.stats->>'shots_total')::NUMERIC;
    shots_on   := (NEW.stats->>'shots_on_target')::NUMERIC;
    passes_t   := (NEW.stats->>'passes_total')::NUMERIC;
    passes_a   := (NEW.stats->>'passes_accurate')::NUMERIC;
    duels_t    := (NEW.stats->>'duels_total')::NUMERIC;
    duels_w    := (NEW.stats->>'duels_won')::NUMERIC;
    dribbles_a := (NEW.stats->>'dribbles_attempts')::NUMERIC;
    dribbles_s := (NEW.stats->>'dribbles_success')::NUMERIC;

    -- Per-90 metrics (requires minutes > 0)
    IF minutes IS NOT NULL AND minutes > 0 THEN
        IF goals IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object(
                'goals_per_90', ROUND(goals * 90 / minutes, 3));
        END IF;
        IF assists IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object(
                'assists_per_90', ROUND(assists * 90 / minutes, 3));
        END IF;
    END IF;

    -- Accuracy rates (requires denominator > 0)
    IF shots_t IS NOT NULL AND shots_t > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object(
            'shot_accuracy', ROUND(COALESCE(shots_on, 0) / shots_t * 100, 1));
    END IF;

    IF passes_t IS NOT NULL AND passes_t > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object(
            'pass_accuracy', ROUND(COALESCE(passes_a, 0) / passes_t * 100, 1));
    END IF;

    IF duels_t IS NOT NULL AND duels_t > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object(
            'duel_success_rate', ROUND(COALESCE(duels_w, 0) / duels_t * 100, 1));
    END IF;

    IF dribbles_a IS NOT NULL AND dribbles_a > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object(
            'dribble_success_rate', ROUND(COALESCE(dribbles_s, 0) / dribbles_a * 100, 1));
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Conditional trigger: only fires for FOOTBALL rows
CREATE OR REPLACE TRIGGER trg_football_derived_stats
    BEFORE INSERT OR UPDATE ON player_stats
    FOR EACH ROW
    WHEN (NEW.sport = 'FOOTBALL')
    EXECUTE FUNCTION compute_football_derived_stats();

-- ============================================================================
-- 5. UPDATED get_pending_fixtures
-- ============================================================================
-- Original version was missing external_id and max_retries filter.
-- This version matches what the scheduler actually needs.

CREATE OR REPLACE FUNCTION get_pending_fixtures(
    p_sport TEXT DEFAULT NULL,
    p_limit INTEGER DEFAULT 50,
    p_max_retries INTEGER DEFAULT 3
)
RETURNS TABLE (
    id INTEGER,
    sport TEXT,
    league_id INTEGER,
    season INTEGER,
    home_team_id INTEGER,
    away_team_id INTEGER,
    start_time TIMESTAMPTZ,
    seed_delay_hours INTEGER,
    seed_attempts INTEGER,
    external_id INTEGER
) AS $$
    SELECT
        f.id, f.sport, f.league_id, f.season,
        f.home_team_id, f.away_team_id, f.start_time,
        f.seed_delay_hours, f.seed_attempts, f.external_id
    FROM fixtures f
    WHERE (f.status = 'scheduled' OR f.status = 'completed')
      AND NOW() >= f.start_time + (f.seed_delay_hours || ' hours')::INTERVAL
      AND f.seed_attempts < p_max_retries
      AND (p_sport IS NULL OR f.sport = p_sport)
    ORDER BY f.start_time ASC
    LIMIT p_limit;
$$ LANGUAGE sql STABLE;

-- ============================================================================
-- 6. SCHEMA VERSION BUMP
-- ============================================================================

UPDATE meta SET value = '5.2', updated_at = NOW() WHERE key = 'schema_version';
