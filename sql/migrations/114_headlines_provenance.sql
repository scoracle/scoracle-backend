-- 114_headlines_provenance.sql
--
-- Headlines feature v1 — add provenance columns that mirror news_summaries /
-- transfer_rumors so downstream stages (transfers, narratives, vibe) can read
-- the headline table for enrichment and lineage.
--
-- input_news_ids: articles cited by the model (1+ article IDs).
-- model_version: concrete model that produced the headline.
-- prompt_version: HEADLINES_PROMPT_VERSION constant at generation time.
-- trigger_type: why this generation ran (news_spike for the vetted-link trigger).
-- generated_at: when the headline row was produced (distinct from published_at,
-- which is the source article's timestamp).

BEGIN;

ALTER TABLE headlines
    ADD COLUMN IF NOT EXISTS input_news_ids BIGINT[] NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS model_version TEXT,
    ADD COLUMN IF NOT EXISTS prompt_version TEXT,
    ADD COLUMN IF NOT EXISTS trigger_type TEXT NOT NULL DEFAULT 'news_spike'
        CHECK (trigger_type IN ('news_spike', 'periodic', 'manual')),
    ADD COLUMN IF NOT EXISTS generated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- created_at from migration 113 is redundant now that generated_at exists.
ALTER TABLE headlines DROP COLUMN IF EXISTS created_at;

COMMIT;
