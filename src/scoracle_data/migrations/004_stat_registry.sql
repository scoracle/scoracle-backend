-- Scoracle Data — Stat Registry v5.3
-- Created: 2026-02-10
-- Purpose: Canonical stat name registry. Single source of truth for what
--          JSONB stat keys exist, how they should be displayed, whether they
--          are derived (computed by trigger), and whether they are inverse
--          (lower is better for percentile calculation).
--
--          Both Python (seeders, percentile calculator) and future Go (API)
--          read from this table. No more string conventions scattered across
--          Python config files and SQL functions.
--
-- Changes:
--   1. Create stat_definitions table
--   2. Seed all stat definitions for NBA, NFL, FOOTBALL
--   3. Schema version bump to 5.3

-- ============================================================================
-- 1. STAT DEFINITIONS TABLE
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

-- ============================================================================
-- 2. NBA PLAYER STATS
-- ============================================================================
-- Raw stats written by seed_nba.py from BallDontLie season averages.
-- BDL returns per-game averages, so pts = points per game, etc.

INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    -- Core counting stats (per-game averages from API)
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
    -- Shooting splits
    ('NBA', 'fg_pct',        'Field Goal %',        'player', 'shooting',  false, false, true,  20),
    ('NBA', 'fg3_pct',       'Three-Point %',       'player', 'shooting',  false, false, true,  21),
    ('NBA', 'ft_pct',        'Free Throw %',        'player', 'shooting',  false, false, true,  22),
    ('NBA', 'fgm',           'Field Goals Made',    'player', 'shooting',  false, false, false, 23),
    ('NBA', 'fga',           'Field Goals Attempted','player', 'shooting',  false, false, false, 24),
    ('NBA', 'fg3m',          'Three-Pointers Made', 'player', 'shooting',  false, false, false, 25),
    ('NBA', 'fg3a',          'Three-Pointers Att',  'player', 'shooting',  false, false, false, 26),
    ('NBA', 'ftm',           'Free Throws Made',    'player', 'shooting',  false, false, false, 27),
    ('NBA', 'fta',           'Free Throws Attempted','player', 'shooting', false, false, false, 28),
    -- Derived stats (computed by Postgres trigger)
    ('NBA', 'pts_per_36',    'Points Per 36 Min',   'player', 'advanced',  false, true,  true,  30),
    ('NBA', 'reb_per_36',    'Rebounds Per 36 Min',  'player', 'advanced',  false, true,  true,  31),
    ('NBA', 'ast_per_36',    'Assists Per 36 Min',   'player', 'advanced',  false, true,  true,  32),
    ('NBA', 'stl_per_36',    'Steals Per 36 Min',    'player', 'advanced',  false, true,  true,  33),
    ('NBA', 'blk_per_36',    'Blocks Per 36 Min',    'player', 'advanced',  false, true,  true,  34),
    ('NBA', 'true_shooting_pct', 'True Shooting %',  'player', 'advanced',  false, true,  true,  35),
    ('NBA', 'efficiency',    'Efficiency Rating',    'player', 'advanced',  false, true,  true,  36)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- ============================================================================
-- 2b. NBA TEAM STATS
-- ============================================================================

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
    -- Derived
    ('NBA', 'win_pct',       'Win Percentage',      'team', 'standings',  false, true,  true,  10)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- ============================================================================
-- 3. NFL PLAYER STATS
-- ============================================================================
-- Raw stats written by seed_nfl.py from BallDontLie.
-- Some per-game stats are already provided by the API.

INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('NFL', 'games_played',            'Games Played',          'player', 'general',    false, false, false,  1),
    -- Passing
    ('NFL', 'passing_completions',     'Completions',           'player', 'passing',    false, false, false, 10),
    ('NFL', 'passing_attempts',        'Pass Attempts',         'player', 'passing',    false, false, false, 11),
    ('NFL', 'passing_yards',           'Passing Yards',         'player', 'passing',    false, false, true,  12),
    ('NFL', 'passing_touchdowns',      'Passing TDs',           'player', 'passing',    false, false, true,  13),
    ('NFL', 'passing_interceptions',   'Interceptions Thrown',  'player', 'passing',    true,  false, true,  14),
    ('NFL', 'passing_yards_per_game',  'Pass Yards/Game',       'player', 'passing',    false, false, true,  15),
    ('NFL', 'passing_completion_pct',  'Completion %',          'player', 'passing',    false, false, true,  16),
    ('NFL', 'qbr',                     'Passer Rating',         'player', 'passing',    false, false, true,  17),
    -- Rushing
    ('NFL', 'rushing_attempts',        'Rush Attempts',         'player', 'rushing',    false, false, false, 20),
    ('NFL', 'rushing_yards',           'Rushing Yards',         'player', 'rushing',    false, false, true,  21),
    ('NFL', 'rushing_touchdowns',      'Rushing TDs',           'player', 'rushing',    false, false, true,  22),
    ('NFL', 'rushing_yards_per_game',  'Rush Yards/Game',       'player', 'rushing',    false, false, true,  23),
    ('NFL', 'yards_per_rush_attempt',  'Yards/Carry',           'player', 'rushing',    false, false, true,  24),
    ('NFL', 'rushing_first_downs',     'Rushing First Downs',   'player', 'rushing',    false, false, false, 25),
    -- Receiving
    ('NFL', 'receptions',              'Receptions',            'player', 'receiving',  false, false, true,  30),
    ('NFL', 'receiving_yards',         'Receiving Yards',       'player', 'receiving',  false, false, true,  31),
    ('NFL', 'receiving_touchdowns',    'Receiving TDs',         'player', 'receiving',  false, false, true,  32),
    ('NFL', 'receiving_targets',       'Targets',               'player', 'receiving',  false, false, false, 33),
    ('NFL', 'receiving_yards_per_game','Receiving Yards/Game',  'player', 'receiving',  false, false, true,  34),
    ('NFL', 'yards_per_reception',     'Yards/Reception',       'player', 'receiving',  false, false, true,  35),
    ('NFL', 'receiving_first_downs',   'Receiving First Downs', 'player', 'receiving',  false, false, false, 36),
    -- Defense
    ('NFL', 'total_tackles',           'Total Tackles',         'player', 'defensive',  false, false, true,  40),
    ('NFL', 'solo_tackles',            'Solo Tackles',          'player', 'defensive',  false, false, false, 41),
    ('NFL', 'assist_tackles',          'Assisted Tackles',      'player', 'defensive',  false, false, false, 42),
    ('NFL', 'defensive_sacks',         'Sacks',                 'player', 'defensive',  false, false, true,  43),
    ('NFL', 'defensive_sack_yards',    'Sack Yards',            'player', 'defensive',  false, false, false, 44),
    ('NFL', 'defensive_interceptions', 'Interceptions',         'player', 'defensive',  false, false, true,  45),
    ('NFL', 'interception_touchdowns', 'INT Return TDs',        'player', 'defensive',  false, false, false, 46),
    ('NFL', 'fumbles_forced',          'Forced Fumbles',        'player', 'defensive',  false, false, true,  47),
    ('NFL', 'fumbles_recovered',       'Fumbles Recovered',     'player', 'defensive',  false, false, false, 48),
    -- Kicking
    ('NFL', 'field_goal_attempts',     'FG Attempts',           'player', 'kicking',    false, false, false, 50),
    ('NFL', 'field_goals_made',        'FG Made',               'player', 'kicking',    false, false, false, 51),
    ('NFL', 'field_goal_pct',          'FG Percentage',         'player', 'kicking',    false, false, true,  52),
    -- Punting & Returns
    ('NFL', 'punts',                   'Punts',                 'player', 'special',    false, false, false, 60),
    ('NFL', 'punt_yards',              'Punt Yards',            'player', 'special',    false, false, false, 61),
    ('NFL', 'kick_returns',            'Kick Returns',          'player', 'special',    false, false, false, 62),
    ('NFL', 'kick_return_yards',       'Kick Return Yards',     'player', 'special',    false, false, false, 63),
    ('NFL', 'kick_return_touchdowns',  'Kick Return TDs',       'player', 'special',    false, false, false, 64),
    ('NFL', 'punt_returner_returns',   'Punt Returns',          'player', 'special',    false, false, false, 65),
    ('NFL', 'punt_returner_return_yards','Punt Return Yards',   'player', 'special',    false, false, false, 66),
    ('NFL', 'punt_return_touchdowns',  'Punt Return TDs',       'player', 'special',    false, false, false, 67),
    -- Derived
    ('NFL', 'td_int_ratio',            'TD/INT Ratio',          'player', 'passing',    false, true,  true,  18),
    ('NFL', 'catch_pct',               'Catch %',               'player', 'receiving',  false, true,  true,  37)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- ============================================================================
-- 3b. NFL TEAM STATS
-- ============================================================================

INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    ('NFL', 'wins',              'Wins',               'team', 'standings',  false, false, true,   1),
    ('NFL', 'losses',            'Losses',             'team', 'standings',  true,  false, true,   2),
    ('NFL', 'ties',              'Ties',               'team', 'standings',  false, false, false,  3),
    ('NFL', 'points_for',       'Points For',          'team', 'scoring',   false, false, true,   4),
    ('NFL', 'points_against',   'Points Against',      'team', 'scoring',   true,  false, true,   5),
    ('NFL', 'point_differential','Point Differential',  'team', 'scoring',   false, false, true,   6),
    -- Derived
    ('NFL', 'win_pct',           'Win Percentage',     'team', 'standings',  false, true,  true,   7)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- ============================================================================
-- 4. FOOTBALL PLAYER STATS
-- ============================================================================
-- Raw stats written by seed_football.py from SportMonks.
-- Derived stats computed by compute_football_derived_stats() trigger.

INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    -- Appearances
    ('FOOTBALL', 'appearances',      'Appearances',         'player', 'general',    false, false, false,  1),
    ('FOOTBALL', 'lineups',          'Starting Lineups',    'player', 'general',    false, false, false,  2),
    ('FOOTBALL', 'minutes_played',   'Minutes Played',      'player', 'general',    false, false, true,   3),
    -- Goals & Assists
    ('FOOTBALL', 'goals',            'Goals',               'player', 'scoring',    false, false, true,   10),
    ('FOOTBALL', 'assists',          'Assists',             'player', 'scoring',    false, false, true,   11),
    ('FOOTBALL', 'expected_goals',   'Expected Goals (xG)', 'player', 'scoring',    false, false, true,   12),
    -- Shooting
    ('FOOTBALL', 'shots_total',      'Total Shots',         'player', 'shooting',   false, false, false,  20),
    ('FOOTBALL', 'shots_on_target',  'Shots on Target',     'player', 'shooting',   false, false, false,  21),
    -- Passing
    ('FOOTBALL', 'passes_total',     'Total Passes',        'player', 'passing',    false, false, false,  30),
    ('FOOTBALL', 'passes_accurate',  'Accurate Passes',     'player', 'passing',    false, false, false,  31),
    ('FOOTBALL', 'key_passes',       'Key Passes',          'player', 'passing',    false, false, true,   32),
    ('FOOTBALL', 'crosses_total',    'Total Crosses',       'player', 'passing',    false, false, false,  33),
    ('FOOTBALL', 'crosses_accurate', 'Accurate Crosses',    'player', 'passing',    false, false, false,  34),
    -- Defensive
    ('FOOTBALL', 'tackles',          'Tackles',             'player', 'defensive',  false, false, true,   40),
    ('FOOTBALL', 'interceptions',    'Interceptions',       'player', 'defensive',  false, false, true,   41),
    ('FOOTBALL', 'clearances',       'Clearances',          'player', 'defensive',  false, false, false,  42),
    ('FOOTBALL', 'blocks',           'Blocks',              'player', 'defensive',  false, false, false,  43),
    -- Duels
    ('FOOTBALL', 'duels_total',      'Total Duels',         'player', 'duels',      false, false, false,  50),
    ('FOOTBALL', 'duels_won',        'Duels Won',           'player', 'duels',      false, false, false,  51),
    -- Dribbles
    ('FOOTBALL', 'dribbles_attempts','Dribble Attempts',    'player', 'dribbling',  false, false, false,  55),
    ('FOOTBALL', 'dribbles_success', 'Successful Dribbles', 'player', 'dribbling',  false, false, false,  56),
    -- Discipline
    ('FOOTBALL', 'yellow_cards',     'Yellow Cards',        'player', 'discipline', true,  false, true,   60),
    ('FOOTBALL', 'red_cards',        'Red Cards',           'player', 'discipline', true,  false, true,   61),
    ('FOOTBALL', 'fouls_committed',  'Fouls Committed',     'player', 'discipline', true,  false, false,  62),
    ('FOOTBALL', 'fouls_drawn',      'Fouls Drawn',         'player', 'discipline', false, false, false,  63),
    -- Goalkeeper
    ('FOOTBALL', 'saves',            'Saves',               'player', 'goalkeeper', false, false, true,   70),
    ('FOOTBALL', 'goals_conceded',   'Goals Conceded',      'player', 'goalkeeper', true,  false, true,   71),
    -- Derived: per-90 metrics (computed by trigger)
    ('FOOTBALL', 'goals_per_90',     'Goals Per 90',        'player', 'scoring',    false, true,  true,   13),
    ('FOOTBALL', 'assists_per_90',   'Assists Per 90',      'player', 'scoring',    false, true,  true,   14),
    ('FOOTBALL', 'key_passes_per_90','Key Passes Per 90',   'player', 'passing',    false, true,  true,   35),
    ('FOOTBALL', 'shots_per_90',     'Shots Per 90',        'player', 'shooting',   false, true,  true,   22),
    ('FOOTBALL', 'tackles_per_90',   'Tackles Per 90',      'player', 'defensive',  false, true,  true,   44),
    ('FOOTBALL', 'interceptions_per_90','Interceptions/90', 'player', 'defensive',  false, true,  true,   45),
    -- Derived: accuracy rates (computed by trigger)
    ('FOOTBALL', 'shot_accuracy',    'Shot Accuracy %',     'player', 'shooting',   false, true,  true,   23),
    ('FOOTBALL', 'pass_accuracy',    'Pass Accuracy %',     'player', 'passing',    false, true,  true,   36),
    ('FOOTBALL', 'duel_success_rate','Duel Success Rate %', 'player', 'duels',      false, true,  true,   52),
    ('FOOTBALL', 'dribble_success_rate','Dribble Success %','player', 'dribbling',  false, true,  true,   57),
    -- Derived: goalkeeper per-90
    ('FOOTBALL', 'goals_conceded_per_90','Goals Conceded/90','player','goalkeeper', true,  true,  true,   72),
    ('FOOTBALL', 'save_pct',         'Save Percentage %',   'player', 'goalkeeper', false, true,  true,   73)
ON CONFLICT (sport, key_name, entity_type) DO NOTHING;

-- ============================================================================
-- 4b. FOOTBALL TEAM STATS
-- ============================================================================

INSERT INTO stat_definitions (sport, key_name, display_name, entity_type, category, is_inverse, is_derived, is_percentile_eligible, sort_order) VALUES
    -- Overall standings
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
    -- Home/Away splits
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
-- 5. SCHEMA VERSION BUMP
-- ============================================================================

UPDATE meta SET value = '5.3', updated_at = NOW() WHERE key = 'schema_version';
