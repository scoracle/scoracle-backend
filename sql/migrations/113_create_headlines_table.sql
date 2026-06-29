-- 113_create_headlines_table.sql
--
-- Headlines feature v1 — entity-scoped breaking-news bulletins, one sentence each.
-- Adds a third product to the news rail alongside narratives and transfers.
--
-- Decisions:
--   - Categories: transfer, injury, coaching, contract, other
--   - Expiration: 2 days (filtered at read time)
--   - Sorting: published_at DESC (recency, not heat)
--   - No related entities for v1
--
-- The Rust Cognition Harness inserts here from a new HEADLINE stage that runs
-- before transfers on the vetted article stream.

BEGIN;

CREATE TABLE IF NOT EXISTS headlines (
    id            BIGSERIAL   PRIMARY KEY,
    sport         TEXT        NOT NULL REFERENCES sports(id),
    entity_type   TEXT        NOT NULL CHECK (entity_type IN ('player', 'team')),
    entity_id     INTEGER     NOT NULL,
    title         TEXT        NOT NULL,
    category      TEXT        NOT NULL CHECK (category IN ('transfer', 'injury', 'coaching', 'contract', 'other')),
    source_url    TEXT,
    source_name   TEXT,
    published_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Card read + DISTINCT ON latest-per-entity.
CREATE INDEX IF NOT EXISTS idx_headlines_entity
    ON headlines (sport, entity_type, entity_id, published_at DESC);

-- Category filter (future scope rail).
CREATE INDEX IF NOT EXISTS idx_headlines_category
    ON headlines (category, published_at DESC);

-- Expiration batch cleanup + read-time cutoff.
CREATE INDEX IF NOT EXISTS idx_headlines_published
    ON headlines (published_at DESC);

-- Update the vetted-link enqueue trigger so every entity also gets a headlines stage
-- run, in addition to narratives/vibe (and transfers for teams).
CREATE OR REPLACE FUNCTION enqueue_derive_on_vetted() RETURNS TRIGGER AS $$
DECLARE
    v_count  integer;
    v_epoch  bigint;
    v_ver    text;
    v_stages text[] := ARRAY['headlines', 'narratives', 'vibe'];
BEGIN
    SELECT count(*),
           COALESCE(EXTRACT(EPOCH FROM max(nae.scrubbed_at))::bigint, 0)
      INTO v_count, v_epoch
      FROM news_article_entities nae
      JOIN news_articles a ON a.id = nae.article_id
     WHERE nae.entity_type = NEW.entity_type
       AND nae.entity_id   = NEW.entity_id
       AND nae.sport       = NEW.sport
       AND nae.vetted IS TRUE
       AND (a.published_at IS NULL OR a.published_at > NOW() - INTERVAL '72 hours');

    IF v_count = 0 THEN
        RETURN NEW;
    END IF;

    v_ver := v_count || ':' || v_epoch;

    IF NEW.entity_type = 'team' THEN
        v_stages := array_append(v_stages, 'transfers');
    END IF;

    INSERT INTO pipeline_work
        (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
    SELECT st, NEW.entity_type, NEW.entity_id, NEW.sport, 'pending', v_ver, NOW(), NOW()
      FROM unnest(v_stages) AS st
    ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
        status        = 'pending',
        attempts      = 0,
        available_at  = NOW(),
        updated_at    = NOW(),
        last_error    = NULL,
        input_version = EXCLUDED.input_version
    WHERE pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
       OR pipeline_work.status = 'failed';

    PERFORM pg_notify('pipeline_work_ready', '');

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

COMMIT;
