-- Migration: 001_initial_schema
-- Description: Create all tables for NBA, NFL, and Football data
-- Date: 2026-01-25

-- =============================================================================
-- NBA TABLES
-- =============================================================================

CREATE TABLE IF NOT EXISTS nba_team_profiles (
    id INTEGER PRIMARY KEY,           -- BallDontLie ID
    name VARCHAR(100) NOT NULL,
    full_name VARCHAR(100),
    abbreviation VARCHAR(10),
    city VARCHAR(100),
    conference VARCHAR(20),
    division VARCHAR(20),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS nba_player_profiles (
    id INTEGER PRIMARY KEY,           -- BallDontLie ID
    first_name VARCHAR(100) NOT NULL,
    last_name VARCHAR(100) NOT NULL,
    position VARCHAR(20),
    height VARCHAR(10),               -- "6-2"
    weight VARCHAR(10),               -- "185"
    jersey_number VARCHAR(10),
    college VARCHAR(100),
    country VARCHAR(100),
    draft_year INTEGER,
    draft_round INTEGER,
    draft_number INTEGER,
    team_id INTEGER REFERENCES nba_team_profiles(id),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS nba_player_stats (
    id SERIAL PRIMARY KEY,
    player_id INTEGER NOT NULL REFERENCES nba_player_profiles(id),
    season INTEGER NOT NULL,
    season_type VARCHAR(20) NOT NULL DEFAULT 'regular',
    games_played INTEGER,
    minutes DECIMAL(6,2),
    pts DECIMAL(6,2),
    reb DECIMAL(6,2),
    ast DECIMAL(6,2),
    stl DECIMAL(6,2),
    blk DECIMAL(6,2),
    fg_pct DECIMAL(5,4),
    fg3_pct DECIMAL(5,4),
    ft_pct DECIMAL(5,4),
    fgm DECIMAL(6,2),
    fga DECIMAL(6,2),
    fg3m DECIMAL(6,2),
    fg3a DECIMAL(6,2),
    ftm DECIMAL(6,2),
    fta DECIMAL(6,2),
    oreb DECIMAL(6,2),
    dreb DECIMAL(6,2),
    turnover DECIMAL(6,2),
    pf DECIMAL(6,2),
    plus_minus DECIMAL(6,2),
    raw_json JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(player_id, season, season_type)
);

CREATE TABLE IF NOT EXISTS nba_team_stats (
    id SERIAL PRIMARY KEY,
    team_id INTEGER NOT NULL REFERENCES nba_team_profiles(id),
    season INTEGER NOT NULL,
    season_type VARCHAR(20) NOT NULL DEFAULT 'regular',
    wins INTEGER,
    losses INTEGER,
    games_played INTEGER,
    pts DECIMAL(6,2),
    reb DECIMAL(6,2),
    ast DECIMAL(6,2),
    fg_pct DECIMAL(5,4),
    fg3_pct DECIMAL(5,4),
    ft_pct DECIMAL(5,4),
    raw_json JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(team_id, season, season_type)
);

-- NBA Indexes
CREATE INDEX IF NOT EXISTS idx_nba_player_profiles_team ON nba_player_profiles(team_id);
CREATE INDEX IF NOT EXISTS idx_nba_player_stats_season ON nba_player_stats(season);
CREATE INDEX IF NOT EXISTS idx_nba_player_stats_player ON nba_player_stats(player_id);
CREATE INDEX IF NOT EXISTS idx_nba_team_stats_season ON nba_team_stats(season);
CREATE INDEX IF NOT EXISTS idx_nba_player_stats_json ON nba_player_stats USING GIN (raw_json);
CREATE INDEX IF NOT EXISTS idx_nba_team_stats_json ON nba_team_stats USING GIN (raw_json);

-- =============================================================================
-- NFL TABLES
-- =============================================================================

CREATE TABLE IF NOT EXISTS nfl_team_profiles (
    id INTEGER PRIMARY KEY,           -- BallDontLie ID
    name VARCHAR(100) NOT NULL,
    full_name VARCHAR(100),
    abbreviation VARCHAR(10),
    location VARCHAR(100),
    conference VARCHAR(10),
    division VARCHAR(20),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS nfl_player_profiles (
    id INTEGER PRIMARY KEY,           -- BallDontLie ID
    first_name VARCHAR(100) NOT NULL,
    last_name VARCHAR(100) NOT NULL,
    position VARCHAR(50),
    position_abbreviation VARCHAR(10),
    height VARCHAR(20),
    weight VARCHAR(20),
    jersey_number VARCHAR(10),
    college VARCHAR(100),
    experience VARCHAR(50),
    age INTEGER,
    team_id INTEGER REFERENCES nfl_team_profiles(id),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS nfl_player_stats (
    id SERIAL PRIMARY KEY,
    player_id INTEGER NOT NULL REFERENCES nfl_player_profiles(id),
    season INTEGER NOT NULL,
    postseason BOOLEAN NOT NULL DEFAULT FALSE,
    games_played INTEGER,
    -- Passing
    passing_completions INTEGER,
    passing_attempts INTEGER,
    passing_yards INTEGER,
    passing_touchdowns INTEGER,
    passing_interceptions INTEGER,
    passing_yards_per_game DECIMAL(8,3),
    passing_completion_pct DECIMAL(6,3),
    qbr DECIMAL(6,2),
    -- Rushing
    rushing_attempts INTEGER,
    rushing_yards INTEGER,
    rushing_touchdowns INTEGER,
    rushing_yards_per_game DECIMAL(8,3),
    yards_per_rush_attempt DECIMAL(6,3),
    rushing_first_downs INTEGER,
    -- Receiving
    receptions INTEGER,
    receiving_yards INTEGER,
    receiving_touchdowns INTEGER,
    receiving_targets INTEGER,
    receiving_yards_per_game DECIMAL(8,3),
    yards_per_reception DECIMAL(6,3),
    receiving_first_downs INTEGER,
    -- Defensive
    total_tackles INTEGER,
    solo_tackles INTEGER,
    assist_tackles INTEGER,
    defensive_sacks DECIMAL(5,1),
    defensive_sack_yards DECIMAL(6,1),
    defensive_interceptions INTEGER,
    interception_touchdowns INTEGER,
    fumbles_forced INTEGER,
    fumbles_recovered INTEGER,
    -- Kicking/Punting
    field_goal_attempts INTEGER,
    field_goals_made INTEGER,
    field_goal_pct DECIMAL(6,3),
    punts INTEGER,
    punt_yards INTEGER,
    -- Returns
    kick_returns INTEGER,
    kick_return_yards INTEGER,
    kick_return_touchdowns INTEGER,
    punt_returner_returns INTEGER,
    punt_returner_return_yards INTEGER,
    punt_return_touchdowns INTEGER,
    -- Raw data
    raw_json JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(player_id, season, postseason)
);

CREATE TABLE IF NOT EXISTS nfl_team_stats (
    id SERIAL PRIMARY KEY,
    team_id INTEGER NOT NULL REFERENCES nfl_team_profiles(id),
    season INTEGER NOT NULL,
    postseason BOOLEAN NOT NULL DEFAULT FALSE,
    wins INTEGER,
    losses INTEGER,
    ties INTEGER,
    points_for INTEGER,
    points_against INTEGER,
    point_differential INTEGER,
    raw_json JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(team_id, season, postseason)
);

-- NFL Indexes
CREATE INDEX IF NOT EXISTS idx_nfl_player_profiles_team ON nfl_player_profiles(team_id);
CREATE INDEX IF NOT EXISTS idx_nfl_player_stats_season ON nfl_player_stats(season);
CREATE INDEX IF NOT EXISTS idx_nfl_player_stats_player ON nfl_player_stats(player_id);
CREATE INDEX IF NOT EXISTS idx_nfl_team_stats_season ON nfl_team_stats(season);
CREATE INDEX IF NOT EXISTS idx_nfl_player_stats_json ON nfl_player_stats USING GIN (raw_json);
CREATE INDEX IF NOT EXISTS idx_nfl_team_stats_json ON nfl_team_stats USING GIN (raw_json);

-- =============================================================================
-- FOOTBALL (SOCCER) TABLES
-- =============================================================================

CREATE TABLE IF NOT EXISTS football_leagues (
    id INTEGER PRIMARY KEY,           -- Our internal ID (1-5)
    sportmonks_id INTEGER NOT NULL UNIQUE,
    name VARCHAR(100) NOT NULL,
    country VARCHAR(100),
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Insert Top 5 leagues
INSERT INTO football_leagues (id, sportmonks_id, name, country) VALUES
    (1, 8, 'Premier League', 'England'),
    (2, 564, 'La Liga', 'Spain'),
    (3, 82, 'Bundesliga', 'Germany'),
    (4, 384, 'Serie A', 'Italy'),
    (5, 301, 'Ligue 1', 'France')
ON CONFLICT (id) DO NOTHING;

CREATE TABLE IF NOT EXISTS football_team_profiles (
    id INTEGER PRIMARY KEY,           -- SportMonks team ID
    name VARCHAR(100) NOT NULL,
    short_code VARCHAR(10),
    country VARCHAR(100),
    logo_url TEXT,
    venue_name VARCHAR(200),
    venue_capacity INTEGER,
    founded INTEGER,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS football_player_profiles (
    id INTEGER PRIMARY KEY,           -- SportMonks player ID
    common_name VARCHAR(100),
    first_name VARCHAR(100),
    last_name VARCHAR(100),
    display_name VARCHAR(100),
    nationality VARCHAR(100),
    nationality_id INTEGER,
    position VARCHAR(50),
    detailed_position VARCHAR(100),
    position_id INTEGER,
    height INTEGER,                   -- cm
    weight INTEGER,                   -- kg
    date_of_birth DATE,
    image_url TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS football_player_stats (
    id SERIAL PRIMARY KEY,
    player_id INTEGER NOT NULL REFERENCES football_player_profiles(id),
    team_id INTEGER REFERENCES football_team_profiles(id),
    league_id INTEGER NOT NULL REFERENCES football_leagues(id),
    season INTEGER NOT NULL,
    sportmonks_season_id INTEGER,
    -- Appearances
    appearances INTEGER,
    lineups INTEGER,
    minutes_played INTEGER,
    -- Goals & Assists
    goals INTEGER,
    assists INTEGER,
    -- Shooting
    shots_total INTEGER,
    shots_on_target INTEGER,
    -- Passing
    passes_total INTEGER,
    passes_accurate INTEGER,
    key_passes INTEGER,
    crosses_total INTEGER,
    crosses_accurate INTEGER,
    -- Defensive
    tackles INTEGER,
    interceptions INTEGER,
    clearances INTEGER,
    blocks INTEGER,
    -- Duels
    duels_total INTEGER,
    duels_won INTEGER,
    -- Dribbles
    dribbles_attempts INTEGER,
    dribbles_success INTEGER,
    -- Discipline
    yellow_cards INTEGER,
    red_cards INTEGER,
    fouls_committed INTEGER,
    fouls_drawn INTEGER,
    -- Goalkeeper
    saves INTEGER,
    goals_conceded INTEGER,
    -- Per-90 (computed)
    goals_per_90 DECIMAL(6,3),
    assists_per_90 DECIMAL(6,3),
    -- xG if available
    expected_goals DECIMAL(8,4),
    expected_assists DECIMAL(8,4),
    -- Raw data
    raw_json JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(player_id, league_id, season)
);

CREATE TABLE IF NOT EXISTS football_team_stats (
    id SERIAL PRIMARY KEY,
    team_id INTEGER NOT NULL REFERENCES football_team_profiles(id),
    league_id INTEGER NOT NULL REFERENCES football_leagues(id),
    season INTEGER NOT NULL,
    sportmonks_season_id INTEGER,
    -- Standings
    wins INTEGER,
    draws INTEGER,
    losses INTEGER,
    goals_for INTEGER,
    goals_against INTEGER,
    goal_difference INTEGER,
    points INTEGER,
    position INTEGER,
    -- Form
    form VARCHAR(20),
    -- Raw data
    raw_json JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(team_id, league_id, season)
);

-- Football Indexes
CREATE INDEX IF NOT EXISTS idx_football_player_profiles_nationality ON football_player_profiles(nationality_id);
CREATE INDEX IF NOT EXISTS idx_football_player_stats_season ON football_player_stats(season);
CREATE INDEX IF NOT EXISTS idx_football_player_stats_player ON football_player_stats(player_id);
CREATE INDEX IF NOT EXISTS idx_football_player_stats_league ON football_player_stats(league_id);
CREATE INDEX IF NOT EXISTS idx_football_player_stats_team ON football_player_stats(team_id);
CREATE INDEX IF NOT EXISTS idx_football_team_stats_season ON football_team_stats(season);
CREATE INDEX IF NOT EXISTS idx_football_team_stats_league ON football_team_stats(league_id);
CREATE INDEX IF NOT EXISTS idx_football_player_stats_json ON football_player_stats USING GIN (raw_json);
CREATE INDEX IF NOT EXISTS idx_football_team_stats_json ON football_team_stats USING GIN (raw_json);
