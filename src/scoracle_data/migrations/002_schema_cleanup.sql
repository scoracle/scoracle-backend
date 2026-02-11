-- Scoracle Data — Schema Cleanup v5.1
-- Created: 2026-02-10
-- Purpose: Normalize types, add typed columns, improve indexes.
--
-- Changes:
--   1. Add conference/division typed columns to teams table
--   2. Backfill conference/division from teams.meta JSONB
--   3. Add GIN indexes on percentiles JSONB columns
--   4. Add expression indexes for common JSONB sort keys
--   5. Normalize ML tables: VARCHAR -> TEXT, TIMESTAMP -> TIMESTAMPTZ
--   6. Fix percentile_archive: INTEGER timestamps -> TIMESTAMPTZ

-- ============================================================================
-- 1. TEAMS: Add conference/division as typed columns
-- ============================================================================
-- These are universal for NBA/NFL, NULL for football. Enables indexing and
-- clean WHERE clauses instead of JSONB extraction in application code.

ALTER TABLE teams ADD COLUMN IF NOT EXISTS conference TEXT;
ALTER TABLE teams ADD COLUMN IF NOT EXISTS division TEXT;

-- Backfill from meta JSONB for existing rows
UPDATE teams
SET conference = meta->>'conference',
    division = meta->>'division'
WHERE meta->>'conference' IS NOT NULL
   OR meta->>'division' IS NOT NULL;

-- Index for conference/division filtering (NBA/NFL only)
CREATE INDEX IF NOT EXISTS idx_teams_conference
    ON teams(sport, conference) WHERE conference IS NOT NULL;

-- ============================================================================
-- 2. PERCENTILES: Add GIN indexes
-- ============================================================================
-- stats JSONB already has GIN indexes; percentiles JSONB does not despite
-- being iterated with jsonb_each in archive operations.

CREATE INDEX IF NOT EXISTS idx_player_stats_percentiles_gin
    ON player_stats USING gin(percentiles);
CREATE INDEX IF NOT EXISTS idx_team_stats_percentiles_gin
    ON team_stats USING gin(percentiles);

-- ============================================================================
-- 3. EXPRESSION INDEXES for common JSONB sort keys
-- ============================================================================
-- Standings queries sort by (stats->>'wins')::integer and (stats->>'points')::integer.
-- Without expression indexes, Postgres evaluates these for every row.

CREATE INDEX IF NOT EXISTS idx_team_stats_wins
    ON team_stats (((stats->>'wins')::integer))
    WHERE (stats->>'wins') IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_team_stats_points
    ON team_stats (((stats->>'points')::integer))
    WHERE (stats->>'points') IS NOT NULL;

-- ============================================================================
-- 4. PERCENTILE ARCHIVE: Fix timestamp types
-- ============================================================================
-- calculated_at and archived_at were INTEGER (Unix timestamps) while every
-- other table uses TIMESTAMPTZ. Migrate to TIMESTAMPTZ for consistency.

-- Convert existing integer timestamps to TIMESTAMPTZ
ALTER TABLE percentile_archive
    ALTER COLUMN calculated_at TYPE TIMESTAMPTZ
    USING CASE
        WHEN calculated_at IS NOT NULL THEN to_timestamp(calculated_at)
        ELSE NULL
    END;

ALTER TABLE percentile_archive
    ALTER COLUMN archived_at TYPE TIMESTAMPTZ
    USING to_timestamp(archived_at);

-- Set a proper default
ALTER TABLE percentile_archive
    ALTER COLUMN archived_at SET DEFAULT NOW();

-- Also fix the VARCHAR columns to TEXT for consistency
ALTER TABLE percentile_archive
    ALTER COLUMN entity_type TYPE TEXT,
    ALTER COLUMN stat_category TYPE TEXT,
    ALTER COLUMN comparison_group TYPE TEXT;

-- ============================================================================
-- 5. ML TABLES: Normalize VARCHAR -> TEXT, TIMESTAMP -> TIMESTAMPTZ
-- ============================================================================
-- Postgres treats TEXT and VARCHAR(n) identically for performance, but the
-- inconsistency with core tables is confusing. Standardize on TEXT + TIMESTAMPTZ.

-- transfer_links
ALTER TABLE transfer_links
    ALTER COLUMN player_name TYPE TEXT,
    ALTER COLUMN player_current_team TYPE TEXT,
    ALTER COLUMN team_name TYPE TEXT,
    ALTER COLUMN sport TYPE TEXT,
    ALTER COLUMN trend_direction TYPE TEXT,
    ALTER COLUMN first_linked_at TYPE TIMESTAMPTZ,
    ALTER COLUMN last_mention_at TYPE TIMESTAMPTZ,
    ALTER COLUMN transfer_completed_at TYPE TIMESTAMPTZ,
    ALTER COLUMN created_at TYPE TIMESTAMPTZ,
    ALTER COLUMN updated_at TYPE TIMESTAMPTZ;

ALTER TABLE transfer_links
    ALTER COLUMN first_linked_at SET DEFAULT NOW(),
    ALTER COLUMN last_mention_at SET DEFAULT NOW(),
    ALTER COLUMN created_at SET DEFAULT NOW(),
    ALTER COLUMN updated_at SET DEFAULT NOW();

-- transfer_mentions
ALTER TABLE transfer_mentions
    ALTER COLUMN source_type TYPE TEXT,
    ALTER COLUMN source_name TYPE TEXT,
    ALTER COLUMN source_url TYPE TEXT,
    ALTER COLUMN mentioned_at TYPE TIMESTAMPTZ,
    ALTER COLUMN created_at TYPE TIMESTAMPTZ;

ALTER TABLE transfer_mentions
    ALTER COLUMN created_at SET DEFAULT NOW();

-- historical_transfers
ALTER TABLE historical_transfers
    ALTER COLUMN player_name TYPE TEXT,
    ALTER COLUMN from_team_name TYPE TEXT,
    ALTER COLUMN to_team_name TYPE TEXT,
    ALTER COLUMN sport TYPE TEXT,
    ALTER COLUMN created_at TYPE TIMESTAMPTZ;

ALTER TABLE historical_transfers
    ALTER COLUMN created_at SET DEFAULT NOW();

-- transfer_predictions
ALTER TABLE transfer_predictions
    ALTER COLUMN model_version TYPE TEXT,
    ALTER COLUMN predicted_at TYPE TIMESTAMPTZ;

ALTER TABLE transfer_predictions
    ALTER COLUMN predicted_at SET DEFAULT NOW();

-- vibe_scores
ALTER TABLE vibe_scores
    ALTER COLUMN entity_type TYPE TEXT,
    ALTER COLUMN entity_name TYPE TEXT,
    ALTER COLUMN sport TYPE TEXT,
    ALTER COLUMN calculated_at TYPE TIMESTAMPTZ;

ALTER TABLE vibe_scores
    ALTER COLUMN calculated_at SET DEFAULT NOW();

-- sentiment_samples
ALTER TABLE sentiment_samples
    ALTER COLUMN source_type TYPE TEXT,
    ALTER COLUMN sentiment_label TYPE TEXT,
    ALTER COLUMN analyzed_at TYPE TIMESTAMPTZ;

ALTER TABLE sentiment_samples
    ALTER COLUMN analyzed_at SET DEFAULT NOW();

-- entity_embeddings
ALTER TABLE entity_embeddings
    ALTER COLUMN entity_type TYPE TEXT,
    ALTER COLUMN entity_name TYPE TEXT,
    ALTER COLUMN sport TYPE TEXT,
    ALTER COLUMN season TYPE TEXT,
    ALTER COLUMN position TYPE TEXT,
    ALTER COLUMN computed_at TYPE TIMESTAMPTZ;

ALTER TABLE entity_embeddings
    ALTER COLUMN computed_at SET DEFAULT NOW();

-- entity_similarities
ALTER TABLE entity_similarities
    ALTER COLUMN entity_type TYPE TEXT,
    ALTER COLUMN entity_name TYPE TEXT,
    ALTER COLUMN similar_entity_name TYPE TEXT,
    ALTER COLUMN sport TYPE TEXT,
    ALTER COLUMN season TYPE TEXT,
    ALTER COLUMN computed_at TYPE TIMESTAMPTZ;

ALTER TABLE entity_similarities
    ALTER COLUMN computed_at SET DEFAULT NOW();

-- performance_predictions
ALTER TABLE performance_predictions
    ALTER COLUMN entity_type TYPE TEXT,
    ALTER COLUMN entity_name TYPE TEXT,
    ALTER COLUMN opponent_name TYPE TEXT,
    ALTER COLUMN sport TYPE TEXT,
    ALTER COLUMN model_version TYPE TEXT,
    ALTER COLUMN predicted_at TYPE TIMESTAMPTZ,
    ALTER COLUMN evaluated_at TYPE TIMESTAMPTZ;

ALTER TABLE performance_predictions
    ALTER COLUMN predicted_at SET DEFAULT NOW();

-- prediction_accuracy
ALTER TABLE prediction_accuracy
    ALTER COLUMN model_type TYPE TEXT,
    ALTER COLUMN model_version TYPE TEXT,
    ALTER COLUMN sport TYPE TEXT,
    ALTER COLUMN entity_type TYPE TEXT,
    ALTER COLUMN stat_name TYPE TEXT,
    ALTER COLUMN calculated_at TYPE TIMESTAMPTZ;

ALTER TABLE prediction_accuracy
    ALTER COLUMN calculated_at SET DEFAULT NOW();

-- ml_features
ALTER TABLE ml_features
    ALTER COLUMN entity_type TYPE TEXT,
    ALTER COLUMN sport TYPE TEXT,
    ALTER COLUMN feature_set TYPE TEXT,
    ALTER COLUMN computed_at TYPE TIMESTAMPTZ,
    ALTER COLUMN valid_until TYPE TIMESTAMPTZ;

ALTER TABLE ml_features
    ALTER COLUMN computed_at SET DEFAULT NOW();

-- ml_models
ALTER TABLE ml_models
    ALTER COLUMN model_type TYPE TEXT,
    ALTER COLUMN model_version TYPE TEXT,
    ALTER COLUMN sport TYPE TEXT,
    ALTER COLUMN model_path TYPE TEXT,
    ALTER COLUMN created_at TYPE TIMESTAMPTZ,
    ALTER COLUMN activated_at TYPE TIMESTAMPTZ,
    ALTER COLUMN deactivated_at TYPE TIMESTAMPTZ;

ALTER TABLE ml_models
    ALTER COLUMN created_at SET DEFAULT NOW();

-- ml_job_runs
ALTER TABLE ml_job_runs
    ALTER COLUMN job_name TYPE TEXT,
    ALTER COLUMN sport TYPE TEXT,
    ALTER COLUMN started_at TYPE TIMESTAMPTZ,
    ALTER COLUMN completed_at TYPE TIMESTAMPTZ,
    ALTER COLUMN status TYPE TEXT;

ALTER TABLE ml_job_runs
    ALTER COLUMN started_at SET DEFAULT NOW();

-- ============================================================================
-- 6. Update schema version
-- ============================================================================

UPDATE meta SET value = '5.1', updated_at = NOW() WHERE key = 'schema_version';
