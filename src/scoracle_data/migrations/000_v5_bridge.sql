-- Scoracle Data — V5 Bridge Migration
-- Created: 2026-02-11
-- Purpose: Bridge the v4.1 schema to support v5 unified seeders.
--
-- The live database (v4.1) uses sport-prefixed stats tables with typed columns
-- and UNION ALL views named `players`/`teams` over them.
-- The v5 seeders need real base tables with (id, sport) composite keys.
--
-- This migration:
--   1. Drops the v4 union views for players/teams
--   2. Creates real v5 base tables for players/teams
--   3. Creates player_stats/team_stats with JSONB
--   4. Adds compatibility columns to leagues
--   5. Sets up derived stats triggers + percentile function
--
-- The v4 base tables (nba_player_profiles, etc.) remain intact as historical data.

-- ============================================================================
-- 1. REPLACE V4 UNION VIEWS WITH V5 BASE TABLES
-- ============================================================================

DROP VIEW IF EXISTS players CASCADE;
DROP VIEW IF EXISTS teams CASCADE;

-- Drop entities_minimal if it depends on the views (may be view or table)
DROP TABLE IF EXISTS entities_minimal CASCADE;
DROP VIEW IF EXISTS entities_minimal CASCADE;

CREATE TABLE players (
    id INTEGER NOT NULL,
    sport TEXT NOT NULL REFERENCES sports(id),
    name TEXT NOT NULL,
    first_name TEXT,
    last_name TEXT,
    position TEXT,
    detailed_position TEXT,
    nationality TEXT,
    date_of_birth DATE,
    height_cm INTEGER,
    weight_kg INTEGER,
    photo_url TEXT,
    team_id INTEGER,
    league_id INTEGER,
    meta JSONB DEFAULT '{}',
    raw_response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id, sport)
);

CREATE INDEX idx_players_sport ON players(sport);
CREATE INDEX idx_players_team ON players(team_id);
CREATE INDEX idx_players_name ON players(name);
CREATE INDEX idx_players_position ON players(sport, position);

CREATE TABLE teams (
    id INTEGER NOT NULL,
    sport TEXT NOT NULL REFERENCES sports(id),
    name TEXT NOT NULL,
    short_code TEXT,
    country TEXT,
    city TEXT,
    logo_url TEXT,
    league_id INTEGER,
    conference TEXT,
    division TEXT,
    founded INTEGER,
    venue_name TEXT,
    venue_capacity INTEGER,
    meta JSONB DEFAULT '{}',
    raw_response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id, sport)
);

CREATE INDEX idx_teams_sport ON teams(sport);
CREATE INDEX idx_teams_name ON teams(name);

-- Add 'sportmonks_id' and 'is_benchmark' to leagues for v5 compatibility
ALTER TABLE leagues ADD COLUMN IF NOT EXISTS sportmonks_id INTEGER;
ALTER TABLE leagues ADD COLUMN IF NOT EXISTS is_benchmark BOOLEAN DEFAULT false;
ALTER TABLE leagues ADD COLUMN IF NOT EXISTS sport TEXT;
ALTER TABLE leagues ADD COLUMN IF NOT EXISTS handicap DECIMAL;

-- Populate sport from sport_id
UPDATE leagues SET sport = sport_id WHERE sport IS NULL;

-- SportMonks league IDs are the canonical IDs:
-- PL=8, Bundesliga=82, La Liga=564, Serie A=384, Ligue 1=301
UPDATE leagues SET sportmonks_id = id WHERE sportmonks_id IS NULL;

-- Mark top 5 European leagues as benchmarks
UPDATE leagues SET is_benchmark = true
WHERE id IN (8, 82, 564, 384, 301);  -- PL, Bundesliga, La Liga, Serie A, Ligue 1

-- ============================================================================
-- 2. CREATE V5 UNIFIED STATS TABLES
-- ============================================================================

CREATE TABLE IF NOT EXISTS player_stats (
    player_id INTEGER NOT NULL,
    sport TEXT NOT NULL REFERENCES sports(id),
    season INTEGER NOT NULL,
    league_id INTEGER NOT NULL DEFAULT 0,
    team_id INTEGER,
    stats JSONB NOT NULL DEFAULT '{}',
    percentiles JSONB DEFAULT '{}',
    raw_response JSONB,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (player_id, sport, season, league_id)
);

CREATE INDEX IF NOT EXISTS idx_player_stats_sport_season ON player_stats(sport, season);
CREATE INDEX IF NOT EXISTS idx_player_stats_team ON player_stats(team_id);
CREATE INDEX IF NOT EXISTS idx_player_stats_league ON player_stats(league_id) WHERE league_id > 0;

CREATE TABLE IF NOT EXISTS team_stats (
    team_id INTEGER NOT NULL,
    sport TEXT NOT NULL REFERENCES sports(id),
    season INTEGER NOT NULL,
    league_id INTEGER NOT NULL DEFAULT 0,
    stats JSONB NOT NULL DEFAULT '{}',
    percentiles JSONB DEFAULT '{}',
    raw_response JSONB,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (team_id, sport, season, league_id)
);

CREATE INDEX IF NOT EXISTS idx_team_stats_sport_season ON team_stats(sport, season);
CREATE INDEX IF NOT EXISTS idx_team_stats_league ON team_stats(league_id) WHERE league_id > 0;

-- ============================================================================
-- 3. STAT DEFINITIONS TABLE
-- ============================================================================

CREATE TABLE IF NOT EXISTS stat_definitions (
    id SERIAL PRIMARY KEY,
    sport TEXT NOT NULL REFERENCES sports(id),
    key_name TEXT NOT NULL,
    display_name TEXT NOT NULL,
    entity_type TEXT NOT NULL CHECK (entity_type IN ('player', 'team')),
    category TEXT,
    is_inverse BOOLEAN NOT NULL DEFAULT false,
    is_derived BOOLEAN NOT NULL DEFAULT false,
    is_percentile_eligible BOOLEAN NOT NULL DEFAULT false,
    sort_order INTEGER NOT NULL DEFAULT 0,
    UNIQUE(sport, key_name, entity_type)
);

CREATE INDEX IF NOT EXISTS idx_stat_definitions_sport
    ON stat_definitions(sport, entity_type);

-- Seed stat definitions (NBA player)
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('NBA', 'games_played',  'Games Played',       'player', 'general',   false, false, false,  1),
    ('NBA', 'minutes',       'Minutes Per Game',    'player', 'general',   false, false, true,   2),
    ('NBA', 'pts',           'Points Per Game',     'player', 'scoring',   false, false, true,   3),
    ('NBA', 'reb',           'Rebounds Per Game',   'player', 'rebounding',false, false, true,   4),
    ('NBA', 'ast',           'Assists Per Game',    'player', 'passing',   false, false, true,   5),
    ('NBA', 'stl',           'Steals Per Game',     'player', 'defensive', false, false, true,   6),
    ('NBA', 'blk',           'Blocks Per Game',     'player', 'defensive', false, false, true,   7),
    ('NBA', 'oreb',          'Off Rebounds/Game',   'player', 'rebounding',false, false, false,  8),
    ('NBA', 'dreb',          'Def Rebounds/Game',   'player', 'rebounding',false, false, false,  9),
    ('NBA', 'turnover',      'Turnovers Per Game',  'player', 'general',   true,  false, true,  10),
    ('NBA', 'pf',            'Fouls Per Game',      'player', 'general',   true,  false, false, 11),
    ('NBA', 'plus_minus',    'Plus/Minus',          'player', 'advanced',  false, false, true,  12),
    ('NBA', 'fg_pct',        'Field Goal %',        'player', 'shooting',  false, false, true,  20),
    ('NBA', 'fg3_pct',       'Three-Point %',       'player', 'shooting',  false, false, true,  21),
    ('NBA', 'ft_pct',        'Free Throw %',        'player', 'shooting',  false, false, true,  22),
    ('NBA', 'fgm',           'Field Goals Made',    'player', 'shooting',  false, false, false, 23),
    ('NBA', 'fga',           'Field Goals Attempted','player', 'shooting',  false, false, false, 24),
    ('NBA', 'fg3m',          'Three-Pointers Made', 'player', 'shooting',  false, false, false, 25),
    ('NBA', 'fg3a',          'Three-Pointers Att',  'player', 'shooting',  false, false, false, 26),
    ('NBA', 'ftm',           'Free Throws Made',    'player', 'shooting',  false, false, false, 27),
    ('NBA', 'fta',           'Free Throws Attempted','player', 'shooting', false, false, false, 28),
    ('NBA', 'pts_per_36',    'Points Per 36 Min',   'player', 'advanced',  false, true,  true,  30),
    ('NBA', 'reb_per_36',    'Rebounds Per 36 Min',  'player', 'advanced',  false, true,  true,  31),
    ('NBA', 'ast_per_36',    'Assists Per 36 Min',   'player', 'advanced',  false, true,  true,  32),
    ('NBA', 'stl_per_36',    'Steals Per 36 Min',    'player', 'advanced',  false, true,  true,  33),
    ('NBA', 'blk_per_36',    'Blocks Per 36 Min',    'player', 'advanced',  false, true,  true,  34),
    ('NBA', 'true_shooting_pct', 'True Shooting %',  'player', 'advanced',  false, true,  true,  35),
    ('NBA', 'efficiency',    'Efficiency Rating',    'player', 'advanced',  false, true,  true,  36)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- Seed stat definitions (NBA team)
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('NBA', 'wins',          'Wins',                'team', 'standings',  false, false, true,   1),
    ('NBA', 'losses',        'Losses',              'team', 'standings',  true,  false, true,   2),
    ('NBA', 'games_played',  'Games Played',        'team', 'general',   false, false, false,  3),
    ('NBA', 'pts',           'Points Per Game',     'team', 'scoring',   false, false, true,   4),
    ('NBA', 'reb',           'Rebounds Per Game',    'team', 'rebounding',false, false, true,   5),
    ('NBA', 'ast',           'Assists Per Game',     'team', 'passing',   false, false, true,   6),
    ('NBA', 'fg_pct',        'Field Goal %',        'team', 'shooting',  false, false, true,   7),
    ('NBA', 'fg3_pct',       'Three-Point %',       'team', 'shooting',  false, false, true,   8),
    ('NBA', 'ft_pct',        'Free Throw %',        'team', 'shooting',  false, false, false,  9),
    ('NBA', 'win_pct',       'Win Percentage',      'team', 'standings',  false, true,  true,  10)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- Seed stat definitions (NFL player)
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
    ('NFL', 'td_int_ratio',            'TD/INT Ratio',          'player', 'passing',    false, true,  true,  18),
    ('NFL', 'catch_pct',               'Catch %',               'player', 'receiving',  false, true,  true,  37)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- Seed stat definitions (NFL team)
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('NFL', 'wins',              'Wins',               'team', 'standings',  false, false, true,   1),
    ('NFL', 'losses',            'Losses',             'team', 'standings',  true,  false, true,   2),
    ('NFL', 'ties',              'Ties',               'team', 'standings',  false, false, false,  3),
    ('NFL', 'points_for',       'Points For',          'team', 'scoring',   false, false, true,   4),
    ('NFL', 'points_against',   'Points Against',      'team', 'scoring',   true,  false, true,   5),
    ('NFL', 'point_differential','Point Differential',  'team', 'scoring',   false, false, true,   6),
    ('NFL', 'win_pct',           'Win Percentage',     'team', 'standings',  false, true,  true,   7)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- Seed stat definitions (Football player)
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('FOOTBALL', 'appearances',      'Appearances',         'player', 'general',    false, false, false,  1),
    ('FOOTBALL', 'lineups',          'Starting Lineups',    'player', 'general',    false, false, false,  2),
    ('FOOTBALL', 'minutes_played',   'Minutes Played',      'player', 'general',    false, false, true,   3),
    ('FOOTBALL', 'goals',            'Goals',               'player', 'scoring',    false, false, true,   10),
    ('FOOTBALL', 'assists',          'Assists',             'player', 'scoring',    false, false, true,   11),
    ('FOOTBALL', 'expected_goals',   'Expected Goals (xG)', 'player', 'scoring',    false, false, true,   12),
    ('FOOTBALL', 'shots_total',      'Total Shots',         'player', 'shooting',   false, false, false,  20),
    ('FOOTBALL', 'shots_on_target',  'Shots on Target',     'player', 'shooting',   false, false, false,  21),
    ('FOOTBALL', 'passes_total',     'Total Passes',        'player', 'passing',    false, false, false,  30),
    ('FOOTBALL', 'passes_accurate',  'Accurate Passes',     'player', 'passing',    false, false, false,  31),
    ('FOOTBALL', 'key_passes',       'Key Passes',          'player', 'passing',    false, false, true,   32),
    ('FOOTBALL', 'crosses_total',    'Total Crosses',       'player', 'passing',    false, false, false,  33),
    ('FOOTBALL', 'crosses_accurate', 'Accurate Crosses',    'player', 'passing',    false, false, false,  34),
    ('FOOTBALL', 'tackles',          'Tackles',             'player', 'defensive',  false, false, true,   40),
    ('FOOTBALL', 'interceptions',    'Interceptions',       'player', 'defensive',  false, false, true,   41),
    ('FOOTBALL', 'clearances',       'Clearances',          'player', 'defensive',  false, false, false,  42),
    ('FOOTBALL', 'blocks',           'Blocks',              'player', 'defensive',  false, false, false,  43),
    ('FOOTBALL', 'duels_total',      'Total Duels',         'player', 'duels',      false, false, false,  50),
    ('FOOTBALL', 'duels_won',        'Duels Won',           'player', 'duels',      false, false, false,  51),
    ('FOOTBALL', 'dribbles_attempts','Dribble Attempts',    'player', 'dribbling',  false, false, false,  55),
    ('FOOTBALL', 'dribbles_success', 'Successful Dribbles', 'player', 'dribbling',  false, false, false,  56),
    ('FOOTBALL', 'yellow_cards',     'Yellow Cards',        'player', 'discipline', true,  false, true,   60),
    ('FOOTBALL', 'red_cards',        'Red Cards',           'player', 'discipline', true,  false, true,   61),
    ('FOOTBALL', 'fouls_committed',  'Fouls Committed',     'player', 'discipline', true,  false, false,  62),
    ('FOOTBALL', 'fouls_drawn',      'Fouls Drawn',         'player', 'discipline', false, false, false,  63),
    ('FOOTBALL', 'saves',            'Saves',               'player', 'goalkeeper', false, false, true,   70),
    ('FOOTBALL', 'goals_conceded',   'Goals Conceded',      'player', 'goalkeeper', true,  false, true,   71),
    ('FOOTBALL', 'goals_per_90',     'Goals Per 90',        'player', 'scoring',    false, true,  true,   13),
    ('FOOTBALL', 'assists_per_90',   'Assists Per 90',      'player', 'scoring',    false, true,  true,   14),
    ('FOOTBALL', 'key_passes_per_90','Key Passes Per 90',   'player', 'passing',    false, true,  true,   35),
    ('FOOTBALL', 'shots_per_90',     'Shots Per 90',        'player', 'shooting',   false, true,  true,   22),
    ('FOOTBALL', 'tackles_per_90',   'Tackles Per 90',      'player', 'defensive',  false, true,  true,   44),
    ('FOOTBALL', 'interceptions_per_90','Interceptions/90', 'player', 'defensive',  false, true,  true,   45),
    ('FOOTBALL', 'shot_accuracy',    'Shot Accuracy %',     'player', 'shooting',   false, true,  true,   23),
    ('FOOTBALL', 'pass_accuracy',    'Pass Accuracy %',     'player', 'passing',    false, true,  true,   36),
    ('FOOTBALL', 'duel_success_rate','Duel Success Rate %', 'player', 'duels',      false, true,  true,   52),
    ('FOOTBALL', 'dribble_success_rate','Dribble Success %','player', 'dribbling',  false, true,  true,   57),
    ('FOOTBALL', 'goals_conceded_per_90','Goals Conceded/90','player','goalkeeper', true,  true,  true,   72),
    ('FOOTBALL', 'save_pct',         'Save Percentage %',   'player', 'goalkeeper', false, true,  true,   73)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- Seed stat definitions (Football team)
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('FOOTBALL', 'matches_played',   'Matches Played',      'team', 'standings',  false, false, false,  1),
    ('FOOTBALL', 'wins',             'Wins',                'team', 'standings',  false, false, true,   2),
    ('FOOTBALL', 'draws',            'Draws',               'team', 'standings',  false, false, false,  3),
    ('FOOTBALL', 'losses',           'Losses',              'team', 'standings',  true,  false, true,   4),
    ('FOOTBALL', 'goals_for',        'Goals For',           'team', 'scoring',    false, false, true,   5),
    ('FOOTBALL', 'goals_against',    'Goals Against',       'team', 'scoring',    true,  false, true,   6),
    ('FOOTBALL', 'goal_difference',  'Goal Difference',     'team', 'scoring',    false, false, true,   7),
    ('FOOTBALL', 'points',           'Points',              'team', 'standings',  false, false, true,   8),
    ('FOOTBALL', 'overall_points',   'Overall Points',      'team', 'standings',  false, false, false,  9),
    ('FOOTBALL', 'position',         'League Position',     'team', 'standings',  false, false, false, 10),
    ('FOOTBALL', 'home_played',      'Home Matches',        'team', 'home',       false, false, false, 20),
    ('FOOTBALL', 'home_won',         'Home Wins',           'team', 'home',       false, false, false, 21),
    ('FOOTBALL', 'home_draw',        'Home Draws',          'team', 'home',       false, false, false, 22),
    ('FOOTBALL', 'home_lost',        'Home Losses',         'team', 'home',       false, false, false, 23),
    ('FOOTBALL', 'home_scored',      'Home Goals Scored',   'team', 'home',       false, false, false, 24),
    ('FOOTBALL', 'home_conceded',    'Home Goals Conceded', 'team', 'home',       false, false, false, 25),
    ('FOOTBALL', 'home_points',      'Home Points',         'team', 'home',       false, false, false, 26),
    ('FOOTBALL', 'away_played',      'Away Matches',        'team', 'away',       false, false, false, 30),
    ('FOOTBALL', 'away_won',         'Away Wins',           'team', 'away',       false, false, false, 31),
    ('FOOTBALL', 'away_draw',        'Away Draws',          'team', 'away',       false, false, false, 32),
    ('FOOTBALL', 'away_lost',        'Away Losses',         'team', 'away',       false, false, false, 33),
    ('FOOTBALL', 'away_scored',      'Away Goals Scored',   'team', 'away',       false, false, false, 34),
    ('FOOTBALL', 'away_conceded',    'Away Goals Conceded', 'team', 'away',       false, false, false, 35),
    ('FOOTBALL', 'away_points',      'Away Points',         'team', 'away',       false, false, false, 36)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- ============================================================================
-- 4. PROVIDER SEASONS TABLE
-- ============================================================================

CREATE TABLE IF NOT EXISTS provider_seasons (
    id SERIAL PRIMARY KEY,
    league_id INTEGER NOT NULL REFERENCES leagues(id),
    season_year INTEGER NOT NULL,
    provider TEXT NOT NULL DEFAULT 'sportmonks',
    provider_season_id INTEGER NOT NULL,
    UNIQUE(league_id, season_year, provider)
);

CREATE INDEX IF NOT EXISTS idx_provider_seasons_lookup
    ON provider_seasons(league_id, season_year);

-- Seed known Premier League seasons (league_id=8, SportMonks ID)
INSERT INTO provider_seasons (league_id, season_year, provider, provider_season_id) VALUES
    (8, 2020, 'sportmonks', 17420),
    (8, 2021, 'sportmonks', 18378),
    (8, 2022, 'sportmonks', 19734),
    (8, 2023, 'sportmonks', 21646),
    (8, 2024, 'sportmonks', 23614),
    (8, 2025, 'sportmonks', 25583)
ON CONFLICT (league_id, season_year, provider) DO NOTHING;

-- Note: Other league season IDs are discovered at runtime via
-- `seed --discover-seasons --seasons 2023-2025`

-- Helper function
CREATE OR REPLACE FUNCTION resolve_provider_season_id(
    p_league_id INTEGER,
    p_season_year INTEGER,
    p_provider TEXT DEFAULT 'sportmonks'
)
RETURNS INTEGER AS $$
    SELECT provider_season_id
    FROM provider_seasons
    WHERE league_id = p_league_id
      AND season_year = p_season_year
      AND provider = p_provider;
$$ LANGUAGE sql STABLE;

-- ============================================================================
-- 5. PERCENTILE ARCHIVE TABLE
-- ============================================================================

CREATE TABLE IF NOT EXISTS percentile_archive (
    id SERIAL PRIMARY KEY,
    entity_type TEXT NOT NULL,
    entity_id INTEGER NOT NULL,
    sport TEXT NOT NULL,
    season INTEGER NOT NULL,
    stat_category TEXT NOT NULL,
    stat_value NUMERIC,
    percentile NUMERIC NOT NULL,
    rank INTEGER,
    sample_size INTEGER,
    comparison_group TEXT,
    calculated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    archived_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_final BOOLEAN NOT NULL DEFAULT false,
    UNIQUE(entity_type, entity_id, sport, season, stat_category, archived_at)
);

-- ============================================================================
-- 6. DERIVED STATS TRIGGERS
-- ============================================================================

-- NBA Player: per-36, true_shooting_pct, efficiency
CREATE OR REPLACE FUNCTION compute_nba_derived_stats()
RETURNS TRIGGER AS $$
DECLARE
    minutes  NUMERIC;
    pts      NUMERIC;
    reb      NUMERIC;
    ast      NUMERIC;
    stl      NUMERIC;
    blk      NUMERIC;
    fga      NUMERIC;
    fgm      NUMERIC;
    fta      NUMERIC;
    ftm      NUMERIC;
    turnover NUMERIC;
    tsa      NUMERIC;
BEGIN
    minutes  := (NEW.stats->>'minutes')::NUMERIC;
    pts      := (NEW.stats->>'pts')::NUMERIC;
    reb      := (NEW.stats->>'reb')::NUMERIC;
    ast      := (NEW.stats->>'ast')::NUMERIC;
    stl      := (NEW.stats->>'stl')::NUMERIC;
    blk      := (NEW.stats->>'blk')::NUMERIC;
    fga      := (NEW.stats->>'fga')::NUMERIC;
    fgm      := (NEW.stats->>'fgm')::NUMERIC;
    fta      := (NEW.stats->>'fta')::NUMERIC;
    ftm      := (NEW.stats->>'ftm')::NUMERIC;
    turnover := (NEW.stats->>'turnover')::NUMERIC;

    IF minutes IS NOT NULL AND minutes > 0 THEN
        IF pts IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object('pts_per_36', ROUND(pts / minutes * 36, 1));
        END IF;
        IF reb IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object('reb_per_36', ROUND(reb / minutes * 36, 1));
        END IF;
        IF ast IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object('ast_per_36', ROUND(ast / minutes * 36, 1));
        END IF;
        IF stl IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object('stl_per_36', ROUND(stl / minutes * 36, 1));
        END IF;
        IF blk IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object('blk_per_36', ROUND(blk / minutes * 36, 1));
        END IF;
    END IF;

    IF pts IS NOT NULL AND fga IS NOT NULL AND fta IS NOT NULL THEN
        tsa := fga + 0.44 * fta;
        IF tsa > 0 THEN
            NEW.stats := NEW.stats || jsonb_build_object('true_shooting_pct', ROUND(pts / (2 * tsa) * 100, 1));
        END IF;
    END IF;

    IF pts IS NOT NULL AND reb IS NOT NULL AND ast IS NOT NULL
       AND stl IS NOT NULL AND blk IS NOT NULL
       AND fga IS NOT NULL AND fgm IS NOT NULL
       AND fta IS NOT NULL AND ftm IS NOT NULL
       AND turnover IS NOT NULL THEN
        NEW.stats := NEW.stats || jsonb_build_object(
            'efficiency', ROUND((pts + reb + ast + stl + blk) - ((fga - fgm) + (fta - ftm) + turnover), 1));
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_nba_player_derived_stats ON player_stats;
CREATE TRIGGER trg_nba_player_derived_stats
    BEFORE INSERT OR UPDATE ON player_stats
    FOR EACH ROW
    WHEN (NEW.sport = 'NBA')
    EXECUTE FUNCTION compute_nba_derived_stats();

-- NBA Team: win_pct
CREATE OR REPLACE FUNCTION compute_nba_team_derived_stats()
RETURNS TRIGGER AS $$
DECLARE
    wins NUMERIC; losses NUMERIC; total NUMERIC;
BEGIN
    wins   := (NEW.stats->>'wins')::NUMERIC;
    losses := (NEW.stats->>'losses')::NUMERIC;
    IF wins IS NOT NULL AND losses IS NOT NULL THEN
        total := wins + losses;
        IF total > 0 THEN
            NEW.stats := NEW.stats || jsonb_build_object('win_pct', ROUND(wins / total, 3));
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_nba_team_derived_stats ON team_stats;
CREATE TRIGGER trg_nba_team_derived_stats
    BEFORE INSERT OR UPDATE ON team_stats
    FOR EACH ROW
    WHEN (NEW.sport = 'NBA')
    EXECUTE FUNCTION compute_nba_team_derived_stats();

-- NFL Player: td_int_ratio, catch_pct
CREATE OR REPLACE FUNCTION compute_nfl_derived_stats()
RETURNS TRIGGER AS $$
DECLARE
    pass_td NUMERIC; pass_int NUMERIC; rec NUMERIC; targets NUMERIC;
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

DROP TRIGGER IF EXISTS trg_nfl_player_derived_stats ON player_stats;
CREATE TRIGGER trg_nfl_player_derived_stats
    BEFORE INSERT OR UPDATE ON player_stats
    FOR EACH ROW
    WHEN (NEW.sport = 'NFL')
    EXECUTE FUNCTION compute_nfl_derived_stats();

-- NFL Team: win_pct (with ties)
CREATE OR REPLACE FUNCTION compute_nfl_team_derived_stats()
RETURNS TRIGGER AS $$
DECLARE
    wins NUMERIC; losses NUMERIC; ties NUMERIC; total NUMERIC;
BEGIN
    wins   := (NEW.stats->>'wins')::NUMERIC;
    losses := (NEW.stats->>'losses')::NUMERIC;
    ties   := COALESCE((NEW.stats->>'ties')::NUMERIC, 0);
    IF wins IS NOT NULL AND losses IS NOT NULL THEN
        total := wins + losses + ties;
        IF total > 0 THEN
            NEW.stats := NEW.stats || jsonb_build_object('win_pct', ROUND(wins / total, 3));
        END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_nfl_team_derived_stats ON team_stats;
CREATE TRIGGER trg_nfl_team_derived_stats
    BEFORE INSERT OR UPDATE ON team_stats
    FOR EACH ROW
    WHEN (NEW.sport = 'NFL')
    EXECUTE FUNCTION compute_nfl_team_derived_stats();

-- Football Player: per-90 metrics + accuracy rates
CREATE OR REPLACE FUNCTION compute_football_derived_stats()
RETURNS TRIGGER AS $$
DECLARE
    minutes NUMERIC; goals NUMERIC; assists NUMERIC; key_passes NUMERIC;
    shots_t NUMERIC; shots_on NUMERIC; passes_t NUMERIC; passes_a NUMERIC;
    tackles NUMERIC; intercepts NUMERIC; duels_t NUMERIC; duels_w NUMERIC;
    dribbles_a NUMERIC; dribbles_s NUMERIC; saves NUMERIC; conceded NUMERIC;
BEGIN
    minutes    := (NEW.stats->>'minutes_played')::NUMERIC;
    goals      := (NEW.stats->>'goals')::NUMERIC;
    assists    := (NEW.stats->>'assists')::NUMERIC;
    key_passes := (NEW.stats->>'key_passes')::NUMERIC;
    shots_t    := (NEW.stats->>'shots_total')::NUMERIC;
    shots_on   := (NEW.stats->>'shots_on_target')::NUMERIC;
    passes_t   := (NEW.stats->>'passes_total')::NUMERIC;
    passes_a   := (NEW.stats->>'passes_accurate')::NUMERIC;
    tackles    := (NEW.stats->>'tackles')::NUMERIC;
    intercepts := (NEW.stats->>'interceptions')::NUMERIC;
    duels_t    := (NEW.stats->>'duels_total')::NUMERIC;
    duels_w    := (NEW.stats->>'duels_won')::NUMERIC;
    dribbles_a := (NEW.stats->>'dribbles_attempts')::NUMERIC;
    dribbles_s := (NEW.stats->>'dribbles_success')::NUMERIC;
    saves      := (NEW.stats->>'saves')::NUMERIC;
    conceded   := (NEW.stats->>'goals_conceded')::NUMERIC;

    IF minutes IS NOT NULL AND minutes > 0 THEN
        IF goals IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object('goals_per_90', ROUND(goals * 90 / minutes, 3));
        END IF;
        IF assists IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object('assists_per_90', ROUND(assists * 90 / minutes, 3));
        END IF;
        IF key_passes IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object('key_passes_per_90', ROUND(key_passes * 90 / minutes, 3));
        END IF;
        IF shots_t IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object('shots_per_90', ROUND(shots_t * 90 / minutes, 3));
        END IF;
        IF tackles IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object('tackles_per_90', ROUND(tackles * 90 / minutes, 3));
        END IF;
        IF intercepts IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object('interceptions_per_90', ROUND(intercepts * 90 / minutes, 3));
        END IF;
        IF conceded IS NOT NULL THEN
            NEW.stats := NEW.stats || jsonb_build_object('goals_conceded_per_90', ROUND(conceded * 90 / minutes, 3));
        END IF;
    END IF;

    IF shots_t IS NOT NULL AND shots_t > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object('shot_accuracy', ROUND(COALESCE(shots_on, 0) / shots_t * 100, 1));
    END IF;
    IF passes_t IS NOT NULL AND passes_t > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object('pass_accuracy', ROUND(COALESCE(passes_a, 0) / passes_t * 100, 1));
    END IF;
    IF duels_t IS NOT NULL AND duels_t > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object('duel_success_rate', ROUND(COALESCE(duels_w, 0) / duels_t * 100, 1));
    END IF;
    IF dribbles_a IS NOT NULL AND dribbles_a > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object('dribble_success_rate', ROUND(COALESCE(dribbles_s, 0) / dribbles_a * 100, 1));
    END IF;
    IF saves IS NOT NULL AND conceded IS NOT NULL AND (saves + conceded) > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object('save_pct', ROUND(saves / (saves + conceded) * 100, 1));
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_football_derived_stats ON player_stats;
CREATE TRIGGER trg_football_derived_stats
    BEFORE INSERT OR UPDATE ON player_stats
    FOR EACH ROW
    WHEN (NEW.sport = 'FOOTBALL')
    EXECUTE FUNCTION compute_football_derived_stats();

-- ============================================================================
-- 7. PERCENTILE RECALCULATION FUNCTION
-- ============================================================================

CREATE OR REPLACE FUNCTION recalculate_percentiles(
    p_sport TEXT,
    p_season INTEGER,
    p_inverse_stats TEXT[] DEFAULT ARRAY[]::TEXT[]
)
RETURNS TABLE (players_updated INTEGER, teams_updated INTEGER) AS $$
DECLARE
    v_players INTEGER := 0;
    v_teams INTEGER := 0;
    v_inverse TEXT[];
BEGIN
    SELECT array_agg(DISTINCT key_name) INTO v_inverse
    FROM (
        SELECT key_name FROM stat_definitions
        WHERE sport = p_sport AND is_inverse = true
        UNION
        SELECT unnest(p_inverse_stats)
    ) combined;

    v_inverse := COALESCE(v_inverse, ARRAY[]::TEXT[]);

    -- PLAYER PERCENTILES (partitioned by position)
    WITH stat_keys AS (
        SELECT DISTINCT key
        FROM player_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season
          AND jsonb_typeof(val) = 'number'
          AND (val::text)::numeric != 0
    ),
    player_positions AS (
        SELECT ps.player_id, COALESCE(p.position, 'Unknown') AS position
        FROM player_stats ps
        JOIN players p ON p.id = ps.player_id AND p.sport = ps.sport
        WHERE ps.sport = p_sport AND ps.season = p_season
    ),
    expanded AS (
        SELECT ps.player_id, pp.position, sk.key AS stat_key,
               (ps.stats->>sk.key)::numeric AS stat_value
        FROM player_stats ps
        CROSS JOIN stat_keys sk
        JOIN player_positions pp ON pp.player_id = ps.player_id
        WHERE ps.sport = p_sport AND ps.season = p_season
          AND ps.stats ? sk.key
          AND (ps.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT player_id, position, stat_key,
            CASE
                WHEN stat_key = ANY(v_inverse) THEN
                    round((1.0 - percent_rank() OVER (
                        PARTITION BY position, stat_key ORDER BY stat_value ASC
                    ))::numeric * 100, 1)
                ELSE
                    round((percent_rank() OVER (
                        PARTITION BY position, stat_key ORDER BY stat_value ASC
                    ))::numeric * 100, 1)
            END AS percentile,
            count(*) OVER (PARTITION BY position, stat_key) AS sample_size
        FROM expanded
    ),
    aggregated AS (
        SELECT player_id, position, max(sample_size) AS max_sample_size,
            jsonb_object_agg(stat_key, percentile)
                || jsonb_build_object('_position_group', position, '_sample_size', max(sample_size))
            AS percentiles_json
        FROM ranked GROUP BY player_id, position
    )
    UPDATE player_stats ps
    SET percentiles = agg.percentiles_json, updated_at = NOW()
    FROM aggregated agg
    WHERE ps.player_id = agg.player_id
      AND ps.sport = p_sport AND ps.season = p_season;

    GET DIAGNOSTICS v_players = ROW_COUNT;

    -- TEAM PERCENTILES (all teams together)
    WITH stat_keys AS (
        SELECT DISTINCT key
        FROM team_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season
          AND jsonb_typeof(val) = 'number'
          AND (val::text)::numeric != 0
    ),
    expanded AS (
        SELECT ts.team_id, sk.key AS stat_key,
               (ts.stats->>sk.key)::numeric AS stat_value
        FROM team_stats ts
        CROSS JOIN stat_keys sk
        WHERE ts.sport = p_sport AND ts.season = p_season
          AND ts.stats ? sk.key
          AND (ts.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT team_id, stat_key,
            CASE
                WHEN stat_key = ANY(v_inverse) THEN
                    round((1.0 - percent_rank() OVER (
                        PARTITION BY stat_key ORDER BY stat_value ASC
                    ))::numeric * 100, 1)
                ELSE
                    round((percent_rank() OVER (
                        PARTITION BY stat_key ORDER BY stat_value ASC
                    ))::numeric * 100, 1)
            END AS percentile,
            count(*) OVER (PARTITION BY stat_key) AS sample_size
        FROM expanded
    ),
    aggregated AS (
        SELECT team_id,
            jsonb_object_agg(stat_key, percentile)
                || jsonb_build_object('_sample_size', max(sample_size))
            AS percentiles_json
        FROM ranked GROUP BY team_id
    )
    UPDATE team_stats ts
    SET percentiles = agg.percentiles_json, updated_at = NOW()
    FROM aggregated agg
    WHERE ts.team_id = agg.team_id
      AND ts.sport = p_sport AND ts.season = p_season;

    GET DIAGNOSTICS v_teams = ROW_COUNT;

    RETURN QUERY SELECT v_players, v_teams;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- 8. SCHEMA VERSION BUMP
-- ============================================================================

UPDATE meta SET value = '5.5', updated_at = NOW() WHERE key = 'schema_version';
