-- Migration 011: Embed Percentiles as JSONB in Stats Tables
--
-- This migration adds JSONB columns to store percentiles directly in stats tables,
-- eliminating the need for a separate percentile_cache table.
--
-- Benefits:
-- - Single endpoint serves stats + percentiles
-- - No JOIN required for percentile data
-- - Flexible schema (add/remove stats without migrations)
-- - Simpler caching (one cache key per entity)

-- ============================================================================
-- NBA PLAYER STATS
-- ============================================================================

ALTER TABLE nba_player_stats 
ADD COLUMN IF NOT EXISTS percentiles JSONB DEFAULT '{}';

ALTER TABLE nba_player_stats 
ADD COLUMN IF NOT EXISTS percentile_position_group TEXT;

ALTER TABLE nba_player_stats 
ADD COLUMN IF NOT EXISTS percentile_sample_size INTEGER;

-- GIN index for JSONB queries (e.g., filtering by percentile threshold)
CREATE INDEX IF NOT EXISTS idx_nba_player_stats_percentiles 
ON nba_player_stats USING GIN (percentiles);

-- ============================================================================
-- NBA TEAM STATS
-- ============================================================================

ALTER TABLE nba_team_stats 
ADD COLUMN IF NOT EXISTS percentiles JSONB DEFAULT '{}';

ALTER TABLE nba_team_stats 
ADD COLUMN IF NOT EXISTS percentile_sample_size INTEGER;

CREATE INDEX IF NOT EXISTS idx_nba_team_stats_percentiles 
ON nba_team_stats USING GIN (percentiles);

-- ============================================================================
-- NFL PLAYER STATS
-- ============================================================================

ALTER TABLE nfl_player_stats 
ADD COLUMN IF NOT EXISTS percentiles JSONB DEFAULT '{}';

ALTER TABLE nfl_player_stats 
ADD COLUMN IF NOT EXISTS percentile_position_group TEXT;

ALTER TABLE nfl_player_stats 
ADD COLUMN IF NOT EXISTS percentile_sample_size INTEGER;

CREATE INDEX IF NOT EXISTS idx_nfl_player_stats_percentiles 
ON nfl_player_stats USING GIN (percentiles);

-- ============================================================================
-- NFL TEAM STATS
-- ============================================================================

ALTER TABLE nfl_team_stats 
ADD COLUMN IF NOT EXISTS percentiles JSONB DEFAULT '{}';

ALTER TABLE nfl_team_stats 
ADD COLUMN IF NOT EXISTS percentile_sample_size INTEGER;

CREATE INDEX IF NOT EXISTS idx_nfl_team_stats_percentiles 
ON nfl_team_stats USING GIN (percentiles);

-- ============================================================================
-- FOOTBALL (SOCCER) PLAYER STATS
-- ============================================================================

ALTER TABLE football_player_stats 
ADD COLUMN IF NOT EXISTS percentiles JSONB DEFAULT '{}';

ALTER TABLE football_player_stats 
ADD COLUMN IF NOT EXISTS percentile_position_group TEXT;

ALTER TABLE football_player_stats 
ADD COLUMN IF NOT EXISTS percentile_sample_size INTEGER;

CREATE INDEX IF NOT EXISTS idx_football_player_stats_percentiles 
ON football_player_stats USING GIN (percentiles);

-- ============================================================================
-- FOOTBALL (SOCCER) TEAM STATS
-- ============================================================================

ALTER TABLE football_team_stats 
ADD COLUMN IF NOT EXISTS percentiles JSONB DEFAULT '{}';

ALTER TABLE football_team_stats 
ADD COLUMN IF NOT EXISTS percentile_sample_size INTEGER;

CREATE INDEX IF NOT EXISTS idx_football_team_stats_percentiles 
ON football_team_stats USING GIN (percentiles);

-- ============================================================================
-- COMMENTS
-- ============================================================================

COMMENT ON COLUMN nba_player_stats.percentiles IS 
    'JSONB object containing percentile rankings for all non-zero stats. Format: {"stat_name": percentile_value}';

COMMENT ON COLUMN nba_player_stats.percentile_position_group IS 
    'Position group used for percentile comparison (e.g., Guard, Forward, Center)';

COMMENT ON COLUMN nba_player_stats.percentile_sample_size IS 
    'Number of players in the comparison group';
