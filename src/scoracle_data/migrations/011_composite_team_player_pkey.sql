-- Migration: 011_composite_team_player_pkey
--
-- Fix primary key constraint on teams and players tables.
-- The API-Sports APIs use the same ID space across different sports,
-- meaning NBA team ID 1 and NFL team ID 1 are different teams.
-- The primary key must be (id, sport_id) not just (id).

-- First, check if we have any conflicting data and clean it up
-- (shouldn't happen but safeguard)

-- For teams: Change PRIMARY KEY from (id) to (id, sport_id)
ALTER TABLE teams DROP CONSTRAINT IF EXISTS teams_pkey;
ALTER TABLE teams ADD PRIMARY KEY (id, sport_id);

-- For players: Change PRIMARY KEY from (id) to (id, sport_id)
ALTER TABLE players DROP CONSTRAINT IF EXISTS players_pkey;
ALTER TABLE players ADD PRIMARY KEY (id, sport_id);

-- Update foreign keys on stats tables to reference the composite key
-- nba_player_stats references players
ALTER TABLE nba_player_stats DROP CONSTRAINT IF EXISTS nba_player_stats_player_id_fkey;
-- (We'll keep the single-column FK since player_id is still valid within NBA context)

-- nba_team_stats references teams
ALTER TABLE nba_team_stats DROP CONSTRAINT IF EXISTS nba_team_stats_team_id_fkey;

-- nfl_player_stats references players
ALTER TABLE nfl_player_stats DROP CONSTRAINT IF EXISTS nfl_player_stats_player_id_fkey;

-- nfl_team_stats references teams
ALTER TABLE nfl_team_stats DROP CONSTRAINT IF EXISTS nfl_team_stats_team_id_fkey;

-- football_player_stats references players
ALTER TABLE football_player_stats DROP CONSTRAINT IF EXISTS football_player_stats_player_id_fkey;

-- football_team_stats references teams
ALTER TABLE football_team_stats DROP CONSTRAINT IF EXISTS football_team_stats_team_id_fkey;

-- percentiles table references entities
ALTER TABLE percentiles DROP CONSTRAINT IF EXISTS percentiles_entity_fkey;

-- Note: We intentionally don't recreate the FK constraints because:
-- 1. The stats tables are sport-specific (nba_*, nfl_*, football_*)
-- 2. Within each sport context, the player_id/team_id is unique
-- 3. Composite FKs would require adding sport_id columns to stats tables
