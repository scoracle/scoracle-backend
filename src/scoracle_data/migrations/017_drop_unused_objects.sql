-- Migration: 017_drop_unused_objects
--
-- Drops database objects that are no longer used by the application:
--
-- 1. All 9 materialized views from migration 009 — never queried by application code.
--    These views also JOIN against the `players`/`teams` UNION views (016), adding
--    unnecessary overhead on refresh.
--
-- 2. The refresh_all_materialized_views() function from migration 009.
--
-- 3. The percentile_cache table — superseded by JSONB percentiles stored directly
--    in sport-specific stats tables (since migration 011_percentiles_jsonb).
--    Indexes from migrations 007, 008, 010 are dropped with CASCADE.
--
-- NOTE: The `players` and `teams` UNION views (migration 016) are intentionally
-- KEPT — they are still queried by ml/jobs/vibe_calculator.py and
-- ml/jobs/prediction_refresh.py.

-- ============================================================================
-- Drop Materialized Views (migration 009)
-- ============================================================================

-- NBA
DROP MATERIALIZED VIEW IF EXISTS mv_nba_player_leaderboard CASCADE;
DROP MATERIALIZED VIEW IF EXISTS mv_nba_team_standings CASCADE;

-- NFL
DROP MATERIALIZED VIEW IF EXISTS mv_nfl_passing_leaders CASCADE;
DROP MATERIALIZED VIEW IF EXISTS mv_nfl_rushing_leaders CASCADE;
DROP MATERIALIZED VIEW IF EXISTS mv_nfl_receiving_leaders CASCADE;
DROP MATERIALIZED VIEW IF EXISTS mv_nfl_team_standings CASCADE;

-- Football (Soccer)
DROP MATERIALIZED VIEW IF EXISTS mv_football_top_scorers CASCADE;
DROP MATERIALIZED VIEW IF EXISTS mv_football_standings CASCADE;

-- Cross-sport
DROP MATERIALIZED VIEW IF EXISTS mv_player_profiles CASCADE;

-- ============================================================================
-- Drop the refresh function (migration 009)
-- ============================================================================

DROP FUNCTION IF EXISTS refresh_all_materialized_views();

-- ============================================================================
-- Drop percentile_cache table and all its indexes (migrations 007, 008, 010)
-- ============================================================================
-- CASCADE drops dependent indexes automatically:
--   idx_percentile_entity (007)
--   idx_percentile_lookup (007)
--   idx_percentile_ranking (007)
--   idx_percentile_cache_entity_lookup (008)
--   idx_percentile_cache_nonzero_lookup (010)

DROP TABLE IF EXISTS percentile_cache CASCADE;
