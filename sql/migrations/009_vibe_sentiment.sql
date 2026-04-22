-- Vibe pivot: qualitative blurb → numeric sentiment score (1-100).
--
-- Migration 007 treated vibe as a narrative blurb. That required Gemma to
-- generate ~140 char prose per call with NumPredict=800 worth of reasoning
-- headroom — too expensive to cover more than headliners on real-time
-- triggers and starters in daily batch.
--
-- The new shape asks Gemma for a single integer. Output tokens drop by
-- ~50x, which lets us score the long tail and power new surfaces like
-- "hottest entities." The frontend owns all presentation (emoji, color).
--
-- We keep the blurb column (nullable) as a debug surface: the generator
-- writes NULL by default, but a hybrid mode later can store a short
-- rationale alongside the score without another migration.

ALTER TABLE vibe_scores
    ADD COLUMN IF NOT EXISTS sentiment SMALLINT
        CHECK (sentiment IS NULL OR sentiment BETWEEN 1 AND 100);

ALTER TABLE vibe_scores ALTER COLUMN blurb DROP NOT NULL;

CREATE INDEX IF NOT EXISTS idx_vibe_scores_sport_sentiment
    ON vibe_scores (sport, sentiment DESC, generated_at DESC)
    WHERE sentiment IS NOT NULL;
