-- Scoracle Stats Database - NFL Unified Player Stats Schema
-- Version: 2.0
-- Created: 2024-12-27
--
-- This migration consolidates the 5 position-specific NFL player tables
-- into a single unified table. Widgets handle position-based display.

-- ============================================================================
-- NFL UNIFIED PLAYER STATISTICS (All Positions)
-- ============================================================================

CREATE TABLE IF NOT EXISTS nfl_player_stats (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    player_id INTEGER NOT NULL REFERENCES players(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    team_id INTEGER REFERENCES teams(id),

    -- Games (common to all positions)
    games_played INTEGER DEFAULT 0,
    games_started INTEGER DEFAULT 0,

    -- ========================================================================
    -- PASSING (QB)
    -- ========================================================================
    pass_attempts INTEGER DEFAULT 0,
    pass_completions INTEGER DEFAULT 0,
    pass_yards INTEGER DEFAULT 0,
    pass_touchdowns INTEGER DEFAULT 0,
    interceptions_thrown INTEGER DEFAULT 0,
    passer_rating REAL DEFAULT 0,
    completion_pct REAL DEFAULT 0,
    yards_per_attempt REAL DEFAULT 0,
    yards_per_completion REAL DEFAULT 0,
    pass_yards_per_game REAL DEFAULT 0,
    td_int_ratio REAL DEFAULT 0,
    qbr REAL DEFAULT 0,
    longest_pass INTEGER DEFAULT 0,
    passes_20_plus INTEGER DEFAULT 0,
    passes_40_plus INTEGER DEFAULT 0,
    sacks_taken INTEGER DEFAULT 0,
    sack_yards_lost INTEGER DEFAULT 0,
    sack_pct REAL DEFAULT 0,
    times_hit INTEGER DEFAULT 0,
    hurries_against INTEGER DEFAULT 0,
    air_yards INTEGER DEFAULT 0,
    air_yards_per_attempt REAL DEFAULT 0,
    completed_air_yards INTEGER DEFAULT 0,
    pass_red_zone_attempts INTEGER DEFAULT 0,
    pass_red_zone_completions INTEGER DEFAULT 0,
    pass_red_zone_touchdowns INTEGER DEFAULT 0,

    -- ========================================================================
    -- RUSHING (RB, QB, WR)
    -- ========================================================================
    rush_attempts INTEGER DEFAULT 0,
    rush_yards INTEGER DEFAULT 0,
    rush_touchdowns INTEGER DEFAULT 0,
    yards_per_carry REAL DEFAULT 0,
    rush_yards_per_game REAL DEFAULT 0,
    longest_rush INTEGER DEFAULT 0,
    rushes_20_plus INTEGER DEFAULT 0,
    rushes_40_plus INTEGER DEFAULT 0,
    rush_first_downs INTEGER DEFAULT 0,
    rush_first_down_pct REAL DEFAULT 0,
    yards_after_contact INTEGER DEFAULT 0,
    yards_after_contact_per_carry REAL DEFAULT 0,
    broken_tackles INTEGER DEFAULT 0,
    rush_fumbles INTEGER DEFAULT 0,
    rush_fumbles_lost INTEGER DEFAULT 0,
    rush_red_zone_attempts INTEGER DEFAULT 0,
    rush_red_zone_touchdowns INTEGER DEFAULT 0,

    -- ========================================================================
    -- RECEIVING (WR, TE, RB)
    -- ========================================================================
    targets INTEGER DEFAULT 0,
    receptions INTEGER DEFAULT 0,
    receiving_yards INTEGER DEFAULT 0,
    receiving_touchdowns INTEGER DEFAULT 0,
    catch_pct REAL DEFAULT 0,
    yards_per_reception REAL DEFAULT 0,
    yards_per_target REAL DEFAULT 0,
    receiving_yards_per_game REAL DEFAULT 0,
    longest_reception INTEGER DEFAULT 0,
    receptions_20_plus INTEGER DEFAULT 0,
    receptions_40_plus INTEGER DEFAULT 0,
    target_share REAL DEFAULT 0,
    yards_after_catch INTEGER DEFAULT 0,
    yards_after_catch_per_reception REAL DEFAULT 0,
    receiving_first_downs INTEGER DEFAULT 0,
    contested_catches INTEGER DEFAULT 0,
    contested_catch_pct REAL DEFAULT 0,
    drops INTEGER DEFAULT 0,
    drop_pct REAL DEFAULT 0,
    rec_fumbles INTEGER DEFAULT 0,
    rec_fumbles_lost INTEGER DEFAULT 0,
    rec_red_zone_targets INTEGER DEFAULT 0,
    rec_red_zone_receptions INTEGER DEFAULT 0,
    rec_red_zone_touchdowns INTEGER DEFAULT 0,

    -- ========================================================================
    -- DEFENSE (LB, DB, DL)
    -- ========================================================================
    tackles_total INTEGER DEFAULT 0,
    tackles_solo INTEGER DEFAULT 0,
    tackles_assist INTEGER DEFAULT 0,
    tackles_for_loss INTEGER DEFAULT 0,
    tackles_for_loss_yards INTEGER DEFAULT 0,
    sacks REAL DEFAULT 0,
    sack_yards INTEGER DEFAULT 0,
    qb_hits INTEGER DEFAULT 0,
    hurries INTEGER DEFAULT 0,
    pressures INTEGER DEFAULT 0,
    pressure_pct REAL DEFAULT 0,
    def_interceptions INTEGER DEFAULT 0,
    int_yards INTEGER DEFAULT 0,
    int_touchdowns INTEGER DEFAULT 0,
    int_longest INTEGER DEFAULT 0,
    passes_defended INTEGER DEFAULT 0,
    forced_fumbles INTEGER DEFAULT 0,
    fumble_recoveries INTEGER DEFAULT 0,
    fumble_recovery_yards INTEGER DEFAULT 0,
    fumble_touchdowns INTEGER DEFAULT 0,
    def_targets INTEGER DEFAULT 0,
    completions_allowed INTEGER DEFAULT 0,
    yards_allowed INTEGER DEFAULT 0,
    touchdowns_allowed INTEGER DEFAULT 0,
    passer_rating_allowed REAL DEFAULT 0,
    safeties INTEGER DEFAULT 0,
    def_snaps INTEGER DEFAULT 0,

    -- ========================================================================
    -- KICKING (K)
    -- ========================================================================
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
    xp_attempts INTEGER DEFAULT 0,
    xp_made INTEGER DEFAULT 0,
    xp_pct REAL DEFAULT 0,
    kicking_points INTEGER DEFAULT 0,

    -- ========================================================================
    -- PUNTING (P)
    -- ========================================================================
    punts INTEGER DEFAULT 0,
    punt_yards INTEGER DEFAULT 0,
    punt_avg REAL DEFAULT 0,
    punt_long INTEGER DEFAULT 0,
    punts_inside_20 INTEGER DEFAULT 0,
    touchbacks INTEGER DEFAULT 0,
    punt_net_avg REAL DEFAULT 0,

    -- ========================================================================
    -- RETURN STATS (KR, PR)
    -- ========================================================================
    kick_returns INTEGER DEFAULT 0,
    kick_return_yards INTEGER DEFAULT 0,
    kick_return_avg REAL DEFAULT 0,
    kick_return_long INTEGER DEFAULT 0,
    kick_return_touchdowns INTEGER DEFAULT 0,
    punt_returns INTEGER DEFAULT 0,
    punt_return_yards INTEGER DEFAULT 0,
    punt_return_avg REAL DEFAULT 0,
    punt_return_long INTEGER DEFAULT 0,
    punt_return_touchdowns INTEGER DEFAULT 0,

    -- Metadata
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(player_id, season_id)
);

CREATE INDEX IF NOT EXISTS idx_nfl_player_stats_player ON nfl_player_stats(player_id);
CREATE INDEX IF NOT EXISTS idx_nfl_player_stats_season ON nfl_player_stats(season_id);
CREATE INDEX IF NOT EXISTS idx_nfl_player_stats_team ON nfl_player_stats(team_id);

-- ============================================================================
-- DROP OLD POSITION-SPECIFIC TABLES
-- ============================================================================

DROP TABLE IF EXISTS nfl_player_passing;
DROP TABLE IF EXISTS nfl_player_rushing;
DROP TABLE IF EXISTS nfl_player_receiving;
DROP TABLE IF EXISTS nfl_player_defense;
DROP TABLE IF EXISTS nfl_player_kicking;
