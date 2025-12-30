-- Scoracle Stats Database - Football (Soccer) Statistics Schema
-- Version: 1.0
-- Created: 2024-12-25

-- ============================================================================
-- FOOTBALL PLAYER STATISTICS
-- ============================================================================

CREATE TABLE IF NOT EXISTS football_player_stats (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    player_id INTEGER NOT NULL REFERENCES players(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    team_id INTEGER REFERENCES teams(id),
    league_id INTEGER REFERENCES leagues(id),

    -- Appearances
    appearances INTEGER DEFAULT 0,
    starts INTEGER DEFAULT 0,
    bench_appearances INTEGER DEFAULT 0,
    minutes_played INTEGER DEFAULT 0,

    -- Scoring
    goals INTEGER DEFAULT 0,
    assists INTEGER DEFAULT 0,
    goals_assists INTEGER DEFAULT 0,
    goals_per_90 REAL DEFAULT 0,
    assists_per_90 REAL DEFAULT 0,

    -- Shots
    shots_total INTEGER DEFAULT 0,
    shots_on_target INTEGER DEFAULT 0,
    shot_accuracy REAL DEFAULT 0,
    shots_per_90 REAL DEFAULT 0,
    goals_per_shot REAL DEFAULT 0,
    goals_per_shot_on_target REAL DEFAULT 0,

    -- Expected Goals (if available)
    expected_goals REAL DEFAULT 0,
    expected_assists REAL DEFAULT 0,
    expected_goals_per_90 REAL DEFAULT 0,

    -- Passing
    passes_total INTEGER DEFAULT 0,
    passes_accurate INTEGER DEFAULT 0,
    pass_accuracy REAL DEFAULT 0,
    passes_per_90 REAL DEFAULT 0,
    key_passes INTEGER DEFAULT 0,
    key_passes_per_90 REAL DEFAULT 0,
    crosses_total INTEGER DEFAULT 0,
    crosses_accurate INTEGER DEFAULT 0,
    cross_accuracy REAL DEFAULT 0,

    -- Long Balls & Through Balls
    long_balls_total INTEGER DEFAULT 0,
    long_balls_accurate INTEGER DEFAULT 0,
    long_ball_accuracy REAL DEFAULT 0,
    through_balls INTEGER DEFAULT 0,

    -- Dribbling
    dribbles_attempted INTEGER DEFAULT 0,
    dribbles_successful INTEGER DEFAULT 0,
    dribble_success_rate REAL DEFAULT 0,
    dribbles_per_90 REAL DEFAULT 0,

    -- Duels
    duels_total INTEGER DEFAULT 0,
    duels_won INTEGER DEFAULT 0,
    duel_success_rate REAL DEFAULT 0,
    aerial_duels_total INTEGER DEFAULT 0,
    aerial_duels_won INTEGER DEFAULT 0,
    aerial_duel_success_rate REAL DEFAULT 0,
    ground_duels_total INTEGER DEFAULT 0,
    ground_duels_won INTEGER DEFAULT 0,

    -- Tackles & Interceptions
    tackles INTEGER DEFAULT 0,
    tackles_won INTEGER DEFAULT 0,
    tackle_success_rate REAL DEFAULT 0,
    tackles_per_90 REAL DEFAULT 0,
    interceptions INTEGER DEFAULT 0,
    interceptions_per_90 REAL DEFAULT 0,
    blocks INTEGER DEFAULT 0,
    clearances INTEGER DEFAULT 0,

    -- Defending
    ball_recoveries INTEGER DEFAULT 0,
    dispossessed INTEGER DEFAULT 0,
    errors_leading_to_goal INTEGER DEFAULT 0,

    -- Fouls & Cards
    fouls_committed INTEGER DEFAULT 0,
    fouls_drawn INTEGER DEFAULT 0,
    yellow_cards INTEGER DEFAULT 0,
    red_cards INTEGER DEFAULT 0,
    second_yellow_cards INTEGER DEFAULT 0,

    -- Penalties
    penalties_won INTEGER DEFAULT 0,
    penalties_scored INTEGER DEFAULT 0,
    penalties_missed INTEGER DEFAULT 0,
    penalties_conceded INTEGER DEFAULT 0,

    -- Goalkeeper Specific
    saves INTEGER DEFAULT 0,
    save_percentage REAL DEFAULT 0,
    goals_conceded INTEGER DEFAULT 0,
    goals_conceded_per_90 REAL DEFAULT 0,
    clean_sheets INTEGER DEFAULT 0,
    penalty_saves INTEGER DEFAULT 0,
    punches INTEGER DEFAULT 0,
    claims INTEGER DEFAULT 0,
    catches INTEGER DEFAULT 0,

    -- Metadata
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(player_id, season_id, league_id)
);

-- Create indexes for Football player stats
CREATE INDEX IF NOT EXISTS idx_football_player_stats_player ON football_player_stats(player_id);
CREATE INDEX IF NOT EXISTS idx_football_player_stats_season ON football_player_stats(season_id);
CREATE INDEX IF NOT EXISTS idx_football_player_stats_league ON football_player_stats(league_id);
CREATE INDEX IF NOT EXISTS idx_football_player_stats_team ON football_player_stats(team_id);
CREATE INDEX IF NOT EXISTS idx_football_player_stats_goals ON football_player_stats(season_id, league_id, goals DESC);

-- ============================================================================
-- FOOTBALL TEAM STATISTICS
-- ============================================================================

CREATE TABLE IF NOT EXISTS football_team_stats (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    team_id INTEGER NOT NULL REFERENCES teams(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    league_id INTEGER NOT NULL REFERENCES leagues(id),

    -- Record
    matches_played INTEGER DEFAULT 0,
    wins INTEGER DEFAULT 0,
    draws INTEGER DEFAULT 0,
    losses INTEGER DEFAULT 0,
    points INTEGER DEFAULT 0,

    -- Home/Away
    home_played INTEGER DEFAULT 0,
    home_wins INTEGER DEFAULT 0,
    home_draws INTEGER DEFAULT 0,
    home_losses INTEGER DEFAULT 0,
    home_goals_for INTEGER DEFAULT 0,
    home_goals_against INTEGER DEFAULT 0,
    away_played INTEGER DEFAULT 0,
    away_wins INTEGER DEFAULT 0,
    away_draws INTEGER DEFAULT 0,
    away_losses INTEGER DEFAULT 0,
    away_goals_for INTEGER DEFAULT 0,
    away_goals_against INTEGER DEFAULT 0,

    -- Goals
    goals_for INTEGER DEFAULT 0,
    goals_against INTEGER DEFAULT 0,
    goal_difference INTEGER DEFAULT 0,
    goals_per_game REAL DEFAULT 0,
    goals_conceded_per_game REAL DEFAULT 0,
    clean_sheets INTEGER DEFAULT 0,
    failed_to_score INTEGER DEFAULT 0,

    -- Scoring Patterns
    scored_first INTEGER DEFAULT 0,
    conceded_first INTEGER DEFAULT 0,
    come_from_behind_wins INTEGER DEFAULT 0,
    dropped_points_from_winning INTEGER DEFAULT 0,

    -- Form (last 5 matches encoded)
    form TEXT,
    form_points INTEGER DEFAULT 0,

    -- Shots
    shots_total INTEGER DEFAULT 0,
    shots_on_target INTEGER DEFAULT 0,
    shots_per_game REAL DEFAULT 0,
    shots_on_target_per_game REAL DEFAULT 0,
    shot_accuracy REAL DEFAULT 0,

    -- Shots Conceded
    shots_against INTEGER DEFAULT 0,
    shots_on_target_against INTEGER DEFAULT 0,

    -- Passing
    passes_per_game REAL DEFAULT 0,
    pass_accuracy REAL DEFAULT 0,
    crosses_per_game REAL DEFAULT 0,

    -- Defense
    tackles_per_game REAL DEFAULT 0,
    interceptions_per_game REAL DEFAULT 0,
    clearances_per_game REAL DEFAULT 0,
    blocks_per_game REAL DEFAULT 0,

    -- Possession
    avg_possession REAL DEFAULT 0,

    -- Set Pieces
    corners_per_game REAL DEFAULT 0,
    free_kicks_per_game REAL DEFAULT 0,

    -- Discipline
    yellow_cards INTEGER DEFAULT 0,
    red_cards INTEGER DEFAULT 0,
    fouls_per_game REAL DEFAULT 0,

    -- Penalties
    penalties_scored INTEGER DEFAULT 0,
    penalties_missed INTEGER DEFAULT 0,
    penalties_conceded INTEGER DEFAULT 0,

    -- Expected Goals (if available)
    expected_goals REAL DEFAULT 0,
    expected_goals_against REAL DEFAULT 0,
    expected_goal_difference REAL DEFAULT 0,

    -- Standings
    league_position INTEGER,
    points_per_game REAL DEFAULT 0,

    -- Metadata
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(team_id, season_id, league_id)
);

-- Create indexes for Football team stats
CREATE INDEX IF NOT EXISTS idx_football_team_stats_team ON football_team_stats(team_id);
CREATE INDEX IF NOT EXISTS idx_football_team_stats_season ON football_team_stats(season_id);
CREATE INDEX IF NOT EXISTS idx_football_team_stats_league ON football_team_stats(league_id);
CREATE INDEX IF NOT EXISTS idx_football_team_stats_position ON football_team_stats(season_id, league_id, league_position);

-- ============================================================================
-- FOOTBALL LEAGUE SEASONS (for standings tracking)
-- ============================================================================

CREATE TABLE IF NOT EXISTS football_standings_snapshot (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    team_id INTEGER NOT NULL REFERENCES teams(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    league_id INTEGER NOT NULL REFERENCES leagues(id),
    matchday INTEGER NOT NULL,
    position INTEGER NOT NULL,
    points INTEGER DEFAULT 0,
    played INTEGER DEFAULT 0,
    wins INTEGER DEFAULT 0,
    draws INTEGER DEFAULT 0,
    losses INTEGER DEFAULT 0,
    goals_for INTEGER DEFAULT 0,
    goals_against INTEGER DEFAULT 0,
    goal_difference INTEGER DEFAULT 0,
    form TEXT,
    snapshot_date TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(team_id, season_id, league_id, matchday)
);

CREATE INDEX IF NOT EXISTS idx_standings_snapshot ON football_standings_snapshot(league_id, season_id, matchday);
