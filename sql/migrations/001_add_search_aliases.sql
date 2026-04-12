-- Add search_aliases column to teams and players for fuzzy news matching.
-- Stores alternate name forms (e.g., "Bayern Munich", "FC Bayern" for "FC Bayern Munchen").

ALTER TABLE teams ADD COLUMN IF NOT EXISTS search_aliases TEXT[] DEFAULT '{}';
ALTER TABLE players ADD COLUMN IF NOT EXISTS search_aliases TEXT[] DEFAULT '{}';

CREATE INDEX IF NOT EXISTS idx_teams_search_aliases ON teams USING GIN(search_aliases);
