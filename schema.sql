-- Scoracle Data — Consolidated Schema v7.0
-- Updated: 2026-02-15
--
-- Single-file schema representing the complete database state.
-- Includes API response functions and materialized views.
--
-- Design principles:
--   - 4 unified tables (players, player_stats, teams, team_stats) shared by all sports
--   - Sport-specific data lives in JSONB (stats, meta) — no schema changes for new stats
--   - Postgres triggers compute derived stats (per-36, per-90, TS%, win_pct, etc.)
--   - Percentile calculation runs in Postgres via recalculate_percentiles()
--   - stat_definitions table is the canonical registry for all stat keys
--   - provider_seasons table maps external API season IDs to internal IDs
--   - API functions return complete JSON — Go is a pure transport layer
--   - mv_autofill_entities materialized view powers frontend autocomplete

-- ============================================================================
-- 1. CORE INFRASTRUCTURE
-- ============================================================================

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO meta (key, value) VALUES
    ('schema_version', '7.0'),
    ('last_full_sync', ''),
    ('last_incremental_sync', '')
ON CONFLICT (key) DO NOTHING;

CREATE TABLE IF NOT EXISTS sports (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    api_base_url TEXT DEFAULT NULL,
    current_season INTEGER NOT NULL,
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO sports (id, display_name, current_season) VALUES
    ('NBA', 'NBA Basketball', 2025),
    ('NFL', 'NFL Football', 2025),
    ('FOOTBALL', 'Football (Soccer)', 2025)
ON CONFLICT (id) DO NOTHING;

-- ============================================================================
-- 2. LEAGUES
-- ============================================================================
-- League metadata for multi-league sports (currently Football).
-- NBA/NFL have one league each — conference/division data lives in teams table.
-- League IDs use SportMonks IDs directly (8, 82, 301, 384, 564).

CREATE TABLE IF NOT EXISTS leagues (
    id INTEGER PRIMARY KEY,
    sport TEXT NOT NULL REFERENCES sports(id),
    name TEXT NOT NULL,
    country TEXT,
    logo_url TEXT,
    sportmonks_id INTEGER,
    is_benchmark BOOLEAN DEFAULT false,
    is_active BOOLEAN DEFAULT true,
    handicap DECIMAL,
    meta JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_leagues_sport ON leagues(sport);

INSERT INTO leagues (id, sport, name, country, sportmonks_id, is_benchmark) VALUES
    (8,   'FOOTBALL', 'Premier League', 'England', 8,   true),
    (82,  'FOOTBALL', 'Bundesliga',     'Germany', 82,  true),
    (301, 'FOOTBALL', 'Ligue 1',        'France',  301, true),
    (384, 'FOOTBALL', 'Serie A',        'Italy',   384, true),
    (564, 'FOOTBALL', 'La Liga',        'Spain',   564, true)
ON CONFLICT (id) DO NOTHING;

-- ============================================================================
-- 3. PLAYERS
-- ============================================================================
-- Profile/identity data shared across all sports.
-- Height and weight stored as TEXT (raw provider format, e.g., "6-6", "225").
-- Frontend handles display formatting/conversion.

CREATE TABLE IF NOT EXISTS players (
    id INTEGER NOT NULL,
    sport TEXT NOT NULL REFERENCES sports(id),
    name TEXT NOT NULL,
    first_name TEXT,
    last_name TEXT,
    position TEXT,
    detailed_position TEXT,
    nationality TEXT,
    date_of_birth DATE,
    height TEXT,
    weight TEXT,
    photo_url TEXT,
    team_id INTEGER,
    league_id INTEGER,
    meta JSONB DEFAULT '{}',
    raw_response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id, sport)
);

CREATE INDEX IF NOT EXISTS idx_players_sport ON players(sport);
CREATE INDEX IF NOT EXISTS idx_players_team ON players(team_id);
CREATE INDEX IF NOT EXISTS idx_players_name ON players(name);
CREATE INDEX IF NOT EXISTS idx_players_position ON players(sport, position);
CREATE INDEX IF NOT EXISTS idx_players_league ON players(league_id) WHERE league_id IS NOT NULL;

-- ============================================================================
-- 4. PLAYER STATS
-- ============================================================================
-- Per-season performance data. JSONB for sport-specific stats.
-- One row per player per sport per season per league.
-- NBA/NFL use league_id = 0 (single league).

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

-- ============================================================================
-- 5. TEAMS
-- ============================================================================

CREATE TABLE IF NOT EXISTS teams (
    id INTEGER NOT NULL,
    sport TEXT NOT NULL REFERENCES sports(id),
    name TEXT NOT NULL,
    short_code TEXT,
    country TEXT,
    city TEXT,
    logo_url TEXT,
    league_id INTEGER,
    founded INTEGER,
    venue_name TEXT,
    venue_capacity INTEGER,
    conference TEXT,
    division TEXT,
    meta JSONB DEFAULT '{}',
    raw_response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id, sport)
);

CREATE INDEX IF NOT EXISTS idx_teams_sport ON teams(sport);
CREATE INDEX IF NOT EXISTS idx_teams_name ON teams(name);
CREATE INDEX IF NOT EXISTS idx_teams_league ON teams(league_id) WHERE league_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_teams_conference ON teams(sport, conference) WHERE conference IS NOT NULL;

-- ============================================================================
-- 6. TEAM STATS
-- ============================================================================

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
CREATE INDEX IF NOT EXISTS idx_team_stats_wins
    ON team_stats (((stats->>'wins')::integer)) WHERE (stats->>'wins') IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_team_stats_points
    ON team_stats (((stats->>'points')::integer)) WHERE (stats->>'points') IS NOT NULL;

-- ============================================================================
-- 7. STAT DEFINITIONS — canonical stat registry
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

CREATE INDEX IF NOT EXISTS idx_stat_definitions_sport ON stat_definitions(sport, entity_type);

-- NBA player stats
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('NBA', 'games_played',       'Games Played',          'player', 'general',    false, false, false,  1),
    ('NBA', 'minutes',            'Minutes Per Game',      'player', 'general',    false, false, true,   2),
    ('NBA', 'pts',                'Points Per Game',       'player', 'scoring',    false, false, true,   3),
    ('NBA', 'reb',                'Rebounds Per Game',      'player', 'rebounding', false, false, true,   4),
    ('NBA', 'ast',                'Assists Per Game',       'player', 'passing',    false, false, true,   5),
    ('NBA', 'stl',                'Steals Per Game',        'player', 'defensive',  false, false, true,   6),
    ('NBA', 'blk',                'Blocks Per Game',        'player', 'defensive',  false, false, true,   7),
    ('NBA', 'oreb',               'Off Rebounds/Game',      'player', 'rebounding', false, false, false,  8),
    ('NBA', 'dreb',               'Def Rebounds/Game',      'player', 'rebounding', false, false, false,  9),
    ('NBA', 'turnover',           'Turnovers Per Game',     'player', 'general',    true,  false, true,  10),
    ('NBA', 'pf',                 'Fouls Per Game',         'player', 'general',    true,  false, false, 11),
    ('NBA', 'plus_minus',         'Plus/Minus',             'player', 'advanced',   false, false, true,  12),
    ('NBA', 'fg_pct',             'Field Goal %',           'player', 'shooting',   false, false, true,  20),
    ('NBA', 'fg3_pct',            'Three-Point %',          'player', 'shooting',   false, false, true,  21),
    ('NBA', 'ft_pct',             'Free Throw %',           'player', 'shooting',   false, false, true,  22),
    ('NBA', 'fgm',                'Field Goals Made',       'player', 'shooting',   false, false, false, 23),
    ('NBA', 'fga',                'Field Goals Attempted',  'player', 'shooting',   false, false, false, 24),
    ('NBA', 'fg3m',               'Three-Pointers Made',    'player', 'shooting',   false, false, false, 25),
    ('NBA', 'fg3a',               'Three-Pointers Att',     'player', 'shooting',   false, false, false, 26),
    ('NBA', 'ftm',                'Free Throws Made',       'player', 'shooting',   false, false, false, 27),
    ('NBA', 'fta',                'Free Throws Attempted',  'player', 'shooting',   false, false, false, 28),
    ('NBA', 'pts_per_36',         'Points Per 36 Min',      'player', 'advanced',   false, true,  true,  30),
    ('NBA', 'reb_per_36',         'Rebounds Per 36 Min',     'player', 'advanced',   false, true,  true,  31),
    ('NBA', 'ast_per_36',         'Assists Per 36 Min',      'player', 'advanced',   false, true,  true,  32),
    ('NBA', 'stl_per_36',         'Steals Per 36 Min',       'player', 'advanced',   false, true,  true,  33),
    ('NBA', 'blk_per_36',         'Blocks Per 36 Min',       'player', 'advanced',   false, true,  true,  34),
    ('NBA', 'true_shooting_pct',  'True Shooting %',        'player', 'advanced',   false, true,  true,  35),
    ('NBA', 'efficiency',         'Efficiency Rating',      'player', 'advanced',   false, true,  true,  36)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- NBA team stats
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('NBA', 'wins',          'Wins',             'team', 'standings',  false, false, true,   1),
    ('NBA', 'losses',        'Losses',           'team', 'standings',  true,  false, true,   2),
    ('NBA', 'games_played',  'Games Played',     'team', 'general',    false, false, false,  3),
    ('NBA', 'pts',           'Points Per Game',  'team', 'scoring',    false, false, true,   4),
    ('NBA', 'reb',           'Rebounds Per Game', 'team', 'rebounding', false, false, true,   5),
    ('NBA', 'ast',           'Assists Per Game',  'team', 'passing',    false, false, true,   6),
    ('NBA', 'fg_pct',        'Field Goal %',     'team', 'shooting',   false, false, true,   7),
    ('NBA', 'fg3_pct',       'Three-Point %',    'team', 'shooting',   false, false, true,   8),
    ('NBA', 'ft_pct',        'Free Throw %',     'team', 'shooting',   false, false, false,  9),
    ('NBA', 'win_pct',       'Win Percentage',   'team', 'standings',  false, true,  true,  10)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

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

-- Football player stats
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('FOOTBALL', 'appearances',           'Appearances',          'player', 'general',    false, false, false,  1),
    ('FOOTBALL', 'lineups',               'Starting Lineups',     'player', 'general',    false, false, false,  2),
    ('FOOTBALL', 'minutes_played',        'Minutes Played',       'player', 'general',    false, false, true,   3),
    ('FOOTBALL', 'goals',                 'Goals',                'player', 'scoring',    false, false, true,  10),
    ('FOOTBALL', 'assists',               'Assists',              'player', 'scoring',    false, false, true,  11),
    ('FOOTBALL', 'expected_goals',        'Expected Goals (xG)',  'player', 'scoring',    false, false, true,  12),
    ('FOOTBALL', 'goals_per_90',          'Goals Per 90',         'player', 'scoring',    false, true,  true,  13),
    ('FOOTBALL', 'assists_per_90',        'Assists Per 90',       'player', 'scoring',    false, true,  true,  14),
    ('FOOTBALL', 'shots_total',           'Total Shots',          'player', 'shooting',   false, false, false, 20),
    ('FOOTBALL', 'shots_on_target',       'Shots on Target',      'player', 'shooting',   false, false, false, 21),
    ('FOOTBALL', 'shots_per_90',          'Shots Per 90',         'player', 'shooting',   false, true,  true,  22),
    ('FOOTBALL', 'shot_accuracy',         'Shot Accuracy %',      'player', 'shooting',   false, true,  true,  23),
    ('FOOTBALL', 'passes_total',          'Total Passes',         'player', 'passing',    false, false, false, 30),
    ('FOOTBALL', 'passes_accurate',       'Accurate Passes',      'player', 'passing',    false, false, false, 31),
    ('FOOTBALL', 'key_passes',            'Key Passes',           'player', 'passing',    false, false, true,  32),
    ('FOOTBALL', 'crosses_total',         'Total Crosses',        'player', 'passing',    false, false, false, 33),
    ('FOOTBALL', 'crosses_accurate',      'Accurate Crosses',     'player', 'passing',    false, false, false, 34),
    ('FOOTBALL', 'key_passes_per_90',     'Key Passes Per 90',    'player', 'passing',    false, true,  true,  35),
    ('FOOTBALL', 'pass_accuracy',         'Pass Accuracy %',      'player', 'passing',    false, true,  true,  36),
    ('FOOTBALL', 'tackles',               'Tackles',              'player', 'defensive',  false, false, true,  40),
    ('FOOTBALL', 'interceptions',         'Interceptions',        'player', 'defensive',  false, false, true,  41),
    ('FOOTBALL', 'clearances',            'Clearances',           'player', 'defensive',  false, false, false, 42),
    ('FOOTBALL', 'blocks',                'Blocks',               'player', 'defensive',  false, false, false, 43),
    ('FOOTBALL', 'tackles_per_90',        'Tackles Per 90',       'player', 'defensive',  false, true,  true,  44),
    ('FOOTBALL', 'interceptions_per_90',  'Interceptions/90',     'player', 'defensive',  false, true,  true,  45),
    ('FOOTBALL', 'duels_total',           'Total Duels',          'player', 'duels',      false, false, false, 50),
    ('FOOTBALL', 'duels_won',             'Duels Won',            'player', 'duels',      false, false, false, 51),
    ('FOOTBALL', 'duel_success_rate',     'Duel Success Rate %',  'player', 'duels',      false, true,  true,  52),
    ('FOOTBALL', 'dribbles_attempts',     'Dribble Attempts',     'player', 'dribbling',  false, false, false, 55),
    ('FOOTBALL', 'dribbles_success',      'Successful Dribbles',  'player', 'dribbling',  false, false, false, 56),
    ('FOOTBALL', 'dribble_success_rate',  'Dribble Success %',    'player', 'dribbling',  false, true,  true,  57),
    ('FOOTBALL', 'yellow_cards',          'Yellow Cards',         'player', 'discipline', true,  false, true,  60),
    ('FOOTBALL', 'red_cards',             'Red Cards',            'player', 'discipline', true,  false, true,  61),
    ('FOOTBALL', 'fouls_committed',       'Fouls Committed',      'player', 'discipline', true,  false, false, 62),
    ('FOOTBALL', 'fouls_drawn',           'Fouls Drawn',          'player', 'discipline', false, false, false, 63),
    ('FOOTBALL', 'saves',                 'Saves',                'player', 'goalkeeper', false, false, true,  70),
    ('FOOTBALL', 'goals_conceded',        'Goals Conceded',       'player', 'goalkeeper', true,  false, true,  71),
    ('FOOTBALL', 'goals_conceded_per_90', 'Goals Conceded/90',    'player', 'goalkeeper', true,  true,  true,  72),
    ('FOOTBALL', 'save_pct',              'Save Percentage %',    'player', 'goalkeeper', false, true,  true,  73)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- Football team stats
INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('FOOTBALL', 'matches_played',  'Matches Played',       'team', 'standings', false, false, false,  1),
    ('FOOTBALL', 'wins',            'Wins',                 'team', 'standings', false, false, true,   2),
    ('FOOTBALL', 'draws',           'Draws',                'team', 'standings', false, false, false,  3),
    ('FOOTBALL', 'losses',          'Losses',               'team', 'standings', true,  false, true,   4),
    ('FOOTBALL', 'goals_for',       'Goals For',            'team', 'scoring',   false, false, true,   5),
    ('FOOTBALL', 'goals_against',   'Goals Against',        'team', 'scoring',   true,  false, true,   6),
    ('FOOTBALL', 'goal_difference', 'Goal Difference',      'team', 'scoring',   false, false, true,   7),
    ('FOOTBALL', 'points',          'Points',               'team', 'standings', false, false, true,   8),
    ('FOOTBALL', 'overall_points',  'Overall Points',       'team', 'standings', false, false, false,  9),
    ('FOOTBALL', 'position',        'League Position',      'team', 'standings', false, false, false, 10),
    ('FOOTBALL', 'home_played',     'Home Matches',         'team', 'home',      false, false, false, 20),
    ('FOOTBALL', 'home_won',        'Home Wins',            'team', 'home',      false, false, false, 21),
    ('FOOTBALL', 'home_draw',       'Home Draws',           'team', 'home',      false, false, false, 22),
    ('FOOTBALL', 'home_lost',       'Home Losses',          'team', 'home',      false, false, false, 23),
    ('FOOTBALL', 'home_scored',     'Home Goals Scored',    'team', 'home',      false, false, false, 24),
    ('FOOTBALL', 'home_conceded',   'Home Goals Conceded',  'team', 'home',      false, false, false, 25),
    ('FOOTBALL', 'home_points',     'Home Points',          'team', 'home',      false, false, false, 26),
    ('FOOTBALL', 'away_played',     'Away Matches',         'team', 'away',      false, false, false, 30),
    ('FOOTBALL', 'away_won',        'Away Wins',            'team', 'away',      false, false, false, 31),
    ('FOOTBALL', 'away_draw',       'Away Draws',           'team', 'away',      false, false, false, 32),
    ('FOOTBALL', 'away_lost',       'Away Losses',          'team', 'away',      false, false, false, 33),
    ('FOOTBALL', 'away_scored',     'Away Goals Scored',    'team', 'away',      false, false, false, 34),
    ('FOOTBALL', 'away_conceded',   'Away Goals Conceded',  'team', 'away',      false, false, false, 35),
    ('FOOTBALL', 'away_points',     'Away Points',          'team', 'away',      false, false, false, 36)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- ============================================================================
-- 8. FIXTURES & SCHEDULING
-- ============================================================================

CREATE TABLE IF NOT EXISTS fixtures (
    id SERIAL PRIMARY KEY,
    external_id INTEGER UNIQUE,
    sport TEXT NOT NULL REFERENCES sports(id),
    league_id INTEGER,
    season INTEGER NOT NULL,
    home_team_id INTEGER NOT NULL,
    away_team_id INTEGER NOT NULL,
    start_time TIMESTAMPTZ NOT NULL,
    venue_name TEXT,
    round TEXT,
    status TEXT NOT NULL DEFAULT 'scheduled'
        CHECK (status IN ('scheduled', 'in_progress', 'completed', 'seeded', 'cancelled', 'postponed')),
    seed_delay_hours INTEGER NOT NULL DEFAULT 4,
    seeded_at TIMESTAMPTZ,
    seed_attempts INTEGER DEFAULT 0,
    last_seed_error TEXT,
    home_score INTEGER,
    away_score INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT different_teams CHECK (home_team_id != away_team_id)
);

CREATE INDEX IF NOT EXISTS idx_fixtures_pending_seed
    ON fixtures(sport, status, start_time) WHERE status = 'scheduled' OR status = 'completed';
CREATE INDEX IF NOT EXISTS idx_fixtures_sport_date ON fixtures(sport, start_time);
CREATE INDEX IF NOT EXISTS idx_fixtures_league_date ON fixtures(league_id, start_time) WHERE league_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_fixtures_home_team ON fixtures(home_team_id);
CREATE INDEX IF NOT EXISTS idx_fixtures_away_team ON fixtures(away_team_id);
CREATE INDEX IF NOT EXISTS idx_fixtures_season ON fixtures(season);

-- ============================================================================
-- 9. PROVIDER SEASONS
-- ============================================================================
-- Maps internal (league_id, season_year) to external API season IDs.
-- Currently only SportMonks requires this (BDL uses plain year integers).

CREATE TABLE IF NOT EXISTS provider_seasons (
    id SERIAL PRIMARY KEY,
    league_id INTEGER NOT NULL REFERENCES leagues(id),
    season_year INTEGER NOT NULL,
    provider TEXT NOT NULL DEFAULT 'sportmonks',
    provider_season_id INTEGER NOT NULL,
    UNIQUE(league_id, season_year, provider)
);

CREATE INDEX IF NOT EXISTS idx_provider_seasons_lookup ON provider_seasons(league_id, season_year);

-- ============================================================================
-- 10. PERCENTILE ARCHIVE
-- ============================================================================

CREATE TABLE IF NOT EXISTS percentile_archive (
    id SERIAL PRIMARY KEY,
    entity_type TEXT NOT NULL,
    entity_id INTEGER NOT NULL,
    sport TEXT NOT NULL,
    season INTEGER NOT NULL,
    stat_category TEXT NOT NULL,
    stat_value REAL,
    percentile REAL,
    rank INTEGER,
    sample_size INTEGER,
    comparison_group TEXT,
    calculated_at TIMESTAMPTZ,
    archived_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    is_final BOOLEAN DEFAULT false,
    UNIQUE(entity_type, entity_id, sport, season, stat_category, archived_at)
);

CREATE INDEX IF NOT EXISTS idx_percentile_archive_sport_season ON percentile_archive(sport, season);
CREATE INDEX IF NOT EXISTS idx_percentile_archive_entity ON percentile_archive(entity_type, entity_id, sport);
CREATE INDEX IF NOT EXISTS idx_percentile_archive_final ON percentile_archive(sport, season, is_final) WHERE is_final = true;

-- ============================================================================
-- 11. VIEWS
-- ============================================================================

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
    p.height,
    p.weight,
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
-- 12. DERIVED STATS TRIGGERS
-- ============================================================================

-- NBA player: per-36, true shooting %, efficiency
CREATE OR REPLACE FUNCTION compute_nba_derived_stats()
RETURNS TRIGGER AS $$
DECLARE
    minutes NUMERIC; pts NUMERIC; reb NUMERIC; ast NUMERIC;
    stl NUMERIC; blk NUMERIC; fga NUMERIC; fgm NUMERIC;
    fta NUMERIC; ftm NUMERIC; turnover NUMERIC; tsa NUMERIC;
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
        IF pts IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('pts_per_36', ROUND(pts / minutes * 36, 1)); END IF;
        IF reb IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('reb_per_36', ROUND(reb / minutes * 36, 1)); END IF;
        IF ast IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('ast_per_36', ROUND(ast / minutes * 36, 1)); END IF;
        IF stl IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('stl_per_36', ROUND(stl / minutes * 36, 1)); END IF;
        IF blk IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('blk_per_36', ROUND(blk / minutes * 36, 1)); END IF;
    END IF;

    IF pts IS NOT NULL AND fga IS NOT NULL AND fta IS NOT NULL THEN
        tsa := fga + 0.44 * fta;
        IF tsa > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('true_shooting_pct', ROUND(pts / (2 * tsa) * 100, 1)); END IF;
    END IF;

    IF pts IS NOT NULL AND reb IS NOT NULL AND ast IS NOT NULL AND stl IS NOT NULL AND blk IS NOT NULL
       AND fga IS NOT NULL AND fgm IS NOT NULL AND fta IS NOT NULL AND ftm IS NOT NULL AND turnover IS NOT NULL THEN
        NEW.stats := NEW.stats || jsonb_build_object('efficiency', ROUND((pts + reb + ast + stl + blk) - ((fga - fgm) + (fta - ftm) + turnover), 1));
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- NBA team: win_pct
CREATE OR REPLACE FUNCTION compute_nba_team_derived_stats()
RETURNS TRIGGER AS $$
DECLARE wins NUMERIC; losses NUMERIC; total NUMERIC;
BEGIN
    wins := (NEW.stats->>'wins')::NUMERIC; losses := (NEW.stats->>'losses')::NUMERIC;
    IF wins IS NOT NULL AND losses IS NOT NULL THEN
        total := wins + losses;
        IF total > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('win_pct', ROUND(wins / total, 3)); END IF;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- NFL player: td_int_ratio, catch_pct
CREATE OR REPLACE FUNCTION compute_nfl_derived_stats()
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
CREATE OR REPLACE FUNCTION compute_nfl_team_derived_stats()
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

-- Football player: per-90 metrics, accuracy rates, GK stats
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
        IF goals IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('goals_per_90', ROUND(goals * 90 / minutes, 3)); END IF;
        IF assists IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('assists_per_90', ROUND(assists * 90 / minutes, 3)); END IF;
        IF key_passes IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('key_passes_per_90', ROUND(key_passes * 90 / minutes, 3)); END IF;
        IF shots_t IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('shots_per_90', ROUND(shots_t * 90 / minutes, 3)); END IF;
        IF tackles IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('tackles_per_90', ROUND(tackles * 90 / minutes, 3)); END IF;
        IF intercepts IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('interceptions_per_90', ROUND(intercepts * 90 / minutes, 3)); END IF;
        IF conceded IS NOT NULL THEN NEW.stats := NEW.stats || jsonb_build_object('goals_conceded_per_90', ROUND(conceded * 90 / minutes, 3)); END IF;
    END IF;

    IF shots_t IS NOT NULL AND shots_t > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('shot_accuracy', ROUND(COALESCE(shots_on, 0) / shots_t * 100, 1)); END IF;
    IF passes_t IS NOT NULL AND passes_t > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('pass_accuracy', ROUND(COALESCE(passes_a, 0) / passes_t * 100, 1)); END IF;
    IF duels_t IS NOT NULL AND duels_t > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('duel_success_rate', ROUND(COALESCE(duels_w, 0) / duels_t * 100, 1)); END IF;
    IF dribbles_a IS NOT NULL AND dribbles_a > 0 THEN NEW.stats := NEW.stats || jsonb_build_object('dribble_success_rate', ROUND(COALESCE(dribbles_s, 0) / dribbles_a * 100, 1)); END IF;
    IF saves IS NOT NULL AND conceded IS NOT NULL AND (saves + conceded) > 0 THEN
        NEW.stats := NEW.stats || jsonb_build_object('save_pct', ROUND(saves / (saves + conceded) * 100, 1));
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create triggers (DROP first to ensure clean state)
DROP TRIGGER IF EXISTS trg_nba_player_derived_stats ON player_stats;
DROP TRIGGER IF EXISTS trg_nba_team_derived_stats ON team_stats;
DROP TRIGGER IF EXISTS trg_nfl_player_derived_stats ON player_stats;
DROP TRIGGER IF EXISTS trg_nfl_team_derived_stats ON team_stats;
DROP TRIGGER IF EXISTS trg_football_derived_stats ON player_stats;

CREATE TRIGGER trg_nba_player_derived_stats BEFORE INSERT OR UPDATE ON player_stats FOR EACH ROW WHEN (NEW.sport = 'NBA') EXECUTE FUNCTION compute_nba_derived_stats();
CREATE TRIGGER trg_nba_team_derived_stats BEFORE INSERT OR UPDATE ON team_stats FOR EACH ROW WHEN (NEW.sport = 'NBA') EXECUTE FUNCTION compute_nba_team_derived_stats();
CREATE TRIGGER trg_nfl_player_derived_stats BEFORE INSERT OR UPDATE ON player_stats FOR EACH ROW WHEN (NEW.sport = 'NFL') EXECUTE FUNCTION compute_nfl_derived_stats();
CREATE TRIGGER trg_nfl_team_derived_stats BEFORE INSERT OR UPDATE ON team_stats FOR EACH ROW WHEN (NEW.sport = 'NFL') EXECUTE FUNCTION compute_nfl_team_derived_stats();
CREATE TRIGGER trg_football_derived_stats BEFORE INSERT OR UPDATE ON player_stats FOR EACH ROW WHEN (NEW.sport = 'FOOTBALL') EXECUTE FUNCTION compute_football_derived_stats();

-- ============================================================================
-- 13. HELPER FUNCTIONS
-- ============================================================================

CREATE OR REPLACE FUNCTION get_pending_fixtures(
    p_sport TEXT DEFAULT NULL,
    p_limit INTEGER DEFAULT 50,
    p_max_retries INTEGER DEFAULT 3
)
RETURNS TABLE (
    id INTEGER, sport TEXT, league_id INTEGER, season INTEGER,
    home_team_id INTEGER, away_team_id INTEGER, start_time TIMESTAMPTZ,
    seed_delay_hours INTEGER, seed_attempts INTEGER, external_id INTEGER
) AS $$
    SELECT f.id, f.sport, f.league_id, f.season,
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

CREATE OR REPLACE FUNCTION mark_fixture_seeded(
    p_fixture_id INTEGER,
    p_home_score INTEGER DEFAULT NULL,
    p_away_score INTEGER DEFAULT NULL
)
RETURNS VOID AS $$
BEGIN
    UPDATE fixtures SET
        status = 'seeded', seeded_at = NOW(),
        home_score = COALESCE(p_home_score, home_score),
        away_score = COALESCE(p_away_score, away_score),
        updated_at = NOW()
    WHERE id = p_fixture_id;
END;
$$ LANGUAGE plpgsql;

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

CREATE OR REPLACE FUNCTION fn_stat_leaders(
    p_sport TEXT, p_season INTEGER, p_stat_name TEXT,
    p_limit INTEGER DEFAULT 25, p_position TEXT DEFAULT NULL,
    p_league_id INTEGER DEFAULT 0
)
RETURNS TABLE (rank BIGINT, player_id INTEGER, name TEXT, position TEXT, team_name TEXT, stat_value NUMERIC) AS $$
    SELECT
        ROW_NUMBER() OVER (ORDER BY stat.val DESC) AS rank,
        p.id AS player_id, p.name, p.position, t.name AS team_name, stat.val AS stat_value
    FROM player_stats s
    CROSS JOIN LATERAL (SELECT (s.stats->>p_stat_name)::NUMERIC AS val) stat
    JOIN players p ON s.player_id = p.id AND s.sport = p.sport
    LEFT JOIN teams t ON s.team_id = t.id AND s.sport = t.sport
    WHERE s.sport = p_sport AND s.season = p_season AND s.league_id = p_league_id
      AND stat.val IS NOT NULL
      AND (p_position IS NULL OR p.position = p_position)
    ORDER BY stat.val DESC
    LIMIT p_limit;
$$ LANGUAGE sql STABLE;

CREATE OR REPLACE FUNCTION fn_standings(
    p_sport TEXT, p_season INTEGER,
    p_league_id INTEGER DEFAULT 0, p_conference TEXT DEFAULT NULL
)
RETURNS TABLE (
    rank BIGINT, id INTEGER, name TEXT, logo_url TEXT,
    conference TEXT, division TEXT, league_name TEXT, stats JSONB, win_pct NUMERIC
) AS $$
    SELECT
        ROW_NUMBER() OVER (
            ORDER BY
                CASE WHEN p_sport = 'FOOTBALL' THEN (s.stats->>'points')::INTEGER END DESC NULLS LAST,
                CASE WHEN p_sport = 'FOOTBALL' THEN (s.stats->>'goal_difference')::INTEGER END DESC NULLS LAST,
                CASE WHEN p_sport = 'FOOTBALL' THEN (s.stats->>'goals_for')::INTEGER END DESC NULLS LAST,
                CASE WHEN p_sport != 'FOOTBALL' THEN (s.stats->>'wins')::INTEGER END DESC NULLS LAST
        ) AS rank,
        t.id, t.name, t.logo_url, t.conference, t.division, l.name AS league_name, s.stats,
        CASE WHEN p_sport != 'FOOTBALL' THEN ROUND(
            (s.stats->>'wins')::NUMERIC /
            NULLIF((s.stats->>'wins')::INTEGER + (s.stats->>'losses')::INTEGER, 0), 3
        ) END AS win_pct
    FROM team_stats s
    JOIN teams t ON s.team_id = t.id AND s.sport = t.sport
    LEFT JOIN leagues l ON s.league_id = l.id
    WHERE s.sport = p_sport AND s.season = p_season AND s.league_id = p_league_id
      AND (p_conference IS NULL OR t.conference = p_conference)
    ORDER BY rank;
$$ LANGUAGE sql STABLE;

-- ============================================================================
-- 14. PERCENTILE CALCULATION
-- ============================================================================
-- Reads inverse stats from stat_definitions table.
-- p_inverse_stats parameter kept for backward compatibility (merged with DB list).

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
        SELECT key_name FROM stat_definitions WHERE sport = p_sport AND is_inverse = true
        UNION
        SELECT unnest(p_inverse_stats)
    ) combined;
    v_inverse := COALESCE(v_inverse, ARRAY[]::TEXT[]);

    -- Player percentiles (partitioned by position)
    WITH stat_keys AS (
        SELECT DISTINCT key FROM player_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    player_positions AS (
        SELECT ps.player_id, COALESCE(p.position, 'Unknown') AS position
        FROM player_stats ps JOIN players p ON p.id = ps.player_id AND p.sport = ps.sport
        WHERE ps.sport = p_sport AND ps.season = p_season
    ),
    expanded AS (
        SELECT ps.player_id, pp.position, sk.key AS stat_key, (ps.stats->>sk.key)::numeric AS stat_value
        FROM player_stats ps CROSS JOIN stat_keys sk JOIN player_positions pp ON pp.player_id = ps.player_id
        WHERE ps.sport = p_sport AND ps.season = p_season AND ps.stats ? sk.key AND (ps.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT player_id, position, stat_key,
            CASE WHEN stat_key = ANY(v_inverse)
                THEN round((1.0 - percent_rank() OVER (PARTITION BY position, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
                ELSE round((percent_rank() OVER (PARTITION BY position, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
            END AS percentile,
            count(*) OVER (PARTITION BY position, stat_key) AS sample_size
        FROM expanded
    ),
    aggregated AS (
        SELECT player_id, position, max(sample_size) AS max_sample_size,
            jsonb_object_agg(stat_key, percentile) || jsonb_build_object('_position_group', position, '_sample_size', max(sample_size)) AS percentiles_json
        FROM ranked GROUP BY player_id, position
    )
    UPDATE player_stats ps SET percentiles = agg.percentiles_json, updated_at = NOW()
    FROM aggregated agg WHERE ps.player_id = agg.player_id AND ps.sport = p_sport AND ps.season = p_season;
    GET DIAGNOSTICS v_players = ROW_COUNT;

    -- Team percentiles (no position partitioning)
    WITH stat_keys AS (
        SELECT DISTINCT key FROM team_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    expanded AS (
        SELECT ts.team_id, sk.key AS stat_key, (ts.stats->>sk.key)::numeric AS stat_value
        FROM team_stats ts CROSS JOIN stat_keys sk
        WHERE ts.sport = p_sport AND ts.season = p_season AND ts.stats ? sk.key AND (ts.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT team_id, stat_key,
            CASE WHEN stat_key = ANY(v_inverse)
                THEN round((1.0 - percent_rank() OVER (PARTITION BY stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
                ELSE round((percent_rank() OVER (PARTITION BY stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
            END AS percentile,
            count(*) OVER (PARTITION BY stat_key) AS sample_size
        FROM expanded
    ),
    aggregated AS (
        SELECT team_id, jsonb_object_agg(stat_key, percentile) || jsonb_build_object('_sample_size', max(sample_size)) AS percentiles_json
        FROM ranked GROUP BY team_id
    )
    UPDATE team_stats ts SET percentiles = agg.percentiles_json, updated_at = NOW()
    FROM aggregated agg WHERE ts.team_id = agg.team_id AND ts.sport = p_sport AND ts.season = p_season;
    GET DIAGNOSTICS v_teams = ROW_COUNT;

    RETURN QUERY SELECT v_players, v_teams;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- 8. API RESPONSE FUNCTIONS
-- ============================================================================
-- These functions return complete JSON responses, making Go a pure transport
-- layer. No struct scanning, no marshaling — Postgres serializes, Go passes
-- bytes through.

-- Player Profile — complete JSON response
CREATE OR REPLACE FUNCTION api_player_profile(p_id INT, p_sport TEXT)
RETURNS JSON AS $$
    SELECT row_to_json(v)
    FROM v_player_profile v
    WHERE v.id = p_id AND v.sport_id = p_sport;
$$ LANGUAGE sql STABLE;

-- Team Profile — complete JSON response
CREATE OR REPLACE FUNCTION api_team_profile(p_id INT, p_sport TEXT)
RETURNS JSON AS $$
    SELECT row_to_json(v)
    FROM v_team_profile v
    WHERE v.id = p_id AND v.sport_id = p_sport;
$$ LANGUAGE sql STABLE;

-- Entity Stats — complete JSON response with percentile metadata extracted
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

-- Available Seasons — complete JSON response
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
-- 9. MATERIALIZED VIEWS
-- ============================================================================

-- Bootstrap — Pre-computed entity list for frontend autocomplete.
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

CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_autofill_pk
    ON mv_autofill_entities (id, type, sport);

CREATE INDEX IF NOT EXISTS idx_mv_autofill_sport
    ON mv_autofill_entities (sport);

-- ============================================================================
-- 15. NOTIFICATIONS & FOLLOWS
-- ============================================================================
-- Users, follows, device tokens, and scheduled notification queue.
-- Supports push notifications triggered by percentile threshold crossings.

CREATE TABLE IF NOT EXISTS users (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    timezone    TEXT NOT NULL DEFAULT 'UTC',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS user_follows (
    id          SERIAL PRIMARY KEY,
    user_id     UUID NOT NULL REFERENCES users(id),
    entity_type TEXT NOT NULL CHECK (entity_type IN ('player', 'team')),
    entity_id   INTEGER NOT NULL,
    sport       TEXT NOT NULL REFERENCES sports(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, entity_type, entity_id, sport)
);

CREATE INDEX IF NOT EXISTS idx_user_follows_entity
    ON user_follows(entity_type, entity_id, sport);
CREATE INDEX IF NOT EXISTS idx_user_follows_user
    ON user_follows(user_id);

CREATE TABLE IF NOT EXISTS user_devices (
    id          SERIAL PRIMARY KEY,
    user_id     UUID NOT NULL REFERENCES users(id),
    platform    TEXT NOT NULL CHECK (platform IN ('ios', 'android', 'web')),
    token       TEXT NOT NULL,
    is_active   BOOLEAN DEFAULT true,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, token)
);

CREATE TABLE IF NOT EXISTS notifications (
    id              SERIAL PRIMARY KEY,
    user_id         UUID NOT NULL REFERENCES users(id),
    entity_type     TEXT NOT NULL,
    entity_id       INTEGER NOT NULL,
    sport           TEXT NOT NULL REFERENCES sports(id),
    fixture_id      INTEGER REFERENCES fixtures(id),
    stat_key        TEXT NOT NULL,
    percentile      NUMERIC NOT NULL,
    message         TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'scheduled'
        CHECK (status IN ('scheduled', 'sending', 'sent', 'failed')),
    scheduled_for   TIMESTAMPTZ NOT NULL,
    sent_at         TIMESTAMPTZ,
    last_error      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_notifications_dispatch
    ON notifications(status, scheduled_for) WHERE status = 'scheduled';
CREATE INDEX IF NOT EXISTS idx_notifications_user
    ON notifications(user_id, created_at DESC);

-- ============================================================================
-- 16. NOTIFICATION HELPER FUNCTIONS
-- ============================================================================

-- Archive current percentiles before recalculation (for notification diffing).
CREATE OR REPLACE FUNCTION archive_current_percentiles(p_sport TEXT, p_season INTEGER)
RETURNS VOID AS $$
BEGIN
    -- Archive player percentiles
    INSERT INTO percentile_archive (entity_type, entity_id, sport, season, stat_category, percentile, sample_size, calculated_at)
    SELECT 'player', ps.player_id, ps.sport, ps.season, kv.key,
           (kv.value::text)::real,
           COALESCE((ps.percentiles->>'_sample_size')::integer, 0),
           ps.updated_at
    FROM player_stats ps
    CROSS JOIN LATERAL jsonb_each(ps.percentiles) AS kv(key, value)
    WHERE ps.sport = p_sport AND ps.season = p_season
      AND kv.key NOT LIKE '\_%'
      AND jsonb_typeof(kv.value) = 'number'
    ON CONFLICT (entity_type, entity_id, sport, season, stat_category, archived_at) DO NOTHING;

    -- Archive team percentiles
    INSERT INTO percentile_archive (entity_type, entity_id, sport, season, stat_category, percentile, sample_size, calculated_at)
    SELECT 'team', ts.team_id, ts.sport, ts.season, kv.key,
           (kv.value::text)::real,
           COALESCE((ts.percentiles->>'_sample_size')::integer, 0),
           ts.updated_at
    FROM team_stats ts
    CROSS JOIN LATERAL jsonb_each(ts.percentiles) AS kv(key, value)
    WHERE ts.sport = p_sport AND ts.season = p_season
      AND kv.key NOT LIKE '\_%'
      AND jsonb_typeof(kv.value) = 'number'
    ON CONFLICT (entity_type, entity_id, sport, season, stat_category, archived_at) DO NOTHING;
END;
$$ LANGUAGE plpgsql;

-- Detect percentile movements for entities involved in a fixture.
-- Compares percentile_archive (pre-recalc snapshot) vs current values.
CREATE OR REPLACE FUNCTION detect_percentile_changes(p_fixture_id INTEGER)
RETURNS TABLE (
    entity_type TEXT, entity_id INTEGER, sport TEXT, season INTEGER,
    league_id INTEGER, stat_key TEXT, old_percentile REAL, new_percentile REAL,
    sample_size INTEGER
) AS $$
    -- Team changes
    SELECT 'team'::text, ts.team_id, ts.sport, ts.season, ts.league_id,
           kv.key, pa.percentile, (kv.value::text)::real,
           COALESCE((ts.percentiles->>'_sample_size')::integer, 0)
    FROM fixtures f
    JOIN team_stats ts ON ts.sport = f.sport AND ts.season = f.season
        AND ts.team_id IN (f.home_team_id, f.away_team_id)
    CROSS JOIN LATERAL jsonb_each(ts.percentiles) AS kv(key, value)
    LEFT JOIN LATERAL (
        SELECT pa2.percentile FROM percentile_archive pa2
        WHERE pa2.entity_type = 'team'
          AND pa2.entity_id = ts.team_id AND pa2.sport = ts.sport
          AND pa2.season = ts.season AND pa2.stat_category = kv.key
          AND pa2.is_final = false
        ORDER BY pa2.archived_at DESC LIMIT 1
    ) pa ON true
    WHERE f.id = p_fixture_id
      AND kv.key NOT LIKE '\_%'
      AND jsonb_typeof(kv.value) = 'number'

    UNION ALL

    -- Player changes (players on fixture teams)
    SELECT 'player'::text, ps.player_id, ps.sport, ps.season, ps.league_id,
           kv.key, pa.percentile, (kv.value::text)::real,
           COALESCE((ps.percentiles->>'_sample_size')::integer, 0)
    FROM fixtures f
    JOIN player_stats ps ON ps.sport = f.sport AND ps.season = f.season
        AND ps.team_id IN (f.home_team_id, f.away_team_id)
    CROSS JOIN LATERAL jsonb_each(ps.percentiles) AS kv(key, value)
    LEFT JOIN LATERAL (
        SELECT pa2.percentile FROM percentile_archive pa2
        WHERE pa2.entity_type = 'player'
          AND pa2.entity_id = ps.player_id AND pa2.sport = ps.sport
          AND pa2.season = ps.season AND pa2.stat_category = kv.key
          AND pa2.is_final = false
        ORDER BY pa2.archived_at DESC LIMIT 1
    ) pa ON true
    WHERE f.id = p_fixture_id
      AND kv.key NOT LIKE '\_%'
      AND jsonb_typeof(kv.value) = 'number';
$$ LANGUAGE sql STABLE;
