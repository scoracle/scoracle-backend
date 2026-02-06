# Migration History

Migrations are applied alphabetically by filename. The `meta` table tracks
which migrations have been applied (key: `migration_{filename_stem}`).

## Known Issues

- Duplicate numbers exist (two 001s, two 004s, two 010s, two 011s).
  This works correctly because the runner uses full filenames, not numbers.
- Migrations 001-006 are SQLite-era and are no-ops on PostgreSQL (they use
  SQLite syntax that PostgreSQL ignores or handles gracefully via IF NOT EXISTS).
- Migration 016_unified_views creates cross-sport UNION views (`players`, `teams`).
  These are still used by `ml/jobs/vibe_calculator.py` and `ml/jobs/prediction_refresh.py`.
  They should eventually be replaced with direct sport-specific table queries.

## Active Schema (PostgreSQL)

The effective schema is defined by migrations 007+ applied in order:
- 007: PostgreSQL sport-specific schema (v4.0 — the real baseline)
- 008: Performance indexes
- 009: Materialized views (dropped by migration 017)
- 010: Fixtures schedule + Percentile optimization (percentile indexes dropped by 017)
- 011: Composite PKs + Percentiles JSONB
- 012: NBA per-36 stats columns
- 013: ML tables (transfer_links, vibe_scores, performance_predictions)
- 014: Percentile archive table
- 015: raw_response JSONB column on all stats tables
- 016: Cross-sport UNION views (`players`, `teams`) — still used by ML jobs
- 017: Drop unused materialized views (009), percentile_cache table, and stale indexes

## For New Deployments

All migrations run in order. The SQLite-era ones (001-006) are harmless
on PostgreSQL. Migration 007 creates the actual production schema.
