-- Scoracle Stats Database - NBA Statistics Schema
-- Version: 1.0
-- Created: 2024-12-25

-- ============================================================================
-- NBA PLAYER STATISTICS
-- ============================================================================

CREATE TABLE IF NOT EXISTS nba_player_stats (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    player_id INTEGER NOT NULL REFERENCES players(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    team_id INTEGER REFERENCES teams(id),

    -- Games & Minutes
    games_played INTEGER DEFAULT 0,
    games_started INTEGER DEFAULT 0,
    minutes_total INTEGER DEFAULT 0,
    minutes_per_game REAL DEFAULT 0,

    -- Scoring
    points_total INTEGER DEFAULT 0,
    points_per_game REAL DEFAULT 0,

    -- Field Goals
    fgm INTEGER DEFAULT 0,
    fga INTEGER DEFAULT 0,
    fg_pct REAL DEFAULT 0,

    -- Three Pointers
    tpm INTEGER DEFAULT 0,
    tpa INTEGER DEFAULT 0,
    tp_pct REAL DEFAULT 0,

    -- Free Throws
    ftm INTEGER DEFAULT 0,
    fta INTEGER DEFAULT 0,
    ft_pct REAL DEFAULT 0,

    -- Rebounds
    offensive_rebounds INTEGER DEFAULT 0,
    defensive_rebounds INTEGER DEFAULT 0,
    total_rebounds INTEGER DEFAULT 0,
    rebounds_per_game REAL DEFAULT 0,

    -- Assists & Turnovers
    assists INTEGER DEFAULT 0,
    assists_per_game REAL DEFAULT 0,
    turnovers INTEGER DEFAULT 0,
    turnovers_per_game REAL DEFAULT 0,

    -- Defense
    steals INTEGER DEFAULT 0,
    steals_per_game REAL DEFAULT 0,
    blocks INTEGER DEFAULT 0,
    blocks_per_game REAL DEFAULT 0,

    -- Fouls
    personal_fouls INTEGER DEFAULT 0,
    fouls_per_game REAL DEFAULT 0,

    -- Advanced
    plus_minus INTEGER DEFAULT 0,
    plus_minus_per_game REAL DEFAULT 0,
    efficiency REAL DEFAULT 0,
    true_shooting_pct REAL DEFAULT 0,
    effective_fg_pct REAL DEFAULT 0,
    assist_turnover_ratio REAL DEFAULT 0,

    -- Double-doubles, triple-doubles
    double_doubles INTEGER DEFAULT 0,
    triple_doubles INTEGER DEFAULT 0,

    -- Metadata
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(player_id, season_id, team_id)
);

-- Create indexes for NBA player stats
CREATE INDEX IF NOT EXISTS idx_nba_player_stats_player ON nba_player_stats(player_id);
CREATE INDEX IF NOT EXISTS idx_nba_player_stats_season ON nba_player_stats(season_id);
CREATE INDEX IF NOT EXISTS idx_nba_player_stats_team ON nba_player_stats(team_id);
CREATE INDEX IF NOT EXISTS idx_nba_player_stats_ppg ON nba_player_stats(season_id, points_per_game DESC);

-- ============================================================================
-- NBA TEAM STATISTICS
-- ============================================================================

CREATE TABLE IF NOT EXISTS nba_team_stats (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    team_id INTEGER NOT NULL REFERENCES teams(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),

    -- Record
    games_played INTEGER DEFAULT 0,
    wins INTEGER DEFAULT 0,
    losses INTEGER DEFAULT 0,
    win_pct REAL DEFAULT 0,
    home_wins INTEGER DEFAULT 0,
    home_losses INTEGER DEFAULT 0,
    away_wins INTEGER DEFAULT 0,
    away_losses INTEGER DEFAULT 0,
    conference_wins INTEGER DEFAULT 0,
    conference_losses INTEGER DEFAULT 0,
    division_wins INTEGER DEFAULT 0,
    division_losses INTEGER DEFAULT 0,

    -- Streaks
    current_streak INTEGER DEFAULT 0,
    streak_type TEXT,
    last_ten_wins INTEGER DEFAULT 0,
    last_ten_losses INTEGER DEFAULT 0,

    -- Scoring
    points_total INTEGER DEFAULT 0,
    points_per_game REAL DEFAULT 0,
    opponent_ppg REAL DEFAULT 0,
    point_differential REAL DEFAULT 0,

    -- Shooting
    fg_pct REAL DEFAULT 0,
    tp_pct REAL DEFAULT 0,
    ft_pct REAL DEFAULT 0,
    fgm_per_game REAL DEFAULT 0,
    fga_per_game REAL DEFAULT 0,
    tpm_per_game REAL DEFAULT 0,
    tpa_per_game REAL DEFAULT 0,
    ftm_per_game REAL DEFAULT 0,
    fta_per_game REAL DEFAULT 0,

    -- Rebounds
    offensive_rebounds_per_game REAL DEFAULT 0,
    defensive_rebounds_per_game REAL DEFAULT 0,
    total_rebounds_per_game REAL DEFAULT 0,
    opponent_rebounds_per_game REAL DEFAULT 0,

    -- Other Team Stats
    assists_per_game REAL DEFAULT 0,
    steals_per_game REAL DEFAULT 0,
    blocks_per_game REAL DEFAULT 0,
    turnovers_per_game REAL DEFAULT 0,
    fouls_per_game REAL DEFAULT 0,

    -- Advanced
    offensive_rating REAL DEFAULT 0,
    defensive_rating REAL DEFAULT 0,
    net_rating REAL DEFAULT 0,
    pace REAL DEFAULT 0,
    true_shooting_pct REAL DEFAULT 0,
    effective_fg_pct REAL DEFAULT 0,

    -- Rankings
    conference_rank INTEGER,
    division_rank INTEGER,
    league_rank INTEGER,

    -- Metadata
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(team_id, season_id)
);

-- Create indexes for NBA team stats
CREATE INDEX IF NOT EXISTS idx_nba_team_stats_team ON nba_team_stats(team_id);
CREATE INDEX IF NOT EXISTS idx_nba_team_stats_season ON nba_team_stats(season_id);
CREATE INDEX IF NOT EXISTS idx_nba_team_stats_wins ON nba_team_stats(season_id, win_pct DESC);
