-- Migration 015: Add raw_response JSONB columns
-- 
-- Adds raw_response column to all profile and stats tables to store
-- the complete API response for future field extraction.
-- 
-- This enables:
-- 1. Storing all data points from any provider
-- 2. Extracting new fields without re-fetching data
-- 3. Provider independence (store raw data, map later)

-- ============================================================================
-- Player Profile Tables
-- ============================================================================

-- NBA Player Profiles
ALTER TABLE nba_player_profiles 
    ADD COLUMN IF NOT EXISTS raw_response JSONB;

COMMENT ON COLUMN nba_player_profiles.raw_response IS 
    'Full API response from data provider for future field extraction';

-- NFL Player Profiles
ALTER TABLE nfl_player_profiles 
    ADD COLUMN IF NOT EXISTS raw_response JSONB;

COMMENT ON COLUMN nfl_player_profiles.raw_response IS 
    'Full API response from data provider for future field extraction';

-- Football Player Profiles
ALTER TABLE football_player_profiles 
    ADD COLUMN IF NOT EXISTS raw_response JSONB;

COMMENT ON COLUMN football_player_profiles.raw_response IS 
    'Full API response from data provider for future field extraction';

-- ============================================================================
-- Team Profile Tables
-- ============================================================================

-- NBA Team Profiles
ALTER TABLE nba_team_profiles 
    ADD COLUMN IF NOT EXISTS raw_response JSONB;

COMMENT ON COLUMN nba_team_profiles.raw_response IS 
    'Full API response from data provider for future field extraction';

-- NFL Team Profiles
ALTER TABLE nfl_team_profiles 
    ADD COLUMN IF NOT EXISTS raw_response JSONB;

COMMENT ON COLUMN nfl_team_profiles.raw_response IS 
    'Full API response from data provider for future field extraction';

-- Football Team Profiles
ALTER TABLE football_team_profiles 
    ADD COLUMN IF NOT EXISTS raw_response JSONB;

COMMENT ON COLUMN football_team_profiles.raw_response IS 
    'Full API response from data provider for future field extraction';

-- ============================================================================
-- Player Stats Tables
-- ============================================================================

-- NBA Player Stats
ALTER TABLE nba_player_stats 
    ADD COLUMN IF NOT EXISTS raw_response JSONB;

COMMENT ON COLUMN nba_player_stats.raw_response IS 
    'Full API response (all game logs for NBA) for future field extraction';

-- NFL Player Stats  
ALTER TABLE nfl_player_stats 
    ADD COLUMN IF NOT EXISTS raw_response JSONB;

COMMENT ON COLUMN nfl_player_stats.raw_response IS 
    'Full API response from data provider for future field extraction';

-- Football Player Stats
ALTER TABLE football_player_stats 
    ADD COLUMN IF NOT EXISTS raw_response JSONB;

COMMENT ON COLUMN football_player_stats.raw_response IS 
    'Full API response from data provider for future field extraction';

-- ============================================================================
-- Team Stats Tables
-- ============================================================================

-- NBA Team Stats
ALTER TABLE nba_team_stats 
    ADD COLUMN IF NOT EXISTS raw_response JSONB;

COMMENT ON COLUMN nba_team_stats.raw_response IS 
    'Full API response (standings + team stats) for future field extraction';

-- NFL Team Stats
ALTER TABLE nfl_team_stats 
    ADD COLUMN IF NOT EXISTS raw_response JSONB;

COMMENT ON COLUMN nfl_team_stats.raw_response IS 
    'Full API response from data provider for future field extraction';

-- Football Team Stats
ALTER TABLE football_team_stats 
    ADD COLUMN IF NOT EXISTS raw_response JSONB;

COMMENT ON COLUMN football_team_stats.raw_response IS 
    'Full API response from data provider for future field extraction';

-- ============================================================================
-- Metadata Table
-- ============================================================================

-- Add provider tracking to meta table
INSERT INTO meta (key, value) 
VALUES ('raw_response_migration', '015')
ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value;

-- ============================================================================
-- Optional: GIN Indexes for JSONB queries
-- Uncomment if you need to query raw_response frequently
-- ============================================================================

-- CREATE INDEX IF NOT EXISTS idx_nba_player_profiles_raw_gin 
--     ON nba_player_profiles USING GIN (raw_response);
-- 
-- CREATE INDEX IF NOT EXISTS idx_nba_player_stats_raw_gin 
--     ON nba_player_stats USING GIN (raw_response);

-- ============================================================================
-- Example: Extracting Fields from raw_response
-- ============================================================================

-- Query example: Extract birth country from raw_response
-- SELECT 
--     id,
--     full_name,
--     raw_response->'birth'->>'country' as birth_country_from_raw
-- FROM nba_player_profiles
-- WHERE raw_response IS NOT NULL;

-- Query example: Extract all games from NBA player stats raw_response
-- SELECT 
--     player_id,
--     jsonb_array_length(raw_response->'games') as game_count
-- FROM nba_player_stats
-- WHERE raw_response IS NOT NULL;
