-- Migration 010: Percentile Query Optimization
-- 
-- Adds a partial index for percentile queries that filter out zero/null values.
-- This optimizes the common query pattern used by /percentiles and /profile endpoints.
--
-- Expected improvement: 20-50% faster percentile lookups

-- Partial index for non-zero percentile lookups
-- Covers the WHERE clause: stat_value IS NOT NULL AND stat_value != 0
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_percentile_cache_nonzero_lookup
ON percentile_cache(entity_type, entity_id, sport_id, season_id)
WHERE stat_value IS NOT NULL AND stat_value != 0;

-- Add comment explaining the optimization
COMMENT ON INDEX idx_percentile_cache_nonzero_lookup IS 
    'Partial index for percentile queries filtering out zero/null values. Covers /percentiles and /profile endpoints.';
