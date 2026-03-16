-- Scoracle Data — NFL Schema
-- Owner: NFL product owner
-- Contains: NFL-specific views, stat definitions, triggers, functions, grants
-- Depends on: sql/shared.sql (public tables must exist first)

CREATE SCHEMA IF NOT EXISTS nfl;

-- ============================================================================
-- 1. STAT DEFINITIONS
-- ============================================================================

-- NFL player stats
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('NFL', 'games_played',            'Games Played',          'player', 'general',    false, false, false,  1),
    ('NFL', 'passing_completions',     'Completions',           'player', 'passing',    false, false, false, 10),
    ('NFL', 'passing_attempts',        'Pass Attempts',         'player', 'passing',    false, false, false, 11),
    ('NFL', 'passing_yards',           'Passing Yards',         'player', 'passing',    false, false, true,  12),
    ('NFL', 'passing_touchdowns',      'Passing TDs',           'player', 'passing',    false, false, true,  13),
    ('NFL', 'passing_interceptions',   'Interceptions Thrown',  'player', 'passing',    true,  false, true,  14),
    ('NFL', 'passing_yards_per_game',  'Pass Yards/Game',       'player', 'passing',    false, false, true,  15),
    ('NFL', 'passing_completion_pct',  'Completion %',          'player', 'passing',    false, false, true,  16),
    ('NFL', 'qbr',                     'Passer Rating',         'player', 'passing',    false, false, true,  17),
    ('NFL', 'td_int_ratio',            'TD/INT Ratio',          'player', 'passing',    false, true,  true,  18),
    ('NFL', 'rushing_attempts',        'Rush Attempts',         'player', 'rushing',    false, false, false, 20),
    ('NFL', 'rushing_yards',           'Rushing Yards',         'player', 'rushing',    false, false, true,  21),
    ('NFL', 'rushing_touchdowns',      'Rushing TDs',           'player', 'rushing',    false, false, true,  22),
    ('NFL', 'rushing_yards_per_game',  'Rush Yards/Game',       'player', 'rushing',    false, false, true,  23),
    ('NFL', 'yards_per_rush_attempt',  'Yards/Carry',           'player', 'rushing',    false, false, true,  24),
    ('NFL', 'rushing_first_downs',     'Rushing First Downs',   'player', 'rushing',    false, false, false, 25),
    ('NFL', 'receptions',              'Receptions',            'player', 'receiving',  false, false, true,  30),
    ('NFL', 'receiving_yards',         'Receiving Yards',       'player', 'receiving',  false, false, true,  31),
    ('NFL', 'receiving_touchdowns',    'Receiving TDs',         'player', 'receiving',  false, false, true,  32),
    ('NFL', 'receiving_targets',       'Targets',               'player', 'receiving',  false, false, false, 33),
    ('NFL', 'receiving_yards_per_game','Receiving Yards/Game',  'player', 'receiving',  false, false, true,  34),
    ('NFL', 'yards_per_reception',     'Yards/Reception',       'player', 'receiving',  false, false, true,  35),
    ('NFL', 'receiving_first_downs',   'Receiving First Downs', 'player', 'receiving',  false, false, false, 36),
    ('NFL', 'catch_pct',               'Catch %',               'player', 'receiving',  false, true,  true,  37),
    ('NFL', 'total_tackles',           'Total Tackles',         'player', 'defensive',  false, false, true,  40),
    ('NFL', 'solo_tackles',            'Solo Tackles',          'player', 'defensive',  false, false, false, 41),
    ('NFL', 'assist_tackles',          'Assisted Tackles',      'player', 'defensive',  false, false, false, 42),
    ('NFL', 'defensive_sacks',         'Sacks',                 'player', 'defensive',  false, false, true,  43),
    ('NFL', 'defensive_sack_yards',    'Sack Yards',            'player', 'defensive',  false, false, false, 44),
    ('NFL', 'defensive_interceptions', 'Interceptions',         'player', 'defensive',  false, false, true,  45),
    ('NFL', 'interception_touchdowns', 'INT Return TDs',        'player', 'defensive',  false, false, false, 46),
    ('NFL', 'fumbles_forced',          'Forced Fumbles',        'player', 'defensive',  false, false, true,  47),
    ('NFL', 'fumbles_recovered',       'Fumbles Recovered',     'player', 'defensive',  false, false, false, 48),
    ('NFL', 'field_goal_attempts',     'FG Attempts',           'player', 'kicking',    false, false, false, 50),
    ('NFL', 'field_goals_made',        'FG Made',               'player', 'kicking',    false, false, false, 51),
    ('NFL', 'field_goal_pct',          'FG Percentage',         'player', 'kicking',    false, false, true,  52),
    ('NFL', 'punts',                   'Punts',                 'player', 'special',    false, false, false, 60),
    ('NFL', 'punt_yards',              'Punt Yards',            'player', 'special',    false, false, false, 61),
    ('NFL', 'kick_returns',            'Kick Returns',          'player', 'special',    false, false, false, 62),
    ('NFL', 'kick_return_yards',       'Kick Return Yards',     'player', 'special',    false, false, false, 63),
    ('NFL', 'kick_return_touchdowns',  'Kick Return TDs',       'player', 'special',    false, false, false, 64),
    ('NFL', 'punt_returner_returns',   'Punt Returns',          'player', 'special',    false, false, false, 65),
    ('NFL', 'punt_returner_return_yards','Punt Return Yards',   'player', 'special',    false, false, false, 66),
    ('NFL', 'punt_return_touchdowns',  'Punt Return TDs',       'player', 'special',    false, false, false, 67)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- NFL team stats
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('NFL', 'wins',              'Wins',                'team', 'standings', false, false, true,   1),
    ('NFL', 'losses',            'Losses',              'team', 'standings', true,  false, true,   2),
    ('NFL', 'ties',              'Ties',                'team', 'standings', false, false, false,  3),
    ('NFL', 'points_for',        'Points For',          'team', 'scoring',  false, false, true,   4),
    ('NFL', 'points_against',    'Points Against',      'team', 'scoring',  true,  false, true,   5),
    ('NFL', 'point_differential','Point Differential',  'team', 'scoring',  false, false, true,   6),
    ('NFL', 'win_pct',           'Win Percentage',      'team', 'standings', false, true,  true,   7)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- ============================================================================
-- 2. DERIVED STATS TRIGGERS
-- ============================================================================

-- NFL player: td_int_ratio, catch_pct
CREATE OR REPLACE FUNCTION nfl.compute_derived_player_stats()
RETURNS TRIGGER AS $$
DECLARE pass_td NUMERIC; pass_int NUMERIC; rec NUMERIC; targets NUMERIC;
BEGIN
    pass_td  := (NEW.stats->>'passing_touchdowns')::NUMERIC;
    pass_int := (NEW.stats->>'passing_interceptions')::NUMERIC;
    rec      := (NEW.stats->>'receptions')::NUMERIC;
    targets  := (NEW.stats->>'receiving_targets')::NUMERIC;
    IF pass_td IS NOT NULL AND pass_int IS NOT NULL AND pass_int > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object('td_int_ratio', ROUND(pass_td / pass_int, 2));
    END IF;
    IF rec IS NOT NULL AND targets IS NOT NULL AND targets > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object('catch_pct', ROUND(rec / targets * 100, 1));
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- NFL team: win_pct (with ties)
CREATE OR REPLACE FUNCTION nfl.compute_derived_team_stats()
RETURNS TRIGGER AS $$
DECLARE wins NUMERIC; losses NUMERIC; ties NUMERIC; total NUMERIC;
BEGIN
    wins := (NEW.stats->>'wins')::NUMERIC; losses := (NEW.stats->>'losses')::NUMERIC;
    ties := COALESCE((NEW.stats->>'ties')::NUMERIC, 0);
    IF wins IS NOT NULL AND losses IS NOT NULL THEN
        total := wins + losses + ties;
        IF total > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('win_pct', ROUND(wins / total, 3)); END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Triggers on shared tables
DROP TRIGGER IF EXISTS trg_nfl_player_derived_stats ON player_stats;
CREATE TRIGGER trg_nfl_player_derived_stats
    BEFORE INSERT OR UPDATE ON player_stats
    FOR EACH ROW WHEN (NEW.sport = 'NFL')
    EXECUTE FUNCTION nfl.compute_derived_player_stats();

DROP TRIGGER IF EXISTS trg_nfl_team_derived_stats ON team_stats;
CREATE TRIGGER trg_nfl_team_derived_stats
    BEFORE INSERT OR UPDATE ON team_stats
    FOR EACH ROW WHEN (NEW.sport = 'NFL')
    EXECUTE FUNCTION nfl.compute_derived_team_stats();

-- ============================================================================
-- 3. VIEWS (PostgREST surface)
-- ============================================================================

-- Combined player profile + stats
CREATE OR REPLACE VIEW nfl.player AS
SELECT
    p.id, p.name, p.first_name, p.last_name, p.position,
    p.detailed_position, p.nationality, p.date_of_birth::text AS date_of_birth,
    p.height, p.weight, p.photo_url, p.team_id, p.league_id, p.meta,
    CASE WHEN t.id IS NOT NULL THEN json_build_object(
        'id', t.id, 'name', t.name, 'abbreviation', t.short_code,
        'logo_url', t.logo_url, 'country', t.country, 'city', t.city,
        'conference', t.conference, 'division', t.division
    ) END AS team,
    ps.season,
    ps.stats,
    ps.percentiles - '_position_group' - '_sample_size' AS percentiles,
    CASE
        WHEN ps.percentiles IS NOT NULL
            AND ps.percentiles->>'_position_group' IS NOT NULL
        THEN jsonb_build_object(
            'position_group', ps.percentiles->>'_position_group',
            'sample_size', COALESCE((ps.percentiles->>'_sample_size')::int, 0)
        )
    END AS percentile_metadata,
    ps.updated_at AS stats_updated_at
FROM public.players p
LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
LEFT JOIN public.player_stats ps ON ps.player_id = p.id AND ps.sport = p.sport
WHERE p.sport = 'NFL';

COMMENT ON VIEW nfl.player IS
    'NFL player profile with stats. Filter by id, season. Stats columns are NULL when no stats exist.';

-- Combined team profile + stats
CREATE OR REPLACE VIEW nfl.team AS
SELECT
    t.id, t.name, t.short_code, t.logo_url, t.country, t.city,
    t.founded, t.league_id, t.conference, t.division,
    t.venue_name, t.venue_capacity, t.meta,
    ts.season,
    ts.stats,
    ts.percentiles - '_sample_size' AS percentiles,
    CASE
        WHEN ts.percentiles IS NOT NULL
            AND ts.percentiles->>'_sample_size' IS NOT NULL
        THEN jsonb_build_object(
            'sample_size', COALESCE((ts.percentiles->>'_sample_size')::int, 0)
        )
    END AS percentile_metadata,
    ts.updated_at AS stats_updated_at
FROM public.teams t
LEFT JOIN public.team_stats ts ON ts.team_id = t.id AND ts.sport = t.sport
WHERE t.sport = 'NFL';

COMMENT ON VIEW nfl.team IS
    'NFL team profile with stats. Filter by id, season. Stats columns are NULL when no stats exist.';

-- Standings (hardcoded NFL sort: by wins, with ties support)
CREATE OR REPLACE VIEW nfl.standings AS
SELECT
    ts.team_id, ts.season, ts.league_id,
    t.name AS team_name, t.short_code AS team_abbr, t.logo_url,
    t.conference, t.division, ts.stats,
    ROUND(
        (ts.stats->>'wins')::numeric /
        NULLIF((ts.stats->>'wins')::integer + (ts.stats->>'losses')::integer + COALESCE((ts.stats->>'ties')::integer, 0), 0), 3
    ) AS win_pct
FROM public.team_stats ts
JOIN public.teams t ON t.id = ts.team_id AND t.sport = ts.sport
WHERE ts.sport = 'NFL';

COMMENT ON VIEW nfl.standings IS
    'NFL standings. Order by win_pct DESC. Filter by season, conference, division.';

CREATE OR REPLACE VIEW nfl.stat_definitions AS
SELECT id, key_name, display_name, entity_type, category,
       is_inverse, is_derived, is_percentile_eligible, sort_order
FROM public.stat_definitions
WHERE sport = 'NFL';

COMMENT ON VIEW nfl.stat_definitions IS
    'NFL stat registry. Filter by entity_type.';

-- ============================================================================
-- 4. MATERIALIZED VIEW — autofill/search
-- ============================================================================

DROP MATERIALIZED VIEW IF EXISTS nfl.autofill_entities;
CREATE MATERIALIZED VIEW nfl.autofill_entities AS
    SELECT p.id, 'player'::text AS type, p.name, p.position, p.detailed_position,
           t.short_code AS team_abbr, t.name AS team_name,
           NULL::int AS league_id, NULL::text AS league_name, p.meta
    FROM public.players p
    LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
    WHERE p.sport = 'NFL'
UNION ALL
    SELECT t.id, 'team'::text AS type, t.name, t.conference AS position,
           t.division AS detailed_position,
           t.short_code AS team_abbr, NULL::text AS team_name,
           NULL::int AS league_id, NULL::text AS league_name, t.meta
    FROM public.teams t
    WHERE t.sport = 'NFL'
WITH DATA;

CREATE UNIQUE INDEX IF NOT EXISTS idx_nfl_autofill_pk
    ON nfl.autofill_entities (id, type);

-- ============================================================================
-- 5. RPC FUNCTIONS
-- ============================================================================

CREATE OR REPLACE FUNCTION nfl.stat_leaders(
    p_season INTEGER, p_stat_name TEXT,
    p_limit INTEGER DEFAULT 25, p_position TEXT DEFAULT NULL,
    p_league_id INTEGER DEFAULT 0
)
RETURNS TABLE ("rank" BIGINT, player_id INTEGER, name TEXT, "position" TEXT, team_name TEXT, stat_value NUMERIC) AS $$
    SELECT
        ROW_NUMBER() OVER (ORDER BY stat.val DESC) AS rank,
        p.id AS player_id, p.name, p.position, t.name AS team_name, stat.val AS stat_value
    FROM public.player_stats s
    CROSS JOIN LATERAL (SELECT (s.stats->>p_stat_name)::NUMERIC AS val) stat
    JOIN public.players p ON s.player_id = p.id AND s.sport = p.sport
    LEFT JOIN public.teams t ON s.team_id = t.id AND s.sport = t.sport
    WHERE s.sport = 'NFL' AND s.season = p_season AND s.league_id = p_league_id
      AND stat.val IS NOT NULL
      AND (p_position IS NULL OR p.position = p_position)
    ORDER BY stat.val DESC
    LIMIT p_limit;
$$ LANGUAGE sql STABLE;

COMMENT ON FUNCTION nfl.stat_leaders IS
    'Returns top N NFL players by stat category with positional filtering.';

CREATE OR REPLACE FUNCTION nfl.health()
RETURNS json AS $$
    SELECT json_build_object('status', 'ok');
$$ LANGUAGE sql STABLE;

-- ============================================================================
-- 6. GRANTS
-- ============================================================================

GRANT USAGE ON SCHEMA nfl TO web_anon, web_user;
GRANT SELECT ON ALL TABLES IN SCHEMA nfl TO web_anon, web_user;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA nfl TO web_anon, web_user;
