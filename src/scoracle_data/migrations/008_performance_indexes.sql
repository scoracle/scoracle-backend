-- Scoracle Stats Database - Performance Optimization Indexes
-- Version: 3.1
-- Created: 2026-01-09
-- Purpose: Add composite indexes for optimized query patterns

-- ============================================================================
-- COMPOSITE INDEXES FOR API QUERY PATTERNS
-- ============================================================================

-- Player stats lookup: player_id + season_id is the most common query pattern
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_nba_player_stats_player_season
    ON nba_player_stats(player_id, season_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_nfl_player_stats_player_season
    ON nfl_player_stats(player_id, season_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_football_player_stats_player_season
    ON football_player_stats(player_id, season_id);

-- Team stats lookup: team_id + season_id
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_nba_team_stats_team_season
    ON nba_team_stats(team_id, season_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_nfl_team_stats_team_season
    ON nfl_team_stats(team_id, season_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_football_team_stats_team_season
    ON football_team_stats(team_id, season_id, league_id);

-- Season lookup: sport_id + season_year (critical for get_season_id calls)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_seasons_sport_year
    ON seasons(sport_id, season_year);

-- Percentile cache: composite for entity lookups
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_percentile_cache_entity_lookup
    ON percentile_cache(entity_type, entity_id, sport_id, season_id);

-- Players by sport and ID (for get_player queries)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_players_sport_id
    ON players(id, sport_id);

-- Teams by sport and ID (for get_team queries)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_teams_sport_id
    ON teams(id, sport_id);

-- ============================================================================
-- COVERING INDEXES (Include columns to avoid table lookups)
-- ============================================================================

-- Seasons covering index - includes all columns needed for season lookups
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_seasons_covering
    ON seasons(sport_id, season_year)
    INCLUDE (id, is_current);

-- ============================================================================
-- PARTIAL INDEXES (For common filtered queries)
-- ============================================================================

-- Active players only (most API queries filter by active status)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_players_active
    ON players(sport_id, current_team_id)
    WHERE is_active = true;

-- Active teams only
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_teams_active
    ON teams(sport_id, league_id)
    WHERE is_active = true;

-- Current season index (avoids scanning all seasons)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_seasons_current_sport
    ON seasons(sport_id)
    WHERE is_current = true;

-- ============================================================================
-- STATISTICS FOR QUERY OPTIMIZER
-- ============================================================================

-- Analyze tables to update statistics for query planner
ANALYZE players;
ANALYZE teams;
ANALYZE seasons;
ANALYZE nba_player_stats;
ANALYZE nba_team_stats;
ANALYZE nfl_player_stats;
ANALYZE nfl_team_stats;
ANALYZE football_player_stats;
ANALYZE football_team_stats;
ANALYZE percentile_cache;
