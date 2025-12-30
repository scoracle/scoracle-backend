-- Scoracle Stats Database - Initial Schema
-- Version: 1.0
-- Created: 2024-12-25

-- ============================================================================
-- CORE TABLES (Sport-Agnostic)
-- ============================================================================

-- Metadata table for tracking updates and migrations
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Initialize metadata
INSERT OR IGNORE INTO meta (key, value, updated_at) VALUES
    ('schema_version', '1.0', strftime('%s', 'now')),
    ('last_full_sync', '', 0),
    ('last_incremental_sync', '', 0);

-- Sports registry
CREATE TABLE IF NOT EXISTS sports (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    api_base_url TEXT NOT NULL,
    current_season INTEGER NOT NULL,
    is_active INTEGER DEFAULT 1,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

-- Insert supported sports
INSERT OR IGNORE INTO sports (id, display_name, api_base_url, current_season) VALUES
    ('NBA', 'NBA Basketball', 'https://v2.nba.api-sports.io', 2025),
    ('NFL', 'NFL Football', 'https://v1.american-football.api-sports.io', 2025),
    ('FOOTBALL', 'Football (Soccer)', 'https://v3.football.api-sports.io', 2024);

-- Seasons registry
CREATE TABLE IF NOT EXISTS seasons (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    sport_id TEXT NOT NULL REFERENCES sports(id),
    season_year INTEGER NOT NULL,
    season_label TEXT,
    is_current INTEGER DEFAULT 0,
    start_date TEXT,
    end_date TEXT,
    games_played INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(sport_id, season_year)
);

-- Create indexes for seasons
CREATE INDEX IF NOT EXISTS idx_seasons_sport ON seasons(sport_id);
CREATE INDEX IF NOT EXISTS idx_seasons_current ON seasons(sport_id, is_current);

-- Leagues registry (for multi-league sports like Football)
CREATE TABLE IF NOT EXISTS leagues (
    id INTEGER PRIMARY KEY,
    sport_id TEXT NOT NULL REFERENCES sports(id),
    name TEXT NOT NULL,
    country TEXT,
    country_code TEXT,
    logo_url TEXT,
    is_active INTEGER DEFAULT 1,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

-- Create indexes for leagues
CREATE INDEX IF NOT EXISTS idx_leagues_sport ON leagues(sport_id);
CREATE INDEX IF NOT EXISTS idx_leagues_country ON leagues(country);

-- Teams master table
CREATE TABLE IF NOT EXISTS teams (
    id INTEGER PRIMARY KEY,
    sport_id TEXT NOT NULL REFERENCES sports(id),
    league_id INTEGER REFERENCES leagues(id),
    name TEXT NOT NULL,
    abbreviation TEXT,
    logo_url TEXT,
    conference TEXT,
    division TEXT,
    country TEXT,
    city TEXT,
    founded INTEGER,
    venue_name TEXT,
    venue_capacity INTEGER,
    is_active INTEGER DEFAULT 1,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

-- Create indexes for teams
CREATE INDEX IF NOT EXISTS idx_teams_sport ON teams(sport_id);
CREATE INDEX IF NOT EXISTS idx_teams_league ON teams(league_id);
CREATE INDEX IF NOT EXISTS idx_teams_name ON teams(name);
CREATE INDEX IF NOT EXISTS idx_teams_conference ON teams(sport_id, conference);

-- Players master table
CREATE TABLE IF NOT EXISTS players (
    id INTEGER PRIMARY KEY,
    sport_id TEXT NOT NULL REFERENCES sports(id),
    first_name TEXT,
    last_name TEXT,
    full_name TEXT NOT NULL,
    position TEXT,
    position_group TEXT,
    nationality TEXT,
    birth_date TEXT,
    birth_place TEXT,
    height_cm INTEGER,
    weight_kg INTEGER,
    photo_url TEXT,
    current_team_id INTEGER REFERENCES teams(id),
    jersey_number INTEGER,
    is_active INTEGER DEFAULT 1,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

-- Create indexes for players
CREATE INDEX IF NOT EXISTS idx_players_sport ON players(sport_id);
CREATE INDEX IF NOT EXISTS idx_players_team ON players(current_team_id);
CREATE INDEX IF NOT EXISTS idx_players_name ON players(full_name);
CREATE INDEX IF NOT EXISTS idx_players_position ON players(sport_id, position);

-- Player-Team history (for trades/transfers)
CREATE TABLE IF NOT EXISTS player_teams (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    player_id INTEGER NOT NULL REFERENCES players(id),
    team_id INTEGER NOT NULL REFERENCES teams(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    start_date TEXT,
    end_date TEXT,
    is_current INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(player_id, team_id, season_id)
);

-- Create indexes for player_teams
CREATE INDEX IF NOT EXISTS idx_player_teams_player ON player_teams(player_id);
CREATE INDEX IF NOT EXISTS idx_player_teams_team ON player_teams(team_id);
CREATE INDEX IF NOT EXISTS idx_player_teams_season ON player_teams(season_id);

-- ============================================================================
-- PERCENTILE CACHE TABLE
-- ============================================================================

CREATE TABLE IF NOT EXISTS percentile_cache (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_type TEXT NOT NULL CHECK(entity_type IN ('player', 'team')),
    entity_id INTEGER NOT NULL,
    sport_id TEXT NOT NULL REFERENCES sports(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    stat_category TEXT NOT NULL,
    stat_value REAL NOT NULL,
    percentile REAL NOT NULL CHECK(percentile >= 0 AND percentile <= 100),
    rank INTEGER NOT NULL,
    sample_size INTEGER NOT NULL,
    comparison_group TEXT,
    calculated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(entity_type, entity_id, sport_id, season_id, stat_category)
);

-- Create indexes for percentile lookups
CREATE INDEX IF NOT EXISTS idx_percentile_entity ON percentile_cache(entity_type, entity_id);
CREATE INDEX IF NOT EXISTS idx_percentile_lookup ON percentile_cache(entity_type, sport_id, season_id, stat_category);
CREATE INDEX IF NOT EXISTS idx_percentile_ranking ON percentile_cache(sport_id, season_id, stat_category, percentile DESC);

-- ============================================================================
-- SYNC TRACKING TABLE
-- ============================================================================

CREATE TABLE IF NOT EXISTS sync_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    sport_id TEXT NOT NULL REFERENCES sports(id),
    sync_type TEXT NOT NULL CHECK(sync_type IN ('full', 'incremental', 'percentile')),
    entity_type TEXT,
    season_id INTEGER REFERENCES seasons(id),
    started_at INTEGER NOT NULL,
    completed_at INTEGER,
    records_processed INTEGER DEFAULT 0,
    records_inserted INTEGER DEFAULT 0,
    records_updated INTEGER DEFAULT 0,
    status TEXT NOT NULL CHECK(status IN ('running', 'completed', 'failed')),
    error_message TEXT
);

CREATE INDEX IF NOT EXISTS idx_sync_log_sport ON sync_log(sport_id, sync_type);
CREATE INDEX IF NOT EXISTS idx_sync_log_status ON sync_log(status);
