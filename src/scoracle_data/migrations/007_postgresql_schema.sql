-- Scoracle Stats Database - PostgreSQL/Neon Schema
-- Version: 3.0
-- Created: 2024-12-30
-- Purpose: Consolidated PostgreSQL schema for Neon migration
--
-- This file combines all SQLite migrations (001-006) into a single
-- PostgreSQL-compatible schema with native features.

-- ============================================================================
-- CORE TABLES
-- ============================================================================

-- Metadata table for tracking updates and migrations
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Initialize metadata
INSERT INTO meta (key, value) VALUES
    ('schema_version', '3.0'),
    ('last_full_sync', ''),
    ('last_incremental_sync', '')
ON CONFLICT (key) DO NOTHING;

-- Sports registry
CREATE TABLE IF NOT EXISTS sports (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    api_base_url TEXT NOT NULL,
    current_season INTEGER NOT NULL,
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Insert supported sports
INSERT INTO sports (id, display_name, api_base_url, current_season) VALUES
    ('NBA', 'NBA Basketball', 'https://v2.nba.api-sports.io', 2025),
    ('NFL', 'NFL Football', 'https://v1.american-football.api-sports.io', 2025),
    ('FOOTBALL', 'Football (Soccer)', 'https://v3.football.api-sports.io', 2024)
ON CONFLICT (id) DO NOTHING;

-- Seasons registry
CREATE TABLE IF NOT EXISTS seasons (
    id SERIAL PRIMARY KEY,
    sport_id TEXT NOT NULL REFERENCES sports(id),
    season_year INTEGER NOT NULL,
    season_label TEXT,
    is_current BOOLEAN DEFAULT false,
    start_date DATE,
    end_date DATE,
    games_played INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(sport_id, season_year)
);

CREATE INDEX IF NOT EXISTS idx_seasons_sport ON seasons(sport_id);
CREATE INDEX IF NOT EXISTS idx_seasons_current ON seasons(sport_id, is_current) WHERE is_current = true;

-- Leagues registry (for multi-league sports like Football)
CREATE TABLE IF NOT EXISTS leagues (
    id INTEGER PRIMARY KEY,
    sport_id TEXT NOT NULL REFERENCES sports(id),
    name TEXT NOT NULL,
    country TEXT,
    country_code TEXT,
    logo_url TEXT,
    priority_tier INTEGER DEFAULT 0,
    include_in_percentiles BOOLEAN DEFAULT false,
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_leagues_sport ON leagues(sport_id);
CREATE INDEX IF NOT EXISTS idx_leagues_country ON leagues(country);
CREATE INDEX IF NOT EXISTS idx_leagues_priority ON leagues(priority_tier);

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
    venue_city TEXT,
    venue_surface TEXT,
    venue_image TEXT,
    profile_fetched_at TIMESTAMPTZ,
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_teams_sport ON teams(sport_id);
CREATE INDEX IF NOT EXISTS idx_teams_league ON teams(league_id);
CREATE INDEX IF NOT EXISTS idx_teams_name ON teams(name);
CREATE INDEX IF NOT EXISTS idx_teams_conference ON teams(sport_id, conference);
CREATE INDEX IF NOT EXISTS idx_teams_needs_profile ON teams(sport_id) WHERE profile_fetched_at IS NULL;

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
    birth_date DATE,
    birth_place TEXT,
    height_inches INTEGER,
    weight_lbs INTEGER,
    photo_url TEXT,
    current_team_id INTEGER REFERENCES teams(id),
    current_league_id INTEGER REFERENCES leagues(id),
    jersey_number INTEGER,
    college TEXT,
    experience_years INTEGER,
    profile_fetched_at TIMESTAMPTZ,
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_players_sport ON players(sport_id);
CREATE INDEX IF NOT EXISTS idx_players_team ON players(current_team_id);
CREATE INDEX IF NOT EXISTS idx_players_name ON players(full_name);
CREATE INDEX IF NOT EXISTS idx_players_position ON players(sport_id, position);
CREATE INDEX IF NOT EXISTS idx_players_position_group ON players(sport_id, position_group);
CREATE INDEX IF NOT EXISTS idx_players_needs_profile ON players(sport_id) WHERE profile_fetched_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_players_current_league ON players(current_league_id);

-- Player-Team history (for trades/transfers)
CREATE TABLE IF NOT EXISTS player_teams (
    id SERIAL PRIMARY KEY,
    player_id INTEGER NOT NULL REFERENCES players(id),
    team_id INTEGER NOT NULL REFERENCES teams(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    start_date DATE,
    end_date DATE,
    is_current BOOLEAN DEFAULT false,
    detected_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(player_id, team_id, season_id)
);

CREATE INDEX IF NOT EXISTS idx_player_teams_player ON player_teams(player_id);
CREATE INDEX IF NOT EXISTS idx_player_teams_team ON player_teams(team_id);
CREATE INDEX IF NOT EXISTS idx_player_teams_season ON player_teams(season_id);

-- ============================================================================
-- NBA STATISTICS TABLES
-- ============================================================================

CREATE TABLE IF NOT EXISTS nba_player_stats (
    id SERIAL PRIMARY KEY,
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

    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(player_id, season_id, team_id)
);

CREATE INDEX IF NOT EXISTS idx_nba_player_stats_player ON nba_player_stats(player_id);
CREATE INDEX IF NOT EXISTS idx_nba_player_stats_season ON nba_player_stats(season_id);
CREATE INDEX IF NOT EXISTS idx_nba_player_stats_team ON nba_player_stats(team_id);
CREATE INDEX IF NOT EXISTS idx_nba_player_stats_ppg ON nba_player_stats(season_id, points_per_game DESC);

CREATE TABLE IF NOT EXISTS nba_team_stats (
    id SERIAL PRIMARY KEY,
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

    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(team_id, season_id)
);

CREATE INDEX IF NOT EXISTS idx_nba_team_stats_team ON nba_team_stats(team_id);
CREATE INDEX IF NOT EXISTS idx_nba_team_stats_season ON nba_team_stats(season_id);
CREATE INDEX IF NOT EXISTS idx_nba_team_stats_wins ON nba_team_stats(season_id, win_pct DESC);

-- ============================================================================
-- NFL STATISTICS TABLES (Unified)
-- ============================================================================

CREATE TABLE IF NOT EXISTS nfl_player_stats (
    id SERIAL PRIMARY KEY,
    player_id INTEGER NOT NULL REFERENCES players(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    team_id INTEGER REFERENCES teams(id),

    -- Games
    games_played INTEGER DEFAULT 0,
    games_started INTEGER DEFAULT 0,

    -- PASSING (QB)
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

    -- RUSHING (RB, QB, WR)
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

    -- RECEIVING (WR, TE, RB)
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

    -- DEFENSE (LB, DB, DL)
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

    -- KICKING (K)
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

    -- PUNTING (P)
    punts INTEGER DEFAULT 0,
    punt_yards INTEGER DEFAULT 0,
    punt_avg REAL DEFAULT 0,
    punt_long INTEGER DEFAULT 0,
    punts_inside_20 INTEGER DEFAULT 0,
    touchbacks INTEGER DEFAULT 0,
    punt_net_avg REAL DEFAULT 0,

    -- RETURN STATS (KR, PR)
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

    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(player_id, season_id)
);

CREATE INDEX IF NOT EXISTS idx_nfl_player_stats_player ON nfl_player_stats(player_id);
CREATE INDEX IF NOT EXISTS idx_nfl_player_stats_season ON nfl_player_stats(season_id);
CREATE INDEX IF NOT EXISTS idx_nfl_player_stats_team ON nfl_player_stats(team_id);

CREATE TABLE IF NOT EXISTS nfl_team_stats (
    id SERIAL PRIMARY KEY,
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

    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(team_id, season_id)
);

CREATE INDEX IF NOT EXISTS idx_nfl_team_stats_team ON nfl_team_stats(team_id);
CREATE INDEX IF NOT EXISTS idx_nfl_team_stats_season ON nfl_team_stats(season_id);

-- ============================================================================
-- FOOTBALL (SOCCER) STATISTICS TABLES
-- ============================================================================

CREATE TABLE IF NOT EXISTS football_player_stats (
    id SERIAL PRIMARY KEY,
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

    -- Expected Goals
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

    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(player_id, season_id, league_id)
);

CREATE INDEX IF NOT EXISTS idx_football_player_stats_player ON football_player_stats(player_id);
CREATE INDEX IF NOT EXISTS idx_football_player_stats_season ON football_player_stats(season_id);
CREATE INDEX IF NOT EXISTS idx_football_player_stats_league ON football_player_stats(league_id);
CREATE INDEX IF NOT EXISTS idx_football_player_stats_team ON football_player_stats(team_id);
CREATE INDEX IF NOT EXISTS idx_football_player_stats_goals ON football_player_stats(season_id, league_id, goals DESC);

CREATE TABLE IF NOT EXISTS football_team_stats (
    id SERIAL PRIMARY KEY,
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

    -- Form
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

    -- Expected Goals
    expected_goals REAL DEFAULT 0,
    expected_goals_against REAL DEFAULT 0,
    expected_goal_difference REAL DEFAULT 0,

    -- Standings
    league_position INTEGER,
    points_per_game REAL DEFAULT 0,

    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(team_id, season_id, league_id)
);

CREATE INDEX IF NOT EXISTS idx_football_team_stats_team ON football_team_stats(team_id);
CREATE INDEX IF NOT EXISTS idx_football_team_stats_season ON football_team_stats(season_id);
CREATE INDEX IF NOT EXISTS idx_football_team_stats_league ON football_team_stats(league_id);
CREATE INDEX IF NOT EXISTS idx_football_team_stats_position ON football_team_stats(season_id, league_id, league_position);

-- Football standings snapshot
CREATE TABLE IF NOT EXISTS football_standings_snapshot (
    id SERIAL PRIMARY KEY,
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
    snapshot_date DATE NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(team_id, season_id, league_id, matchday)
);

CREATE INDEX IF NOT EXISTS idx_standings_snapshot ON football_standings_snapshot(league_id, season_id, matchday);

-- ============================================================================
-- PERCENTILE CACHE TABLE
-- ============================================================================

CREATE TABLE IF NOT EXISTS percentile_cache (
    id SERIAL PRIMARY KEY,
    entity_type TEXT NOT NULL CHECK(entity_type IN ('player', 'team')),
    entity_id INTEGER NOT NULL,
    sport_id TEXT NOT NULL REFERENCES sports(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    stat_category TEXT NOT NULL,
    stat_value REAL,
    percentile REAL CHECK(percentile >= 0 AND percentile <= 100),
    rank INTEGER,
    sample_size INTEGER,
    comparison_group TEXT,
    calculated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(entity_type, entity_id, sport_id, season_id, stat_category)
);

CREATE INDEX IF NOT EXISTS idx_percentile_entity ON percentile_cache(entity_type, entity_id);
CREATE INDEX IF NOT EXISTS idx_percentile_lookup ON percentile_cache(entity_type, sport_id, season_id, stat_category);
CREATE INDEX IF NOT EXISTS idx_percentile_ranking ON percentile_cache(sport_id, season_id, stat_category, percentile DESC);

-- ============================================================================
-- SYNC TRACKING TABLE
-- ============================================================================

CREATE TABLE IF NOT EXISTS sync_log (
    id SERIAL PRIMARY KEY,
    sport_id TEXT NOT NULL REFERENCES sports(id),
    league_id INTEGER REFERENCES leagues(id),
    sync_type TEXT NOT NULL CHECK(sync_type IN ('full', 'incremental', 'percentile')),
    entity_type TEXT,
    season_id INTEGER REFERENCES seasons(id),
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    records_processed INTEGER DEFAULT 0,
    records_inserted INTEGER DEFAULT 0,
    records_updated INTEGER DEFAULT 0,
    status TEXT NOT NULL CHECK(status IN ('running', 'completed', 'failed')),
    error_message TEXT
);

CREATE INDEX IF NOT EXISTS idx_sync_log_sport ON sync_log(sport_id, sync_type);
CREATE INDEX IF NOT EXISTS idx_sync_log_status ON sync_log(status);
CREATE INDEX IF NOT EXISTS idx_sync_log_started ON sync_log(started_at DESC);

-- ============================================================================
-- ENTITIES MINIMAL (for autocomplete)
-- ============================================================================

CREATE TABLE IF NOT EXISTS entities_minimal (
    id SERIAL PRIMARY KEY,
    entity_type TEXT NOT NULL CHECK(entity_type IN ('team', 'player')),
    entity_id INTEGER NOT NULL,
    sport_id TEXT NOT NULL REFERENCES sports(id),
    league_id INTEGER REFERENCES leagues(id),
    name TEXT NOT NULL,
    normalized_name TEXT,
    tokens TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(entity_type, entity_id)
);

CREATE INDEX IF NOT EXISTS idx_entities_minimal_type ON entities_minimal(entity_type, sport_id);
CREATE INDEX IF NOT EXISTS idx_entities_minimal_league ON entities_minimal(league_id);
CREATE INDEX IF NOT EXISTS idx_entities_minimal_search ON entities_minimal(normalized_name);
-- Full-text search index for tokens
CREATE INDEX IF NOT EXISTS idx_entities_minimal_tokens ON entities_minimal USING gin(to_tsvector('english', COALESCE(tokens, '')));
