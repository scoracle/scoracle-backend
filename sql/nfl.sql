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
    ('NFL', 'punt_return_touchdowns',  'Punt Return TDs',       'player', 'special',    false, false, false, 67),
    ('NFL', 'tackles_for_loss',        'Tackles for Loss',      'player', 'defensive',  false, false, true,  49),
    ('NFL', 'passes_defended',         'Passes Defended',       'player', 'defensive',  false, false, true,  50),
    ('NFL', 'qb_hits',                 'QB Hits',               'player', 'defensive',  false, false, true,  51),
    ('NFL', 'fumbles_touchdowns',      'Fumble Return TDs',     'player', 'defensive',  false, false, false, 52),
    ('NFL', 'fumbles',                 'Fumbles',               'player', 'general',    true,  false, false, 6),
    ('NFL', 'fumbles_lost',            'Fumbles Lost',          'player', 'general',    true,  false, false, 7),
    ('NFL', 'extra_points_made',       'Extra Points Made',     'player', 'kicking',    false, false, false, 53),
    ('NFL', 'total_points',            'Total Points',          'player', 'kicking',    false, false, false, 54),
    ('NFL', 'touchbacks',              'Touchbacks',            'player', 'kicking',    false, false, false, 55),
    ('NFL', 'punts_inside_20',         'Punts Inside 20',       'player', 'special',    false, false, true,  68),
    ('NFL', 'qb_rating',               'Passer Rating (NFL)',   'player', 'passing',    false, false, true,  19),
    ('NFL', 'yards_per_pass_attempt',  'Yards/Attempt',         'player', 'passing',    false, true,  true,  26),
    ('NFL', 'sacks_taken',             'Sacks Taken',           'player', 'passing',    true,  false, false, 27),
    ('NFL', 'sack_yards_lost',         'Sack Yards Lost',       'player', 'passing',    true,  false, false, 28),
    ('NFL', 'long_pass',               'Longest Pass',          'player', 'passing',    false, false, false, 29),
    ('NFL', 'long_rushing',            'Longest Rush',          'player', 'rushing',    false, false, false, 70),
    ('NFL', 'long_reception',          'Longest Reception',     'player', 'receiving',  false, false, false, 71),
    ('NFL', 'long_field_goal_made',    'Longest FG Made',       'player', 'kicking',    false, false, false, 72),
    ('NFL', 'long_punt',               'Longest Punt',          'player', 'special',    false, false, false, 73),
    ('NFL', 'long_kick_return',        'Longest Kick Return',   'player', 'special',    false, false, false, 74),
    ('NFL', 'long_punt_return',        'Longest Punt Return',   'player', 'special',    false, false, false, 75),
    ('NFL', 'avg_punt_yards',          'Yards/Punt',            'player', 'special',    false, true,  true,  76),
    ('NFL', 'yards_per_kick_return',   'Yards/Kick Return',     'player', 'special',    false, true,  true,  77),
    ('NFL', 'yards_per_punt_return',   'Yards/Punt Return',     'player', 'special',    false, true,  true,  78),
    ('NFL', 'interception_yards',      'INT Return Yards',      'player', 'defensive',  false, false, false, 56),
    ('NFL', 'fumbles_touchdowns',      'Fumble Return TDs',     'player', 'defensive',  false, false, false, 57),
    ('NFL', 'fumbles_recovered',       'Fumbles Recovered',     'player', 'defensive',  false, false, false, 58)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- NFL team stats
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('NFL', 'wins',              'Wins',                'team', 'standings', false, false, true,   1),
    ('NFL', 'losses',            'Losses',              'team', 'standings', true,  false, true,   2),
    ('NFL', 'ties',              'Ties',                'team', 'standings', false, false, false,  3),
    ('NFL', 'points_for',        'Points For',          'team', 'scoring',  false, false, true,   4),
    ('NFL', 'points_against',    'Points Against',      'team', 'scoring',  true,  false, true,   5),
    ('NFL', 'point_differential','Point Differential',  'team', 'scoring',  false, false, true,   6),
    ('NFL', 'win_pct',           'Win Percentage',      'team', 'standings', false, true,  true,   7),
    ('NFL', 'passing_yards',           'Passing Yards',         'team', 'offense',   false, false, true,  10),
    ('NFL', 'passing_touchdowns',      'Passing TDs',           'team', 'offense',   false, false, true,  11),
    ('NFL', 'passing_attempts',        'Pass Attempts',         'team', 'offense',   false, false, false, 12),
    ('NFL', 'passing_completions',     'Completions',           'team', 'offense',   false, false, false, 13),
    ('NFL', 'passing_interceptions',   'Interceptions Thrown',  'team', 'offense',   true,  false, true,  14),
    ('NFL', 'rushing_yards',           'Rushing Yards',         'team', 'offense',   false, false, true,  15),
    ('NFL', 'rushing_touchdowns',      'Rushing TDs',           'team', 'offense',   false, false, true,  16),
    ('NFL', 'rushing_attempts',        'Rush Attempts',         'team', 'offense',   false, false, false, 17),
    ('NFL', 'total_yards',             'Total Yards',           'team', 'offense',   false, true,  true,  18),
    ('NFL', 'defensive_sacks',         'Sacks',                 'team', 'defense',   false, false, true,  20),
    ('NFL', 'defensive_interceptions', 'Interceptions',         'team', 'defense',   false, false, true,  21),
    ('NFL', 'total_tackles',           'Total Tackles',         'team', 'defense',   false, false, false, 22),
    ('NFL', 'passes_defended',         'Passes Defended',       'team', 'defense',   false, false, true,  23),
    ('NFL', 'fumbles_lost',            'Fumbles Lost',          'team', 'turnovers', true,  false, true,  30),
    ('NFL', 'turnovers',               'Total Turnovers',       'team', 'turnovers', true,  true,  true,  31),
    ('NFL', 'field_goals_made',        'FG Made',               'team', 'kicking',   false, false, false, 40),
    ('NFL', 'field_goal_attempts',     'FG Attempts',           'team', 'kicking',   false, false, false, 41),
    ('NFL', 'field_goal_pct',          'FG Percentage',         'team', 'kicking',   false, true,  true,  42),
    ('NFL', 'extra_points_made',       'Extra Points Made',     'team', 'kicking',   false, false, false, 43),
    ('NFL', 'games_played',            'Games Played',          'team', 'general',   false, false, false,  0),
    ('NFL', 'points_per_game',         'Points/Game',           'team', 'scoring',   false, true,  true,   8),
    ('NFL', 'points_allowed_per_game', 'Points Allowed/Game',   'team', 'scoring',   true,  true,  true,   9),
    ('NFL', 'yards_per_game',          'Total Yards/Game',      'team', 'offense',   false, true,  true,  19),
    ('NFL', 'yards_per_rush_attempt',  'Yards/Carry',           'team', 'offense',   false, true,  true,  24),
    ('NFL', 'yards_per_pass_attempt',  'Yards/Attempt',         'team', 'offense',   false, true,  true,  25),
    ('NFL', 'passing_completion_pct',  'Completion %',          'team', 'offense',   false, true,  true,  26),
    ('NFL', 'solo_tackles',            'Solo Tackles',          'team', 'defense',   false, false, false, 27),
    ('NFL', 'tackles_for_loss',        'Tackles for Loss',      'team', 'defense',   false, false, true,  28),
    ('NFL', 'qb_hits',                 'QB Hits',               'team', 'defense',   false, false, true,  29),
    ('NFL', 'interception_touchdowns', 'INT Return TDs',        'team', 'defense',   false, false, false, 32),
    ('NFL', 'fumbles_recovered',       'Fumbles Recovered',     'team', 'defense',   false, false, true,  33),
    ('NFL', 'fumbles_touchdowns',      'Fumble Return TDs',     'team', 'defense',   false, false, false, 34),
    ('NFL', 'fumbles',                 'Fumbles',               'team', 'turnovers', true,  false, false, 35),
    ('NFL', 'takeaways',               'Takeaways',             'team', 'defense',   false, true,  true,  36),
    ('NFL', 'turnover_differential',   'Turnover Differential', 'team', 'turnovers', false, true,  true,  37),
    ('NFL', 'punts',                   'Punts',                 'team', 'special',   false, false, false, 50),
    ('NFL', 'punt_yards',              'Punt Yards',            'team', 'special',   false, false, false, 51),
    ('NFL', 'punts_inside_20',         'Punts Inside 20',       'team', 'special',   false, false, true,  52),
    ('NFL', 'gross_avg_punt_yards',    'Yards/Punt',            'team', 'special',   false, true,  true,  53),
    ('NFL', 'touchbacks',              'Touchbacks',            'team', 'special',   false, false, false, 54),
    ('NFL', 'kick_returns',            'Kick Returns',          'team', 'special',   false, false, false, 55),
    ('NFL', 'kick_return_yards',       'Kick Return Yards',     'team', 'special',   false, false, true,  56),
    ('NFL', 'kick_return_touchdowns',  'Kick Return TDs',       'team', 'special',   false, false, false, 57),
    ('NFL', 'yards_per_kick_return',   'Yards/Kick Return',     'team', 'special',   false, true,  true,  58),
    ('NFL', 'punt_returns',            'Punt Returns',          'team', 'special',   false, false, false, 59),
    ('NFL', 'punt_return_yards',       'Punt Return Yards',     'team', 'special',   false, false, true,  60),
    ('NFL', 'punt_return_touchdowns',  'Punt Return TDs',       'team', 'special',   false, false, false, 61),
    ('NFL', 'yards_per_punt_return',   'Yards/Punt Return',     'team', 'special',   false, true,  true,  62),
    ('NFL', 'qbr',                     'Passer Rating (ESPN)',  'team', 'offense',   false, false, true,  63),
    ('NFL', 'qb_rating',               'Passer Rating (NFL)',   'team', 'offense',   false, false, true,  64),
    -- Team-only datapoints from BDL /nfl/v1/team_stats (not derivable from player sums)
    ('NFL', 'first_downs',             'First Downs',           'team', 'offense',   false, false, true,  65),
    ('NFL', 'first_downs_passing',     'First Downs (Passing)', 'team', 'offense',   false, false, true,  66),
    ('NFL', 'first_downs_rushing',     'First Downs (Rushing)', 'team', 'offense',   false, false, true,  67),
    ('NFL', 'first_downs_penalty',     'First Downs (Penalty)', 'team', 'offense',   false, false, false, 68),
    ('NFL', 'third_down_attempts',     'Third Down Attempts',   'team', 'offense',   false, false, false, 70),
    ('NFL', 'third_down_conversions',  'Third Down Conversions','team', 'offense',   false, false, true,  71),
    ('NFL', 'third_down_pct',          'Third Down %',          'team', 'offense',   false, true,  true,  72),
    ('NFL', 'fourth_down_attempts',    'Fourth Down Attempts',  'team', 'offense',   false, false, false, 73),
    ('NFL', 'fourth_down_conversions', 'Fourth Down Conversions','team','offense',   false, false, true,  74),
    ('NFL', 'fourth_down_pct',         'Fourth Down %',         'team', 'offense',   false, true,  true,  75),
    ('NFL', 'red_zone_attempts',       'Red Zone Attempts',     'team', 'offense',   false, false, true,  76),
    ('NFL', 'red_zone_scores',         'Red Zone Scores',       'team', 'offense',   false, false, true,  77),
    ('NFL', 'red_zone_pct',            'Red Zone %',            'team', 'offense',   false, true,  true,  78),
    ('NFL', 'total_drives',            'Total Drives',          'team', 'offense',   false, false, false, 79),
    ('NFL', 'total_offensive_plays',   'Total Offensive Plays', 'team', 'offense',   false, false, false, 80),
    ('NFL', 'yards_per_play',          'Yards/Play',            'team', 'offense',   false, true,  true,  81),
    ('NFL', 'net_passing_yards',       'Net Passing Yards',     'team', 'offense',   false, false, true,  82),
    ('NFL', 'sack_yards_lost',         'Sack Yards Lost',       'team', 'offense',   true,  false, false, 83),
    ('NFL', 'possession_time_seconds', 'Possession Time (s)',   'team', 'general',   false, false, true,  84),
    ('NFL', 'avg_possession_seconds',  'Avg Possession/Game (s)','team','general',   false, true,  true,  85),
    ('NFL', 'penalties',               'Penalties',             'team', 'discipline',true,  false, true,  86),
    ('NFL', 'penalty_yards',           'Penalty Yards',         'team', 'discipline',true,  false, true,  87),
    ('NFL', 'defensive_touchdowns',    'Defensive TDs',         'team', 'defense',   false, false, true,  88)
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

-- Drop legacy views from pre-consolidation (players, player_stats, teams, team_stats)
DROP VIEW IF EXISTS nfl.players;
DROP VIEW IF EXISTS nfl.player_stats;
DROP VIEW IF EXISTS nfl.teams;
DROP VIEW IF EXISTS nfl.team_stats;

-- Combined player profile + stats
CREATE OR REPLACE VIEW nfl.player AS
SELECT
    p.id, p.name, p.first_name, p.last_name, p.position,
    p.detailed_position, p.nationality, p.date_of_birth::text AS date_of_birth,
    p.height, p.weight, p.photo_url, p.team_id, p.league_id,
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
    t.venue_name, t.venue_capacity,
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
    SELECT
        p.id,
        'player'::text AS type,
        p.name,
        p.first_name,
        p.last_name,
        p.position,
        p.detailed_position,
        p.nationality,
        p.date_of_birth::text AS date_of_birth,
        p.height,
        p.weight,
        p.photo_url,
        p.team_id,
        NULL::int AS league_id,
        NULL::text AS league_name,
        t.short_code AS team_abbr,
        t.name AS team_name,
        t.logo_url AS team_logo_url,
        jsonb_build_array(
            LOWER(p.first_name),
            LOWER(p.last_name),
            LOWER(REPLACE(p.name, ' ', '')),
            LOWER(COALESCE(t.short_code, '')),
            LOWER(COALESCE(t.name, '')),
            unaccent(LOWER(p.first_name)),
            unaccent(LOWER(p.last_name)),
            unaccent(LOWER(REPLACE(p.name, ' ', ''))),
            unaccent(LOWER(COALESCE(t.name, '')))
        ) AS search_tokens,
        -- Pass the full player meta blob through. The frontend decides
        -- which fields to render; the backend doesn't curate.
        COALESCE(p.meta, '{}'::jsonb) || jsonb_build_object('display_name', p.name) AS meta
    FROM public.players p
    LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
    WHERE p.sport = 'NFL'
      AND (
          EXISTS (
              SELECT 1 FROM public.player_stats ps
              WHERE ps.player_id = p.id AND ps.sport = p.sport
          )
          -- Rookie exemption: BDL labels first-year players "Rookie" in
          -- meta.experience, so unplayed rookies stay in autofill.
          OR p.meta->>'experience' ILIKE 'rookie%'
      )
UNION ALL
    SELECT
        t.id,
        'team'::text AS type,
        t.name,
        NULL::text AS first_name,
        NULL::text AS last_name,
        t.conference AS position,
        t.division AS detailed_position,
        t.country AS nationality,
        NULL::text AS date_of_birth,
        NULL::text AS height,
        NULL::text AS weight,
        t.logo_url AS photo_url,
        NULL::int AS team_id,
        NULL::int AS league_id,
        NULL::text AS league_name,
        t.short_code AS team_abbr,
        NULL::text AS team_name,
        NULL::text AS team_logo_url,
        jsonb_build_array(
            LOWER(REPLACE(t.name, ' ', '')),
            LOWER(t.short_code),
            LOWER(t.city),
            LOWER(t.country),
            unaccent(LOWER(REPLACE(t.name, ' ', ''))),
            unaccent(LOWER(t.city))
        ) AS search_tokens,
        jsonb_build_object(
            'display_name', t.name,
            'abbreviation', t.short_code,
            'city', t.city,
            'country', t.country,
            'conference', t.conference,
            'division', t.division,
            'founded', t.founded,
            'venue_name', t.venue_name,
            'venue_capacity', t.venue_capacity
        ) AS meta
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
-- 6. EVENT -> SEASON AGGREGATION FUNCTIONS
-- ============================================================================

CREATE OR REPLACE FUNCTION nfl.aggregate_player_season(
    p_player_id INTEGER,
    p_season INTEGER,
    p_league_id INTEGER DEFAULT 0
)
RETURNS JSONB AS $$
WITH agg AS (
    SELECT
        COUNT(*)::numeric AS gp,
        -- Passing
        SUM(COALESCE((stats->>'passing_completions')::numeric, 0)) AS pass_cmp_sum,
        SUM(COALESCE((stats->>'passing_attempts')::numeric, 0)) AS pass_att_sum,
        SUM(COALESCE((stats->>'passing_yards')::numeric, 0)) AS pass_yds_sum,
        SUM(COALESCE((stats->>'passing_touchdowns')::numeric, 0)) AS pass_td_sum,
        SUM(COALESCE((stats->>'passing_interceptions')::numeric, 0)) AS pass_int_sum,
        SUM(COALESCE((stats->>'qbr')::numeric, 0)) AS qbr_sum,
        COUNT(*) FILTER (WHERE (stats->>'qbr') IS NOT NULL)::numeric AS qbr_games,
        SUM(COALESCE((stats->>'qb_rating')::numeric, 0)) AS qb_rating_sum,
        COUNT(*) FILTER (WHERE (stats->>'qb_rating') IS NOT NULL)::numeric AS qb_rating_games,
        SUM(COALESCE((stats->>'sacks')::numeric, 0)) AS sacks_taken_sum,
        SUM(COALESCE((stats->>'sacks_loss')::numeric, 0)) AS sack_yards_lost_sum,
        MAX(COALESCE((stats->>'long_pass')::numeric, 0)) AS long_pass_max,
        -- Rushing
        SUM(COALESCE((stats->>'rushing_attempts')::numeric, 0)) AS rush_att_sum,
        SUM(COALESCE((stats->>'rushing_yards')::numeric, 0)) AS rush_yds_sum,
        SUM(COALESCE((stats->>'rushing_touchdowns')::numeric, 0)) AS rush_td_sum,
        MAX(COALESCE((stats->>'long_rushing')::numeric, 0)) AS long_rushing_max,
        -- Receiving
        SUM(COALESCE((stats->>'receptions')::numeric, 0)) AS rec_sum,
        SUM(COALESCE((stats->>'receiving_targets')::numeric, 0)) AS tgt_sum,
        SUM(COALESCE((stats->>'receiving_yards')::numeric, 0)) AS rec_yds_sum,
        SUM(COALESCE((stats->>'receiving_touchdowns')::numeric, 0)) AS rec_td_sum,
        MAX(COALESCE((stats->>'long_reception')::numeric, 0)) AS long_reception_max,
        -- General ball-security
        SUM(COALESCE((stats->>'fumbles')::numeric, 0)) AS fum_sum,
        SUM(COALESCE((stats->>'fumbles_lost')::numeric, 0)) AS fum_lost_sum,
        -- Defense
        SUM(COALESCE((stats->>'total_tackles')::numeric, 0)) AS tackles_sum,
        SUM(COALESCE((stats->>'solo_tackles')::numeric, 0)) AS solo_tackles_sum,
        SUM(COALESCE((stats->>'defensive_sacks')::numeric, 0)) AS sacks_sum,
        SUM(COALESCE((stats->>'defensive_interceptions')::numeric, 0)) AS int_def_sum,
        SUM(COALESCE((stats->>'interception_touchdowns')::numeric, 0)) AS int_td_sum,
        SUM(COALESCE((stats->>'interception_yards')::numeric, 0)) AS int_yds_sum,
        SUM(COALESCE((stats->>'fumbles_recovered')::numeric, 0)) AS fum_rec_sum,
        SUM(COALESCE((stats->>'fumbles_touchdowns')::numeric, 0)) AS fum_td_sum,
        SUM(COALESCE((stats->>'tackles_for_loss')::numeric, 0)) AS tfl_sum,
        SUM(COALESCE((stats->>'passes_defended')::numeric, 0)) AS pd_sum,
        SUM(COALESCE((stats->>'qb_hits')::numeric, 0)) AS qbh_sum,
        -- Kicking
        SUM(COALESCE((stats->>'field_goal_attempts')::numeric, 0)) AS fg_att_sum,
        SUM(COALESCE((stats->>'field_goals_made')::numeric, 0)) AS fg_made_sum,
        SUM(COALESCE((stats->>'extra_points_made')::numeric, 0)) AS xp_sum,
        SUM(COALESCE((stats->>'total_points')::numeric, 0)) AS points_sum,
        SUM(COALESCE((stats->>'touchbacks')::numeric, 0)) AS touchback_sum,
        MAX(COALESCE((stats->>'long_field_goal_made')::numeric, 0)) AS long_fg_max,
        -- Special teams (BDL key → canonical key)
        SUM(COALESCE((stats->>'punts')::numeric, 0)) AS punts_sum,
        SUM(COALESCE((stats->>'punt_yards')::numeric, 0)) AS punt_yds_sum,
        SUM(COALESCE((stats->>'punts_inside_20')::numeric, 0)) AS punts_in20_sum,
        MAX(COALESCE((stats->>'long_punt')::numeric, 0)) AS long_punt_max,
        SUM(COALESCE((stats->>'kick_returns')::numeric, 0)) AS kr_sum,
        SUM(COALESCE((stats->>'kick_return_yards')::numeric, 0)) AS kr_yds_sum,
        SUM(COALESCE((stats->>'kick_return_touchdowns')::numeric, 0)) AS kr_td_sum,
        MAX(COALESCE((stats->>'long_kick_return')::numeric, 0)) AS long_kr_max,
        SUM(COALESCE((stats->>'punt_returns')::numeric, 0)) AS pr_sum,
        SUM(COALESCE((stats->>'punt_return_yards')::numeric, 0)) AS pr_yds_sum,
        SUM(COALESCE((stats->>'punt_return_touchdowns')::numeric, 0)) AS pr_td_sum,
        MAX(COALESCE((stats->>'long_punt_return')::numeric, 0)) AS long_pr_max
    FROM public.event_box_scores
    WHERE player_id = p_player_id
      AND sport = 'NFL'
      AND season = p_season
      AND league_id = p_league_id
      AND NOT (
          COALESCE((stats->>'passing_yards')::numeric, 0) = 0
          AND COALESCE((stats->>'rushing_yards')::numeric, 0) = 0
          AND COALESCE((stats->>'receiving_yards')::numeric, 0) = 0
          AND COALESCE((stats->>'total_tackles')::numeric, 0) = 0
          AND COALESCE((stats->>'fumbles')::numeric, 0) = 0
      )
)
SELECT CASE
    WHEN gp = 0 THEN '{}'::jsonb
    ELSE jsonb_strip_nulls(
        jsonb_build_object(
            'games_played', gp::int,
            'fumbles', fum_sum::int,
            'fumbles_lost', fum_lost_sum::int,
            -- Passing
            'passing_completions', pass_cmp_sum::int,
            'passing_attempts', pass_att_sum::int,
            'passing_yards', pass_yds_sum::int,
            'passing_touchdowns', pass_td_sum::int,
            'passing_interceptions', pass_int_sum::int,
            'passing_yards_per_game', ROUND(pass_yds_sum / gp, 1),
            'passing_completion_pct', CASE WHEN pass_att_sum > 0 THEN ROUND(pass_cmp_sum / pass_att_sum * 100, 1) END,
            'yards_per_pass_attempt', CASE WHEN pass_att_sum > 0 THEN ROUND(pass_yds_sum / pass_att_sum, 2) END,
            'qbr', CASE WHEN qbr_games > 0 THEN ROUND(qbr_sum / qbr_games, 1) END,
            'qb_rating', CASE WHEN qb_rating_games > 0 THEN ROUND(qb_rating_sum / qb_rating_games, 1) END,
            'sacks_taken', CASE WHEN sacks_taken_sum > 0 THEN sacks_taken_sum::int END,
            'sack_yards_lost', CASE WHEN sack_yards_lost_sum > 0 THEN sack_yards_lost_sum::int END,
            'long_pass', CASE WHEN long_pass_max > 0 THEN long_pass_max::int END,
            -- Rushing
            'rushing_attempts', rush_att_sum::int,
            'rushing_yards', rush_yds_sum::int,
            'rushing_touchdowns', rush_td_sum::int,
            'rushing_yards_per_game', ROUND(rush_yds_sum / gp, 1),
            'yards_per_rush_attempt', CASE WHEN rush_att_sum > 0 THEN ROUND(rush_yds_sum / rush_att_sum, 2) END,
            'long_rushing', CASE WHEN long_rushing_max > 0 THEN long_rushing_max::int END,
            -- Receiving
            'receptions', rec_sum::int,
            'receiving_targets', tgt_sum::int,
            'receiving_yards', rec_yds_sum::int,
            'receiving_touchdowns', rec_td_sum::int,
            'receiving_yards_per_game', ROUND(rec_yds_sum / gp, 1),
            'yards_per_reception', CASE WHEN rec_sum > 0 THEN ROUND(rec_yds_sum / rec_sum, 2) END,
            'long_reception', CASE WHEN long_reception_max > 0 THEN long_reception_max::int END,
            -- Defense
            'total_tackles', tackles_sum::int,
            'solo_tackles', solo_tackles_sum::int,
            'assist_tackles', GREATEST(tackles_sum - solo_tackles_sum, 0)::int,
            'defensive_sacks', ROUND(sacks_sum, 1),
            'defensive_interceptions', int_def_sum::int,
            'interception_touchdowns', int_td_sum::int,
            'interception_yards', CASE WHEN int_yds_sum > 0 THEN int_yds_sum::int END,
            'fumbles_recovered', fum_rec_sum::int,
            'fumbles_touchdowns', fum_td_sum::int,
            'tackles_for_loss', tfl_sum::int,
            'passes_defended', pd_sum::int,
            'qb_hits', qbh_sum::int
        ) || jsonb_build_object(
            -- Kicking
            'field_goal_attempts', fg_att_sum::int,
            'field_goals_made', fg_made_sum::int,
            'field_goal_pct', CASE WHEN fg_att_sum > 0 THEN ROUND(fg_made_sum / fg_att_sum * 100, 1) END,
            'long_field_goal_made', CASE WHEN long_fg_max > 0 THEN long_fg_max::int END,
            'extra_points_made', xp_sum::int,
            'total_points', points_sum::int,
            'touchbacks', touchback_sum::int,
            -- Special teams
            'punts', punts_sum::int,
            'punt_yards', punt_yds_sum::int,
            'punts_inside_20', punts_in20_sum::int,
            'avg_punt_yards', CASE WHEN punts_sum > 0 THEN ROUND(punt_yds_sum / punts_sum, 1) END,
            'long_punt', CASE WHEN long_punt_max > 0 THEN long_punt_max::int END,
            'kick_returns', kr_sum::int,
            'kick_return_yards', kr_yds_sum::int,
            'kick_return_touchdowns', kr_td_sum::int,
            'yards_per_kick_return', CASE WHEN kr_sum > 0 THEN ROUND(kr_yds_sum / kr_sum, 2) END,
            'long_kick_return', CASE WHEN long_kr_max > 0 THEN long_kr_max::int END,
            'punt_returner_returns', pr_sum::int,
            'punt_returner_return_yards', pr_yds_sum::int,
            'punt_return_touchdowns', pr_td_sum::int,
            'yards_per_punt_return', CASE WHEN pr_sum > 0 THEN ROUND(pr_yds_sum / pr_sum, 2) END,
            'long_punt_return', CASE WHEN long_pr_max > 0 THEN long_pr_max::int END
        )
    )
END
FROM agg;
$$ LANGUAGE sql STABLE;

CREATE OR REPLACE FUNCTION nfl.aggregate_team_season(
    p_team_id INTEGER,
    p_season INTEGER,
    p_league_id INTEGER DEFAULT 0
)
RETURNS JSONB AS $$
WITH agg AS (
    SELECT
        COUNT(*)::numeric AS gp,
        SUM(CASE WHEN opp.score IS NOT NULL AND ets.score > opp.score THEN 1 ELSE 0 END)::numeric AS wins,
        SUM(CASE WHEN opp.score IS NOT NULL AND ets.score < opp.score THEN 1 ELSE 0 END)::numeric AS losses,
        SUM(CASE WHEN opp.score IS NOT NULL AND ets.score = opp.score THEN 1 ELSE 0 END)::numeric AS ties,
        SUM(COALESCE(ets.score, 0))::numeric AS pf_sum,
        SUM(COALESCE(opp.score, 0))::numeric AS pa_sum,
        -- Offense (passing)
        SUM(COALESCE((ets.stats->>'passing_yards')::numeric, 0))        AS pass_yds_sum,
        SUM(COALESCE((ets.stats->>'passing_touchdowns')::numeric, 0))   AS pass_td_sum,
        SUM(COALESCE((ets.stats->>'passing_attempts')::numeric, 0))     AS pass_att_sum,
        SUM(COALESCE((ets.stats->>'passing_completions')::numeric, 0))  AS pass_cmp_sum,
        SUM(COALESCE((ets.stats->>'passing_interceptions')::numeric, 0))AS pass_int_sum,
        AVG(NULLIF((ets.stats->>'qbr')::numeric, NULL))                 AS qbr_avg,
        AVG(NULLIF((ets.stats->>'qb_rating')::numeric, NULL))           AS qb_rating_avg,
        -- Offense (rushing)
        SUM(COALESCE((ets.stats->>'rushing_yards')::numeric, 0))        AS rush_yds_sum,
        SUM(COALESCE((ets.stats->>'rushing_touchdowns')::numeric, 0))   AS rush_td_sum,
        SUM(COALESCE((ets.stats->>'rushing_attempts')::numeric, 0))     AS rush_att_sum,
        -- Defense
        SUM(COALESCE((ets.stats->>'defensive_sacks')::numeric, 0))      AS sacks_sum,
        SUM(COALESCE((ets.stats->>'defensive_interceptions')::numeric, 0)) AS int_def_sum,
        SUM(COALESCE((ets.stats->>'interception_touchdowns')::numeric, 0)) AS int_td_sum,
        SUM(COALESCE((ets.stats->>'total_tackles')::numeric, 0))        AS tackles_sum,
        SUM(COALESCE((ets.stats->>'solo_tackles')::numeric, 0))         AS solo_tackles_sum,
        SUM(COALESCE((ets.stats->>'passes_defended')::numeric, 0))      AS pd_sum,
        SUM(COALESCE((ets.stats->>'tackles_for_loss')::numeric, 0))     AS tfl_sum,
        SUM(COALESCE((ets.stats->>'qb_hits')::numeric, 0))              AS qbh_sum,
        SUM(COALESCE((ets.stats->>'fumbles_recovered')::numeric, 0))    AS fum_rec_sum,
        SUM(COALESCE((ets.stats->>'fumbles_touchdowns')::numeric, 0))   AS fum_td_sum,
        -- Turnovers
        SUM(COALESCE((ets.stats->>'fumbles')::numeric, 0))              AS fum_sum,
        SUM(COALESCE((ets.stats->>'fumbles_lost')::numeric, 0))         AS fum_lost_sum,
        SUM(COALESCE((opp.stats->>'fumbles_lost')::numeric, 0))         AS opp_fum_lost_sum,
        SUM(COALESCE((opp.stats->>'passing_interceptions')::numeric, 0))AS opp_pass_int_sum,
        -- Kicking
        SUM(COALESCE((ets.stats->>'field_goals_made')::numeric, 0))     AS fg_made_sum,
        SUM(COALESCE((ets.stats->>'field_goal_attempts')::numeric, 0))  AS fg_att_sum,
        SUM(COALESCE((ets.stats->>'extra_points_made')::numeric, 0))    AS xp_sum,
        -- Special teams
        SUM(COALESCE((ets.stats->>'punts')::numeric, 0))                AS punts_sum,
        SUM(COALESCE((ets.stats->>'punt_yards')::numeric, 0))           AS punt_yds_sum,
        SUM(COALESCE((ets.stats->>'punts_inside_20')::numeric, 0))      AS punts_in20_sum,
        SUM(COALESCE((ets.stats->>'touchbacks')::numeric, 0))           AS touchback_sum,
        SUM(COALESCE((ets.stats->>'kick_returns')::numeric, 0))         AS kr_sum,
        SUM(COALESCE((ets.stats->>'kick_return_yards')::numeric, 0))    AS kr_yds_sum,
        SUM(COALESCE((ets.stats->>'kick_return_touchdowns')::numeric, 0)) AS kr_td_sum,
        SUM(COALESCE((ets.stats->>'punt_returns')::numeric, 0))         AS pr_sum,
        SUM(COALESCE((ets.stats->>'punt_return_yards')::numeric, 0))    AS pr_yds_sum,
        SUM(COALESCE((ets.stats->>'punt_return_touchdowns')::numeric, 0)) AS pr_td_sum,
        -- Team-only (BDL /nfl/v1/team_stats)
        SUM(COALESCE((ets.stats->>'first_downs')::numeric, 0))                AS first_downs_sum,
        SUM(COALESCE((ets.stats->>'first_downs_passing')::numeric, 0))        AS first_downs_pass_sum,
        SUM(COALESCE((ets.stats->>'first_downs_rushing')::numeric, 0))        AS first_downs_rush_sum,
        SUM(COALESCE((ets.stats->>'first_downs_penalty')::numeric, 0))        AS first_downs_pen_sum,
        SUM(COALESCE((ets.stats->>'third_down_attempts')::numeric, 0))        AS third_att_sum,
        SUM(COALESCE((ets.stats->>'third_down_conversions')::numeric, 0))     AS third_conv_sum,
        SUM(COALESCE((ets.stats->>'fourth_down_attempts')::numeric, 0))       AS fourth_att_sum,
        SUM(COALESCE((ets.stats->>'fourth_down_conversions')::numeric, 0))    AS fourth_conv_sum,
        SUM(COALESCE((ets.stats->>'red_zone_attempts')::numeric, 0))          AS rz_att_sum,
        SUM(COALESCE((ets.stats->>'red_zone_scores')::numeric, 0))            AS rz_score_sum,
        SUM(COALESCE((ets.stats->>'total_drives')::numeric, 0))               AS drives_sum,
        SUM(COALESCE((ets.stats->>'total_offensive_plays')::numeric, 0))      AS plays_sum,
        SUM(COALESCE((ets.stats->>'net_passing_yards')::numeric, 0))          AS net_pass_yds_sum,
        SUM(COALESCE((ets.stats->>'sack_yards_lost')::numeric, 0))            AS sack_yds_lost_sum,
        SUM(COALESCE((ets.stats->>'possession_time_seconds')::numeric, 0))    AS poss_seconds_sum,
        SUM(COALESCE((ets.stats->>'penalties')::numeric, 0))                  AS penalties_sum,
        SUM(COALESCE((ets.stats->>'penalty_yards')::numeric, 0))              AS penalty_yds_sum,
        SUM(COALESCE((ets.stats->>'defensive_touchdowns')::numeric, 0))       AS def_td_sum
    FROM public.event_team_stats ets
    LEFT JOIN public.event_team_stats opp
        ON opp.fixture_id = ets.fixture_id
       AND opp.sport = ets.sport
       AND opp.season = ets.season
       AND opp.league_id = ets.league_id
       AND opp.team_id <> ets.team_id
    WHERE ets.team_id = p_team_id
      AND ets.sport = 'NFL'
      AND ets.season = p_season
      AND ets.league_id = p_league_id
)
SELECT CASE
    WHEN gp = 0 THEN '{}'::jsonb
    ELSE jsonb_strip_nulls(
        jsonb_build_object(
            'games_played', gp::int,
            'wins', wins::int,
            'losses', losses::int,
            'ties', ties::int,
            'points_for', pf_sum::int,
            'points_against', pa_sum::int,
            'point_differential', (pf_sum - pa_sum)::int,
            'points_per_game', ROUND(pf_sum / gp, 1),
            'points_allowed_per_game', ROUND(pa_sum / gp, 1),
            -- Offense
            'passing_yards', pass_yds_sum::int,
            'passing_touchdowns', pass_td_sum::int,
            'passing_attempts', pass_att_sum::int,
            'passing_completions', pass_cmp_sum::int,
            'passing_interceptions', pass_int_sum::int,
            'passing_completion_pct', CASE WHEN pass_att_sum > 0 THEN ROUND(pass_cmp_sum / pass_att_sum * 100, 1) END,
            'yards_per_pass_attempt', CASE WHEN pass_att_sum > 0 THEN ROUND(pass_yds_sum / pass_att_sum, 2) END,
            'qbr', ROUND(qbr_avg, 1),
            'qb_rating', ROUND(qb_rating_avg, 1),
            'rushing_yards', rush_yds_sum::int,
            'rushing_touchdowns', rush_td_sum::int,
            'rushing_attempts', rush_att_sum::int,
            'yards_per_rush_attempt', CASE WHEN rush_att_sum > 0 THEN ROUND(rush_yds_sum / rush_att_sum, 2) END,
            'total_yards', (pass_yds_sum + rush_yds_sum)::int,
            'yards_per_game', ROUND((pass_yds_sum + rush_yds_sum) / gp, 1),
            -- Defense
            'defensive_sacks', ROUND(sacks_sum, 1),
            'defensive_interceptions', int_def_sum::int,
            'interception_touchdowns', int_td_sum::int,
            'total_tackles', tackles_sum::int,
            'solo_tackles', solo_tackles_sum::int,
            'tackles_for_loss', tfl_sum::int,
            'qb_hits', qbh_sum::int,
            'passes_defended', pd_sum::int,
            'fumbles_recovered', fum_rec_sum::int,
            'fumbles_touchdowns', fum_td_sum::int
        ) || jsonb_build_object(
            -- Turnovers
            'fumbles', fum_sum::int,
            'fumbles_lost', fum_lost_sum::int,
            'turnovers', (pass_int_sum + fum_lost_sum)::int,
            'takeaways', (opp_pass_int_sum + opp_fum_lost_sum)::int,
            'turnover_differential', ((opp_pass_int_sum + opp_fum_lost_sum) - (pass_int_sum + fum_lost_sum))::int,
            -- Kicking
            'field_goals_made', fg_made_sum::int,
            'field_goal_attempts', fg_att_sum::int,
            'field_goal_pct', CASE WHEN fg_att_sum > 0 THEN ROUND(fg_made_sum / fg_att_sum * 100, 1) END,
            'extra_points_made', xp_sum::int,
            -- Special teams
            'punts', punts_sum::int,
            'punt_yards', punt_yds_sum::int,
            'punts_inside_20', punts_in20_sum::int,
            'gross_avg_punt_yards', CASE WHEN punts_sum > 0 THEN ROUND(punt_yds_sum / punts_sum, 1) END,
            'touchbacks', touchback_sum::int,
            'kick_returns', kr_sum::int,
            'kick_return_yards', kr_yds_sum::int,
            'kick_return_touchdowns', kr_td_sum::int,
            'yards_per_kick_return', CASE WHEN kr_sum > 0 THEN ROUND(kr_yds_sum / kr_sum, 2) END,
            'punt_returns', pr_sum::int,
            'punt_return_yards', pr_yds_sum::int,
            'punt_return_touchdowns', pr_td_sum::int,
            'yards_per_punt_return', CASE WHEN pr_sum > 0 THEN ROUND(pr_yds_sum / pr_sum, 2) END
        ) || jsonb_build_object(
            -- Team-only aggregates (BDL /nfl/v1/team_stats)
            'first_downs', first_downs_sum::int,
            'first_downs_passing', first_downs_pass_sum::int,
            'first_downs_rushing', first_downs_rush_sum::int,
            'first_downs_penalty', first_downs_pen_sum::int,
            'third_down_attempts', third_att_sum::int,
            'third_down_conversions', third_conv_sum::int,
            'third_down_pct', CASE WHEN third_att_sum > 0 THEN ROUND(third_conv_sum / third_att_sum * 100, 1) END,
            'fourth_down_attempts', fourth_att_sum::int,
            'fourth_down_conversions', fourth_conv_sum::int,
            'fourth_down_pct', CASE WHEN fourth_att_sum > 0 THEN ROUND(fourth_conv_sum / fourth_att_sum * 100, 1) END,
            'red_zone_attempts', rz_att_sum::int,
            'red_zone_scores', rz_score_sum::int,
            'red_zone_pct', CASE WHEN rz_att_sum > 0 THEN ROUND(rz_score_sum / rz_att_sum * 100, 1) END,
            'total_drives', drives_sum::int,
            'total_offensive_plays', plays_sum::int,
            'yards_per_play', CASE WHEN plays_sum > 0 THEN ROUND((pass_yds_sum + rush_yds_sum) / plays_sum, 2) END,
            'net_passing_yards', net_pass_yds_sum::int,
            'sack_yards_lost', sack_yds_lost_sum::int,
            'possession_time_seconds', poss_seconds_sum::int,
            'avg_possession_seconds', CASE WHEN gp > 0 THEN ROUND(poss_seconds_sum / gp, 1) END,
            'penalties', penalties_sum::int,
            'penalty_yards', penalty_yds_sum::int,
            'defensive_touchdowns', def_td_sum::int
        )
    )
END
FROM agg;
$$ LANGUAGE sql STABLE;

-- ============================================================================
-- 7. GRANTS
-- ============================================================================

GRANT USAGE ON SCHEMA nfl TO web_anon, web_user;
GRANT SELECT ON ALL TABLES IN SCHEMA nfl TO web_anon, web_user;
GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA nfl TO web_anon, web_user;
