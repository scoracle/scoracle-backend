-- Scoracle Stats Database - Imperial Measurements & NFL Player Fields
-- Version: 2.2
-- Created: 2024-12-28
-- Purpose: Rename height/weight columns to imperial, add NFL-specific player fields

-- ============================================================================
-- PLAYERS TABLE: RENAME HEIGHT/WEIGHT TO IMPERIAL
-- ============================================================================

-- SQLite doesn't support RENAME COLUMN in older versions, so we need to:
-- 1. Create new table with correct schema
-- 2. Copy data (converting values)
-- 3. Drop old table
-- 4. Rename new table

-- Create new players table with imperial columns and new NFL fields
CREATE TABLE IF NOT EXISTS players_new (
    id INTEGER PRIMARY KEY,
    sport_id TEXT NOT NULL REFERENCES sports(id),
    first_name TEXT,
    last_name TEXT,
    full_name TEXT NOT NULL,
    position TEXT,
    position_group TEXT,
    nationality TEXT,
    birth_date TEXT,
    birth_place TEXT,
    height_inches INTEGER,      -- Renamed from height_cm
    weight_lbs INTEGER,         -- Renamed from weight_kg
    photo_url TEXT,
    current_team_id INTEGER REFERENCES teams(id),
    current_league_id INTEGER REFERENCES leagues(id),
    jersey_number INTEGER,
    -- New NFL-specific fields
    college TEXT,               -- College attended
    experience_years INTEGER,   -- Years of professional experience
    is_active INTEGER DEFAULT 1,
    profile_fetched_at INTEGER,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

-- Copy existing data, converting metric to imperial
-- height: cm -> inches (divide by 2.54)
-- weight: kg -> lbs (multiply by 2.205)
INSERT INTO players_new (
    id, sport_id, first_name, last_name, full_name, position, position_group,
    nationality, birth_date, birth_place, height_inches, weight_lbs, photo_url,
    current_team_id, current_league_id, jersey_number, is_active,
    profile_fetched_at, created_at, updated_at
)
SELECT
    id, sport_id, first_name, last_name, full_name, position, position_group,
    nationality, birth_date, birth_place,
    CASE WHEN height_cm IS NOT NULL THEN CAST(ROUND(height_cm / 2.54) AS INTEGER) ELSE NULL END,
    CASE WHEN weight_kg IS NOT NULL THEN CAST(ROUND(weight_kg * 2.205) AS INTEGER) ELSE NULL END,
    photo_url, current_team_id, current_league_id, jersey_number, is_active,
    profile_fetched_at, created_at, updated_at
FROM players;

-- Drop old table
DROP TABLE IF EXISTS players;

-- Rename new table
ALTER TABLE players_new RENAME TO players;

-- Recreate indexes
CREATE INDEX IF NOT EXISTS idx_players_sport ON players(sport_id);
CREATE INDEX IF NOT EXISTS idx_players_team ON players(current_team_id);
CREATE INDEX IF NOT EXISTS idx_players_name ON players(full_name);
CREATE INDEX IF NOT EXISTS idx_players_position ON players(sport_id, position);
CREATE INDEX IF NOT EXISTS idx_players_needs_profile ON players(profile_fetched_at) WHERE profile_fetched_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_players_current_league ON players(current_league_id);

-- ============================================================================
-- UPDATE SCHEMA VERSION
-- ============================================================================

UPDATE meta SET value = '2.2', updated_at = strftime('%s', 'now') WHERE key = 'schema_version';
