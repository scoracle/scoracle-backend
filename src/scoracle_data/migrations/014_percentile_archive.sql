-- Migration: 014_percentile_archive
--
-- Create percentile_archive table for historical percentile preservation.
-- This allows end-of-season snapshots to be preserved while the live
-- percentile_cache continues to be overwritten during the season.
--
-- Usage:
-- - During season: percentile_cache is overwritten on each recalculation
-- - End of season: Run archive command to copy to percentile_archive
-- - Historical queries: SELECT from percentile_archive with season filter

CREATE TABLE IF NOT EXISTS percentile_archive (
    id SERIAL PRIMARY KEY,

    -- Same structure as percentile_cache
    entity_type VARCHAR(10) NOT NULL,  -- 'player' or 'team'
    entity_id INTEGER NOT NULL,
    sport_id VARCHAR(20) NOT NULL,
    season_id INTEGER NOT NULL,
    stat_category VARCHAR(50) NOT NULL,
    stat_value REAL,
    percentile REAL,
    rank INTEGER,
    sample_size INTEGER,
    comparison_group VARCHAR(100),
    calculated_at INTEGER,

    -- Archive-specific fields
    archived_at INTEGER NOT NULL,      -- Unix timestamp when archived
    is_final BOOLEAN DEFAULT false,    -- True = end-of-season snapshot

    -- Unique constraint prevents duplicate archives
    UNIQUE(entity_type, entity_id, sport_id, season_id, stat_category, archived_at)
);

-- Indexes for common query patterns
CREATE INDEX IF NOT EXISTS idx_percentile_archive_sport_season
    ON percentile_archive(sport_id, season_id);

CREATE INDEX IF NOT EXISTS idx_percentile_archive_entity
    ON percentile_archive(entity_type, entity_id, sport_id);

CREATE INDEX IF NOT EXISTS idx_percentile_archive_final
    ON percentile_archive(sport_id, season_id, is_final)
    WHERE is_final = true;

-- Example queries:
--
-- Get final 2024 percentiles for a player:
-- SELECT * FROM percentile_archive
-- WHERE entity_type = 'player' AND entity_id = 123
--   AND sport_id = 'NBA' AND season_id = 1 AND is_final = true;
--
-- Compare player across seasons:
-- SELECT season_id, stat_category, percentile
-- FROM percentile_archive
-- WHERE entity_type = 'player' AND entity_id = 123
--   AND sport_id = 'NBA' AND is_final = true
-- ORDER BY season_id, stat_category;
