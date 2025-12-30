-- Scoracle Stats Database - Profile Tracking & Tiered Coverage
-- Version: 2.1
-- Created: 2024-12-27
-- Purpose: Add two-phase seeding support, priority tiers, and percentile exclusion

-- ============================================================================
-- LEAGUES TABLE UPDATES
-- ============================================================================

-- Add priority_tier: 1 = full data, 0 = minimal data
ALTER TABLE leagues ADD COLUMN priority_tier INTEGER DEFAULT 0;

-- Add include_in_percentiles: 1 = used in percentile calculations, 0 = excluded
-- This is separate from priority_tier (e.g., MLS has priority_tier=1 but include_in_percentiles=0)
ALTER TABLE leagues ADD COLUMN include_in_percentiles INTEGER DEFAULT 0;

-- Set priority leagues for Football (Top 5 European + MLS)
UPDATE leagues SET priority_tier = 1, include_in_percentiles = 1 WHERE id = 39;   -- Premier League
UPDATE leagues SET priority_tier = 1, include_in_percentiles = 1 WHERE id = 140;  -- La Liga
UPDATE leagues SET priority_tier = 1, include_in_percentiles = 1 WHERE id = 78;   -- Bundesliga
UPDATE leagues SET priority_tier = 1, include_in_percentiles = 1 WHERE id = 135;  -- Serie A
UPDATE leagues SET priority_tier = 1, include_in_percentiles = 1 WHERE id = 61;   -- Ligue 1
UPDATE leagues SET priority_tier = 1, include_in_percentiles = 0 WHERE id = 253;  -- MLS (full data, no percentiles)

-- ============================================================================
-- TEAMS TABLE UPDATES
-- ============================================================================

-- Add profile_fetched_at: NULL = needs fetch, timestamp = fetched
ALTER TABLE teams ADD COLUMN profile_fetched_at INTEGER;

-- Add additional venue fields
ALTER TABLE teams ADD COLUMN venue_city TEXT;
ALTER TABLE teams ADD COLUMN venue_surface TEXT;
ALTER TABLE teams ADD COLUMN venue_image TEXT;

-- Create index for finding teams needing profile fetch
CREATE INDEX IF NOT EXISTS idx_teams_needs_profile ON teams(profile_fetched_at) WHERE profile_fetched_at IS NULL;

-- ============================================================================
-- PLAYERS TABLE UPDATES
-- ============================================================================

-- Add profile_fetched_at: NULL = needs fetch, timestamp = fetched
ALTER TABLE players ADD COLUMN profile_fetched_at INTEGER;

-- Add current_league_id for Football players (needed for percentile filtering)
ALTER TABLE players ADD COLUMN current_league_id INTEGER REFERENCES leagues(id);

-- Create index for finding players needing profile fetch
CREATE INDEX IF NOT EXISTS idx_players_needs_profile ON players(profile_fetched_at) WHERE profile_fetched_at IS NULL;

-- Create index for filtering by current league
CREATE INDEX IF NOT EXISTS idx_players_current_league ON players(current_league_id);

-- ============================================================================
-- PLAYER_TEAMS TABLE UPDATES
-- ============================================================================

-- Add detected_at for tracking when we detected the assignment
ALTER TABLE player_teams ADD COLUMN detected_at INTEGER;

-- ============================================================================
-- ENTITIES_MINIMAL TABLE (Non-Priority Leagues)
-- ============================================================================

-- Minimal entity data for autocomplete in non-priority leagues
CREATE TABLE IF NOT EXISTS entities_minimal (
    id INTEGER PRIMARY KEY,
    entity_type TEXT NOT NULL CHECK(entity_type IN ('team', 'player')),
    sport_id TEXT NOT NULL REFERENCES sports(id),
    league_id INTEGER REFERENCES leagues(id),
    name TEXT NOT NULL,
    normalized_name TEXT,
    tokens TEXT,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

-- Indexes for searching minimal entities
CREATE INDEX IF NOT EXISTS idx_entities_minimal_type ON entities_minimal(entity_type, sport_id);
CREATE INDEX IF NOT EXISTS idx_entities_minimal_league ON entities_minimal(league_id);
CREATE INDEX IF NOT EXISTS idx_entities_minimal_search ON entities_minimal(normalized_name);

-- ============================================================================
-- SYNC_LOG TABLE UPDATES
-- ============================================================================

-- Add league_id for tracking league-specific syncs
ALTER TABLE sync_log ADD COLUMN league_id INTEGER REFERENCES leagues(id);

-- ============================================================================
-- UPDATE SCHEMA VERSION
-- ============================================================================

UPDATE meta SET value = '2.1', updated_at = strftime('%s', 'now') WHERE key = 'schema_version';
