-- Scoracle Stats Database - Materialized Views for Performance
-- Version: 3.2
-- Created: 2026-01-09
-- Purpose: Pre-computed views for common aggregations and lookups
--
-- These views should be refreshed daily (after data sync) using:
-- REFRESH MATERIALIZED VIEW CONCURRENTLY <view_name>;

-- ============================================================================
-- NBA MATERIALIZED VIEWS
-- ============================================================================

-- NBA Player Leaderboard View - Pre-computed rankings for all stat categories
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_nba_player_leaderboard AS
SELECT
    ps.player_id,
    ps.season_id,
    ps.team_id,
    p.full_name,
    p.position,
    p.position_group,
    t.name as team_name,
    t.abbreviation as team_abbrev,
    p.photo_url,
    s.season_year,
    ps.games_played,
    ps.points_per_game,
    ps.rebounds_per_game,
    ps.assists_per_game,
    ps.steals_per_game,
    ps.blocks_per_game,
    ps.fg_pct,
    ps.tp_pct,
    ps.ft_pct,
    ps.efficiency,
    -- Pre-computed ranks
    RANK() OVER (PARTITION BY ps.season_id ORDER BY ps.points_per_game DESC) as ppg_rank,
    RANK() OVER (PARTITION BY ps.season_id ORDER BY ps.rebounds_per_game DESC) as rpg_rank,
    RANK() OVER (PARTITION BY ps.season_id ORDER BY ps.assists_per_game DESC) as apg_rank,
    RANK() OVER (PARTITION BY ps.season_id ORDER BY ps.steals_per_game DESC) as spg_rank,
    RANK() OVER (PARTITION BY ps.season_id ORDER BY ps.blocks_per_game DESC) as bpg_rank,
    RANK() OVER (PARTITION BY ps.season_id ORDER BY ps.efficiency DESC) as eff_rank
FROM nba_player_stats ps
JOIN players p ON ps.player_id = p.id
JOIN teams t ON ps.team_id = t.id
JOIN seasons s ON ps.season_id = s.id
WHERE ps.games_played >= 10;  -- Minimum games threshold

CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_nba_player_lb_pk
    ON mv_nba_player_leaderboard(player_id, season_id);
CREATE INDEX IF NOT EXISTS idx_mv_nba_player_lb_season
    ON mv_nba_player_leaderboard(season_id);
CREATE INDEX IF NOT EXISTS idx_mv_nba_player_lb_ppg
    ON mv_nba_player_leaderboard(season_id, ppg_rank);

-- NBA Team Standings View
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_nba_team_standings AS
SELECT
    ts.team_id,
    ts.season_id,
    t.name,
    t.abbreviation,
    t.logo_url,
    t.conference,
    t.division,
    s.season_year,
    ts.wins,
    ts.losses,
    ts.win_pct,
    ts.points_per_game,
    ts.opponent_ppg,
    ts.point_differential,
    ts.conference_rank,
    ts.division_rank,
    ts.league_rank,
    ts.current_streak,
    ts.streak_type,
    -- Pre-computed overall rank
    RANK() OVER (PARTITION BY ts.season_id ORDER BY ts.win_pct DESC, ts.point_differential DESC) as overall_rank,
    RANK() OVER (PARTITION BY ts.season_id, t.conference ORDER BY ts.win_pct DESC) as conf_rank_calc
FROM nba_team_stats ts
JOIN teams t ON ts.team_id = t.id
JOIN seasons s ON ts.season_id = s.id;

CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_nba_team_standings_pk
    ON mv_nba_team_standings(team_id, season_id);
CREATE INDEX IF NOT EXISTS idx_mv_nba_team_standings_rank
    ON mv_nba_team_standings(season_id, overall_rank);

-- ============================================================================
-- NFL MATERIALIZED VIEWS
-- ============================================================================

-- NFL Passing Leaders
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_nfl_passing_leaders AS
SELECT
    ps.player_id,
    ps.season_id,
    ps.team_id,
    p.full_name,
    p.position,
    t.name as team_name,
    t.abbreviation as team_abbrev,
    p.photo_url,
    s.season_year,
    ps.games_played,
    ps.pass_yards,
    ps.pass_touchdowns,
    ps.interceptions_thrown,
    ps.passer_rating,
    ps.completion_pct,
    ps.yards_per_attempt,
    RANK() OVER (PARTITION BY ps.season_id ORDER BY ps.pass_yards DESC) as yards_rank,
    RANK() OVER (PARTITION BY ps.season_id ORDER BY ps.pass_touchdowns DESC) as td_rank,
    RANK() OVER (PARTITION BY ps.season_id ORDER BY ps.passer_rating DESC) as rating_rank
FROM nfl_player_stats ps
JOIN players p ON ps.player_id = p.id
JOIN teams t ON ps.team_id = t.id
JOIN seasons s ON ps.season_id = s.id
WHERE ps.pass_attempts >= 100;

CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_nfl_passing_pk
    ON mv_nfl_passing_leaders(player_id, season_id);

-- NFL Rushing Leaders
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_nfl_rushing_leaders AS
SELECT
    ps.player_id,
    ps.season_id,
    ps.team_id,
    p.full_name,
    p.position,
    t.name as team_name,
    t.abbreviation as team_abbrev,
    p.photo_url,
    s.season_year,
    ps.games_played,
    ps.rush_yards,
    ps.rush_touchdowns,
    ps.yards_per_carry,
    ps.rush_attempts,
    RANK() OVER (PARTITION BY ps.season_id ORDER BY ps.rush_yards DESC) as yards_rank,
    RANK() OVER (PARTITION BY ps.season_id ORDER BY ps.rush_touchdowns DESC) as td_rank
FROM nfl_player_stats ps
JOIN players p ON ps.player_id = p.id
JOIN teams t ON ps.team_id = t.id
JOIN seasons s ON ps.season_id = s.id
WHERE ps.rush_attempts >= 50;

CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_nfl_rushing_pk
    ON mv_nfl_rushing_leaders(player_id, season_id);

-- NFL Receiving Leaders
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_nfl_receiving_leaders AS
SELECT
    ps.player_id,
    ps.season_id,
    ps.team_id,
    p.full_name,
    p.position,
    t.name as team_name,
    t.abbreviation as team_abbrev,
    p.photo_url,
    s.season_year,
    ps.games_played,
    ps.receiving_yards,
    ps.receiving_touchdowns,
    ps.receptions,
    ps.targets,
    ps.yards_per_reception,
    RANK() OVER (PARTITION BY ps.season_id ORDER BY ps.receiving_yards DESC) as yards_rank,
    RANK() OVER (PARTITION BY ps.season_id ORDER BY ps.receiving_touchdowns DESC) as td_rank,
    RANK() OVER (PARTITION BY ps.season_id ORDER BY ps.receptions DESC) as rec_rank
FROM nfl_player_stats ps
JOIN players p ON ps.player_id = p.id
JOIN teams t ON ps.team_id = t.id
JOIN seasons s ON ps.season_id = s.id
WHERE ps.targets >= 30;

CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_nfl_receiving_pk
    ON mv_nfl_receiving_leaders(player_id, season_id);

-- NFL Team Standings
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_nfl_team_standings AS
SELECT
    ts.team_id,
    ts.season_id,
    t.name,
    t.abbreviation,
    t.logo_url,
    t.conference,
    t.division,
    s.season_year,
    ts.wins,
    ts.losses,
    ts.ties,
    ts.win_pct,
    ts.points_for,
    ts.points_against,
    ts.point_differential,
    ts.conference_rank,
    ts.division_rank,
    RANK() OVER (PARTITION BY ts.season_id ORDER BY ts.win_pct DESC, ts.point_differential DESC) as overall_rank,
    RANK() OVER (PARTITION BY ts.season_id, t.conference ORDER BY ts.win_pct DESC) as conf_rank_calc,
    RANK() OVER (PARTITION BY ts.season_id, t.division ORDER BY ts.win_pct DESC) as div_rank_calc
FROM nfl_team_stats ts
JOIN teams t ON ts.team_id = t.id
JOIN seasons s ON ts.season_id = s.id;

CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_nfl_team_standings_pk
    ON mv_nfl_team_standings(team_id, season_id);

-- ============================================================================
-- FOOTBALL (SOCCER) MATERIALIZED VIEWS
-- ============================================================================

-- Football Top Scorers
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_football_top_scorers AS
SELECT
    ps.player_id,
    ps.season_id,
    ps.league_id,
    ps.team_id,
    p.full_name,
    p.position,
    t.name as team_name,
    l.name as league_name,
    p.photo_url,
    s.season_year,
    ps.appearances,
    ps.goals,
    ps.assists,
    ps.goals_assists,
    ps.goals_per_90,
    ps.minutes_played,
    RANK() OVER (PARTITION BY ps.season_id, ps.league_id ORDER BY ps.goals DESC) as goals_rank,
    RANK() OVER (PARTITION BY ps.season_id, ps.league_id ORDER BY ps.assists DESC) as assists_rank,
    RANK() OVER (PARTITION BY ps.season_id, ps.league_id ORDER BY ps.goals_assists DESC) as ga_rank
FROM football_player_stats ps
JOIN players p ON ps.player_id = p.id
JOIN teams t ON ps.team_id = t.id
JOIN leagues l ON ps.league_id = l.id
JOIN seasons s ON ps.season_id = s.id
WHERE ps.appearances >= 5;

CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_football_scorers_pk
    ON mv_football_top_scorers(player_id, season_id, league_id);
CREATE INDEX IF NOT EXISTS idx_mv_football_scorers_league
    ON mv_football_top_scorers(league_id, season_id, goals_rank);

-- Football League Standings
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_football_standings AS
SELECT
    ts.team_id,
    ts.season_id,
    ts.league_id,
    t.name,
    t.abbreviation,
    t.logo_url,
    l.name as league_name,
    l.country as league_country,
    s.season_year,
    ts.matches_played,
    ts.wins,
    ts.draws,
    ts.losses,
    ts.points,
    ts.goals_for,
    ts.goals_against,
    ts.goal_difference,
    ts.league_position,
    ts.form,
    ts.avg_possession,
    ts.expected_goals,
    ts.expected_goals_against
FROM football_team_stats ts
JOIN teams t ON ts.team_id = t.id
JOIN leagues l ON ts.league_id = l.id
JOIN seasons s ON ts.season_id = s.id;

CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_football_standings_pk
    ON mv_football_standings(team_id, season_id, league_id);
CREATE INDEX IF NOT EXISTS idx_mv_football_standings_league
    ON mv_football_standings(league_id, season_id, league_position);

-- ============================================================================
-- ENTITY PROFILE VIEW (Pre-joined player/team data for fast lookups)
-- ============================================================================

-- Player Profile View - combines player + team info
CREATE MATERIALIZED VIEW IF NOT EXISTS mv_player_profiles AS
SELECT
    p.id as player_id,
    p.sport_id,
    p.first_name,
    p.last_name,
    p.full_name,
    p.position,
    p.position_group,
    p.nationality,
    p.birth_date,
    p.height_inches,
    p.weight_lbs,
    p.photo_url,
    p.current_team_id,
    p.jersey_number,
    p.college,
    p.experience_years,
    p.is_active,
    t.name as team_name,
    t.abbreviation as team_abbrev,
    t.logo_url as team_logo,
    t.conference as team_conference,
    t.division as team_division
FROM players p
LEFT JOIN teams t ON p.current_team_id = t.id AND t.sport_id = p.sport_id
WHERE p.is_active = true;

CREATE UNIQUE INDEX IF NOT EXISTS idx_mv_player_profiles_pk
    ON mv_player_profiles(player_id);
CREATE INDEX IF NOT EXISTS idx_mv_player_profiles_sport
    ON mv_player_profiles(sport_id);
CREATE INDEX IF NOT EXISTS idx_mv_player_profiles_team
    ON mv_player_profiles(current_team_id);

-- ============================================================================
-- REFRESH FUNCTION
-- ============================================================================

-- Function to refresh all materialized views (call after daily sync)
CREATE OR REPLACE FUNCTION refresh_all_materialized_views()
RETURNS void AS $$
BEGIN
    -- NBA views
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_nba_player_leaderboard;
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_nba_team_standings;

    -- NFL views
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_nfl_passing_leaders;
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_nfl_rushing_leaders;
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_nfl_receiving_leaders;
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_nfl_team_standings;

    -- Football views
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_football_top_scorers;
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_football_standings;

    -- Entity profiles
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_player_profiles;

    RAISE NOTICE 'All materialized views refreshed at %', NOW();
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- INITIAL REFRESH (run after creating views)
-- ============================================================================

-- Note: CONCURRENTLY requires unique indexes, which we've created above
-- For first run, use non-concurrent refresh:
-- REFRESH MATERIALIZED VIEW mv_nba_player_leaderboard;
-- REFRESH MATERIALIZED VIEW mv_nba_team_standings;
-- etc.
