-- Unified Views for backward compatibility
-- Version: 4.1
-- Created: 2026-01-19
-- Purpose: Replace legacy players/teams tables with views that aggregate sport-specific tables
--
-- This migration:
-- 1. Drops the empty legacy tables (players, teams, player_teams)
-- 2. Creates views with the same names that union sport-specific profile tables
-- 3. All existing code continues to work without modification
--
-- IMPORTANT: Only run this if the legacy tables are empty or contain stale data.
-- The sport-specific tables (nba_player_profiles, etc.) should be the source of truth.

-- ============================================================================
-- STEP 1: Drop legacy tables
-- ============================================================================
-- These tables are replaced by sport-specific profile tables.
-- Data should already be in nba_player_profiles, nfl_player_profiles, etc.

DROP TABLE IF EXISTS player_teams CASCADE;
DROP TABLE IF EXISTS players CASCADE;
DROP TABLE IF EXISTS teams CASCADE;


-- ============================================================================
-- STEP 2: Create unified views with legacy names
-- ============================================================================

-- Players view - aggregates all sport-specific player profile tables
CREATE OR REPLACE VIEW players AS
SELECT 
    id,
    'NBA' as sport_id,
    first_name,
    last_name,
    full_name,
    full_name as name,  -- Legacy compatibility alias
    position,
    position_group,
    nationality,
    birth_date,
    birth_place,
    birth_country,
    height_inches,
    weight_lbs,
    photo_url,
    current_team_id,
    NULL::INTEGER as current_league_id,
    jersey_number,
    college,
    experience_years,
    is_active,
    profile_fetched_at,
    updated_at
FROM nba_player_profiles

UNION ALL

SELECT 
    id,
    'NFL' as sport_id,
    first_name,
    last_name,
    full_name,
    full_name as name,
    position,
    position_group,
    nationality,
    birth_date,
    birth_place,
    birth_country,
    height_inches,
    weight_lbs,
    photo_url,
    current_team_id,
    NULL::INTEGER as current_league_id,
    jersey_number,
    college,
    experience_years,
    is_active,
    profile_fetched_at,
    updated_at
FROM nfl_player_profiles

UNION ALL

SELECT 
    id,
    'FOOTBALL' as sport_id,
    first_name,
    last_name,
    full_name,
    full_name as name,
    position,
    position_group,
    nationality,
    birth_date,
    birth_place,
    birth_country,
    height_inches,
    weight_lbs,
    photo_url,
    current_team_id,
    current_league_id,
    jersey_number,
    NULL as college,
    NULL as experience_years,
    is_active,
    profile_fetched_at,
    updated_at
FROM football_player_profiles;


-- Teams view - aggregates all sport-specific team profile tables
CREATE OR REPLACE VIEW teams AS
SELECT 
    id,
    'NBA' as sport_id,
    NULL::INTEGER as league_id,
    name,
    abbreviation,
    conference,
    division,
    city,
    country,
    logo_url,
    founded,
    NULL::BOOLEAN as is_national,
    venue_name,
    venue_address,
    venue_city,
    venue_capacity,
    venue_surface,
    venue_image,
    is_active,
    profile_fetched_at,
    updated_at
FROM nba_team_profiles

UNION ALL

SELECT 
    id,
    'NFL' as sport_id,
    NULL::INTEGER as league_id,
    name,
    abbreviation,
    conference,
    division,
    city,
    country,
    logo_url,
    founded,
    NULL::BOOLEAN as is_national,
    venue_name,
    venue_address,
    venue_city,
    venue_capacity,
    venue_surface,
    venue_image,
    is_active,
    profile_fetched_at,
    updated_at
FROM nfl_team_profiles

UNION ALL

SELECT 
    id,
    'FOOTBALL' as sport_id,
    league_id,
    name,
    abbreviation,
    NULL as conference,
    NULL as division,
    city,
    country,
    logo_url,
    founded,
    is_national,
    venue_name,
    venue_address,
    venue_city,
    venue_capacity,
    venue_surface,
    venue_image,
    is_active,
    profile_fetched_at,
    updated_at
FROM football_team_profiles;


-- ============================================================================
-- STEP 3: Create indexes on the underlying tables for view performance
-- ============================================================================
-- Views don't have indexes, but the underlying tables should be indexed.
-- These indexes improve performance when querying the views.

CREATE INDEX IF NOT EXISTS idx_nba_player_profiles_sport_active 
    ON nba_player_profiles(is_active) WHERE is_active = true;
CREATE INDEX IF NOT EXISTS idx_nfl_player_profiles_sport_active 
    ON nfl_player_profiles(is_active) WHERE is_active = true;
CREATE INDEX IF NOT EXISTS idx_football_player_profiles_sport_active 
    ON football_player_profiles(is_active) WHERE is_active = true;

CREATE INDEX IF NOT EXISTS idx_nba_team_profiles_sport_active 
    ON nba_team_profiles(is_active) WHERE is_active = true;
CREATE INDEX IF NOT EXISTS idx_nfl_team_profiles_sport_active 
    ON nfl_team_profiles(is_active) WHERE is_active = true;
CREATE INDEX IF NOT EXISTS idx_football_team_profiles_sport_active 
    ON football_team_profiles(is_active) WHERE is_active = true;


-- ============================================================================
-- STEP 4: Update metadata
-- ============================================================================
UPDATE meta SET value = '4.1', updated_at = NOW() WHERE key = 'schema_version';
