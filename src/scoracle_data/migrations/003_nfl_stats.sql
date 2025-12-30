-- Scoracle Stats Database - NFL Statistics Schema
-- Version: 1.0
-- Created: 2024-12-25

-- ============================================================================
-- NFL PLAYER PASSING STATISTICS (QB)
-- ============================================================================

CREATE TABLE IF NOT EXISTS nfl_player_passing (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    player_id INTEGER NOT NULL REFERENCES players(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    team_id INTEGER REFERENCES teams(id),

    -- Games
    games_played INTEGER DEFAULT 0,
    games_started INTEGER DEFAULT 0,

    -- Passing Core
    pass_attempts INTEGER DEFAULT 0,
    pass_completions INTEGER DEFAULT 0,
    completion_pct REAL DEFAULT 0,
    pass_yards INTEGER DEFAULT 0,
    pass_yards_per_game REAL DEFAULT 0,
    yards_per_attempt REAL DEFAULT 0,
    yards_per_completion REAL DEFAULT 0,
    pass_touchdowns INTEGER DEFAULT 0,
    interceptions INTEGER DEFAULT 0,
    td_int_ratio REAL DEFAULT 0,

    -- Advanced Passing
    passer_rating REAL DEFAULT 0,
    qbr REAL DEFAULT 0,
    air_yards INTEGER DEFAULT 0,
    air_yards_per_attempt REAL DEFAULT 0,
    completed_air_yards INTEGER DEFAULT 0,

    -- Pressure & Sacks
    sacks_taken INTEGER DEFAULT 0,
    sack_yards_lost INTEGER DEFAULT 0,
    sack_pct REAL DEFAULT 0,
    times_hit INTEGER DEFAULT 0,
    hurries INTEGER DEFAULT 0,

    -- Big Plays
    longest_pass INTEGER DEFAULT 0,
    passes_20_plus INTEGER DEFAULT 0,
    passes_40_plus INTEGER DEFAULT 0,

    -- Red Zone
    red_zone_attempts INTEGER DEFAULT 0,
    red_zone_completions INTEGER DEFAULT 0,
    red_zone_touchdowns INTEGER DEFAULT 0,

    -- Metadata
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(player_id, season_id)
);

CREATE INDEX IF NOT EXISTS idx_nfl_passing_player ON nfl_player_passing(player_id);
CREATE INDEX IF NOT EXISTS idx_nfl_passing_season ON nfl_player_passing(season_id);

-- ============================================================================
-- NFL PLAYER RUSHING STATISTICS (RB, QB, WR)
-- ============================================================================

CREATE TABLE IF NOT EXISTS nfl_player_rushing (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    player_id INTEGER NOT NULL REFERENCES players(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    team_id INTEGER REFERENCES teams(id),

    -- Games
    games_played INTEGER DEFAULT 0,

    -- Rushing Core
    rush_attempts INTEGER DEFAULT 0,
    rush_yards INTEGER DEFAULT 0,
    rush_yards_per_game REAL DEFAULT 0,
    yards_per_carry REAL DEFAULT 0,
    rush_touchdowns INTEGER DEFAULT 0,
    longest_rush INTEGER DEFAULT 0,

    -- Efficiency
    first_downs INTEGER DEFAULT 0,
    first_down_pct REAL DEFAULT 0,
    yards_after_contact INTEGER DEFAULT 0,
    yards_after_contact_per_carry REAL DEFAULT 0,
    broken_tackles INTEGER DEFAULT 0,

    -- Big Plays
    rushes_20_plus INTEGER DEFAULT 0,
    rushes_40_plus INTEGER DEFAULT 0,

    -- Ball Security
    fumbles INTEGER DEFAULT 0,
    fumbles_lost INTEGER DEFAULT 0,

    -- Red Zone
    red_zone_attempts INTEGER DEFAULT 0,
    red_zone_touchdowns INTEGER DEFAULT 0,

    -- Metadata
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(player_id, season_id)
);

CREATE INDEX IF NOT EXISTS idx_nfl_rushing_player ON nfl_player_rushing(player_id);
CREATE INDEX IF NOT EXISTS idx_nfl_rushing_season ON nfl_player_rushing(season_id);

-- ============================================================================
-- NFL PLAYER RECEIVING STATISTICS (WR, TE, RB)
-- ============================================================================

CREATE TABLE IF NOT EXISTS nfl_player_receiving (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    player_id INTEGER NOT NULL REFERENCES players(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    team_id INTEGER REFERENCES teams(id),

    -- Games
    games_played INTEGER DEFAULT 0,

    -- Receiving Core
    targets INTEGER DEFAULT 0,
    receptions INTEGER DEFAULT 0,
    catch_pct REAL DEFAULT 0,
    receiving_yards INTEGER DEFAULT 0,
    receiving_yards_per_game REAL DEFAULT 0,
    yards_per_reception REAL DEFAULT 0,
    yards_per_target REAL DEFAULT 0,
    receiving_touchdowns INTEGER DEFAULT 0,
    longest_reception INTEGER DEFAULT 0,

    -- Advanced
    target_share REAL DEFAULT 0,
    yards_after_catch INTEGER DEFAULT 0,
    yards_after_catch_per_reception REAL DEFAULT 0,
    first_downs INTEGER DEFAULT 0,
    contested_catches INTEGER DEFAULT 0,
    contested_catch_pct REAL DEFAULT 0,
    drops INTEGER DEFAULT 0,
    drop_pct REAL DEFAULT 0,

    -- Big Plays
    receptions_20_plus INTEGER DEFAULT 0,
    receptions_40_plus INTEGER DEFAULT 0,

    -- Ball Security
    fumbles INTEGER DEFAULT 0,
    fumbles_lost INTEGER DEFAULT 0,

    -- Red Zone
    red_zone_targets INTEGER DEFAULT 0,
    red_zone_receptions INTEGER DEFAULT 0,
    red_zone_touchdowns INTEGER DEFAULT 0,

    -- Metadata
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(player_id, season_id)
);

CREATE INDEX IF NOT EXISTS idx_nfl_receiving_player ON nfl_player_receiving(player_id);
CREATE INDEX IF NOT EXISTS idx_nfl_receiving_season ON nfl_player_receiving(season_id);

-- ============================================================================
-- NFL PLAYER DEFENSE STATISTICS (LB, DB, DL)
-- ============================================================================

CREATE TABLE IF NOT EXISTS nfl_player_defense (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    player_id INTEGER NOT NULL REFERENCES players(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    team_id INTEGER REFERENCES teams(id),

    -- Games
    games_played INTEGER DEFAULT 0,
    games_started INTEGER DEFAULT 0,
    snaps INTEGER DEFAULT 0,

    -- Tackles
    tackles_total INTEGER DEFAULT 0,
    tackles_solo INTEGER DEFAULT 0,
    tackles_assist INTEGER DEFAULT 0,
    tackles_for_loss INTEGER DEFAULT 0,
    tackles_for_loss_yards INTEGER DEFAULT 0,

    -- Pass Rush
    sacks REAL DEFAULT 0,
    sack_yards INTEGER DEFAULT 0,
    qb_hits INTEGER DEFAULT 0,
    hurries INTEGER DEFAULT 0,
    pressures INTEGER DEFAULT 0,
    pressure_pct REAL DEFAULT 0,

    -- Turnovers
    interceptions INTEGER DEFAULT 0,
    int_yards INTEGER DEFAULT 0,
    int_touchdowns INTEGER DEFAULT 0,
    int_longest INTEGER DEFAULT 0,
    passes_defended INTEGER DEFAULT 0,
    forced_fumbles INTEGER DEFAULT 0,
    fumble_recoveries INTEGER DEFAULT 0,
    fumble_recovery_yards INTEGER DEFAULT 0,
    fumble_touchdowns INTEGER DEFAULT 0,

    -- Coverage
    targets INTEGER DEFAULT 0,
    completions_allowed INTEGER DEFAULT 0,
    yards_allowed INTEGER DEFAULT 0,
    touchdowns_allowed INTEGER DEFAULT 0,
    passer_rating_allowed REAL DEFAULT 0,

    -- Safeties
    safeties INTEGER DEFAULT 0,

    -- Metadata
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(player_id, season_id)
);

CREATE INDEX IF NOT EXISTS idx_nfl_defense_player ON nfl_player_defense(player_id);
CREATE INDEX IF NOT EXISTS idx_nfl_defense_season ON nfl_player_defense(season_id);

-- ============================================================================
-- NFL PLAYER KICKING STATISTICS (K, P)
-- ============================================================================

CREATE TABLE IF NOT EXISTS nfl_player_kicking (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    player_id INTEGER NOT NULL REFERENCES players(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    team_id INTEGER REFERENCES teams(id),

    -- Games
    games_played INTEGER DEFAULT 0,

    -- Field Goals
    fg_attempts INTEGER DEFAULT 0,
    fg_made INTEGER DEFAULT 0,
    fg_pct REAL DEFAULT 0,
    fg_long INTEGER DEFAULT 0,
    fg_0_19_made INTEGER DEFAULT 0,
    fg_0_19_attempts INTEGER DEFAULT 0,
    fg_20_29_made INTEGER DEFAULT 0,
    fg_20_29_attempts INTEGER DEFAULT 0,
    fg_30_39_made INTEGER DEFAULT 0,
    fg_30_39_attempts INTEGER DEFAULT 0,
    fg_40_49_made INTEGER DEFAULT 0,
    fg_40_49_attempts INTEGER DEFAULT 0,
    fg_50_plus_made INTEGER DEFAULT 0,
    fg_50_plus_attempts INTEGER DEFAULT 0,

    -- Extra Points
    xp_attempts INTEGER DEFAULT 0,
    xp_made INTEGER DEFAULT 0,
    xp_pct REAL DEFAULT 0,

    -- Points
    total_points INTEGER DEFAULT 0,

    -- Punting
    punts INTEGER DEFAULT 0,
    punt_yards INTEGER DEFAULT 0,
    punt_avg REAL DEFAULT 0,
    punt_long INTEGER DEFAULT 0,
    punts_inside_20 INTEGER DEFAULT 0,
    touchbacks INTEGER DEFAULT 0,
    punt_net_avg REAL DEFAULT 0,

    -- Metadata
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(player_id, season_id)
);

CREATE INDEX IF NOT EXISTS idx_nfl_kicking_player ON nfl_player_kicking(player_id);
CREATE INDEX IF NOT EXISTS idx_nfl_kicking_season ON nfl_player_kicking(season_id);

-- ============================================================================
-- NFL TEAM STATISTICS
-- ============================================================================

CREATE TABLE IF NOT EXISTS nfl_team_stats (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    team_id INTEGER NOT NULL REFERENCES teams(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),

    -- Record
    games_played INTEGER DEFAULT 0,
    wins INTEGER DEFAULT 0,
    losses INTEGER DEFAULT 0,
    ties INTEGER DEFAULT 0,
    win_pct REAL DEFAULT 0,
    home_wins INTEGER DEFAULT 0,
    home_losses INTEGER DEFAULT 0,
    home_ties INTEGER DEFAULT 0,
    away_wins INTEGER DEFAULT 0,
    away_losses INTEGER DEFAULT 0,
    away_ties INTEGER DEFAULT 0,
    division_wins INTEGER DEFAULT 0,
    division_losses INTEGER DEFAULT 0,
    conference_wins INTEGER DEFAULT 0,
    conference_losses INTEGER DEFAULT 0,

    -- Scoring
    points_for INTEGER DEFAULT 0,
    points_against INTEGER DEFAULT 0,
    point_differential INTEGER DEFAULT 0,
    points_per_game REAL DEFAULT 0,
    opponent_ppg REAL DEFAULT 0,

    -- Total Offense
    total_yards INTEGER DEFAULT 0,
    yards_per_game REAL DEFAULT 0,
    plays INTEGER DEFAULT 0,
    yards_per_play REAL DEFAULT 0,

    -- Passing Offense
    pass_yards INTEGER DEFAULT 0,
    pass_yards_per_game REAL DEFAULT 0,
    pass_attempts INTEGER DEFAULT 0,
    pass_completions INTEGER DEFAULT 0,
    completion_pct REAL DEFAULT 0,
    pass_touchdowns INTEGER DEFAULT 0,
    interceptions_thrown INTEGER DEFAULT 0,
    team_passer_rating REAL DEFAULT 0,
    sacks_allowed INTEGER DEFAULT 0,

    -- Rushing Offense
    rush_yards INTEGER DEFAULT 0,
    rush_yards_per_game REAL DEFAULT 0,
    rush_attempts INTEGER DEFAULT 0,
    yards_per_carry REAL DEFAULT 0,
    rush_touchdowns INTEGER DEFAULT 0,

    -- Total Defense
    yards_allowed INTEGER DEFAULT 0,
    yards_allowed_per_game REAL DEFAULT 0,
    pass_yards_allowed INTEGER DEFAULT 0,
    rush_yards_allowed INTEGER DEFAULT 0,

    -- Turnovers
    turnovers INTEGER DEFAULT 0,
    takeaways INTEGER DEFAULT 0,
    turnover_differential INTEGER DEFAULT 0,

    -- Penalties
    penalties INTEGER DEFAULT 0,
    penalty_yards INTEGER DEFAULT 0,

    -- Third/Fourth Down
    third_down_conversions INTEGER DEFAULT 0,
    third_down_attempts INTEGER DEFAULT 0,
    third_down_pct REAL DEFAULT 0,
    fourth_down_conversions INTEGER DEFAULT 0,
    fourth_down_attempts INTEGER DEFAULT 0,
    fourth_down_pct REAL DEFAULT 0,

    -- Red Zone
    red_zone_attempts INTEGER DEFAULT 0,
    red_zone_touchdowns INTEGER DEFAULT 0,
    red_zone_pct REAL DEFAULT 0,

    -- Time of Possession
    avg_time_of_possession TEXT,
    time_of_possession_seconds INTEGER DEFAULT 0,

    -- Rankings
    conference_rank INTEGER,
    division_rank INTEGER,

    -- Metadata
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(team_id, season_id)
);

CREATE INDEX IF NOT EXISTS idx_nfl_team_stats_team ON nfl_team_stats(team_id);
CREATE INDEX IF NOT EXISTS idx_nfl_team_stats_season ON nfl_team_stats(season_id);
