-- 082_pipeline_stats.sql
--
-- Corpus / asset-tracking scaffolding. The news + vibes + transfers corpus is a
-- durable proprietary asset that compounds daily; this table captures a daily
-- snapshot per sport so we have its growth + coverage curve over time. Written
-- by a maintenance ticker (StatsInterval, ~24h) — cheap, append-by-day.
--
-- First-class columns for the core metrics; a `metrics` JSONB carries anything
-- we want to track later without a migration. One row per (sport, day) — upsert.

BEGIN;

CREATE TABLE IF NOT EXISTS pipeline_stats (
    id                     BIGSERIAL   PRIMARY KEY,
    sport                  TEXT        NOT NULL REFERENCES sports(id),
    snapshot_date          DATE        NOT NULL,

    -- Corpus size + daily inflow.
    total_articles         INTEGER     NOT NULL DEFAULT 0,
    new_articles           INTEGER     NOT NULL DEFAULT 0,

    -- Gemma output coverage (distinct entities with a CURRENT row, i.e. analysis
    -- generated within the freshness window).
    entities_with_summary  INTEGER     NOT NULL DEFAULT 0,
    entities_with_vibe     INTEGER     NOT NULL DEFAULT 0,
    transfer_rumors_active INTEGER     NOT NULL DEFAULT 0,

    -- Freshness: share of in-scope entities with analysis < 24h old, and the
    -- median age of the latest analysis (hours).
    coverage_pct           NUMERIC(5,2),
    median_staleness_hours NUMERIC(8,2),

    -- Extensible bag for future metrics (no migration needed to add one).
    metrics                JSONB       NOT NULL DEFAULT '{}'::jsonb,

    generated_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (sport, snapshot_date)
);

-- Growth/coverage timeseries read: a sport's snapshots over time, newest first.
CREATE INDEX IF NOT EXISTS idx_pipeline_stats_sport_date
    ON pipeline_stats(sport, snapshot_date DESC);

COMMIT;
