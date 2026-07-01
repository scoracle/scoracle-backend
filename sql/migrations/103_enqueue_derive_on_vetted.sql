-- 103_enqueue_derive_on_vetted.sql  (FIRST-GPT-AUDIT Session 9 — repair real-time trigger semantics)
--
-- The pre-S9 real-time path fired pg_notify('vibe_trigger'/'transfer_trigger', ...)
-- on a raw link INSERT (a "5 articles in 60 min" volume spike) — BEFORE scrub, so it
-- could fire for links that are never vetted — and the in-API listeners ran local model
-- directly off that transient NOTIFY, holding the affected-entity set in process
-- memory (lost on restart, never recovered on a missed NOTIFY).
--
-- This migration makes the trigger ENQUEUE DURABLE WORK on the vetted=TRUE
-- transition instead. NOTIFY becomes only a low-latency wake-up; the API drains the
-- durable pipeline_work queue (migration 102) on wake, on startup, and on a
-- safety-net timeout. Audit "Done when": notifications improve latency but are never
-- required for correctness.
--
-- DRIFT NOTE (verified against the live prod DB, NOT the migration files): there are
-- TWO live AFTER INSERT triggers on news_article_entities —
-- trg_vibe_trigger_on_news_link→notify_vibe_trigger (mig 011/034) AND
-- sentiment_trigger→notify_sentiment_trigger (mig 088) — and BOTH fire
-- pg_notify('vibe_trigger', ...). (Migration 088's DROP targeted a phantom name, so
-- 011's trigger survived its rename.) We drop both by their real names below.
--
-- PARITY: input_version mirrors go/internal/corpus/corpus.go CorpusVersion exactly
-- (vetted-link count : max(scrubbed_at) epoch, over articles published within
-- ml.NewsLookback = 72h). The trigger is now the SOLE enqueuer of these stages
-- (cmd/pipeline's Go enqueue is removed in S9), so there is no cross-path fingerprint
-- to thrash against — but keep this formula in lock-step with CorpusVersion and
-- ml.NewsLookback if either changes. The ON CONFLICT clause mirrors work.Enqueue.
--
-- Additive + idempotent.

BEGIN;

-- 1. Remove the old raw-insert volume-spike triggers + their notify functions.
--    (Drop triggers before functions to clear the dependency.)
DROP TRIGGER IF EXISTS trg_vibe_trigger_on_news_link ON news_article_entities;
DROP TRIGGER IF EXISTS sentiment_trigger             ON news_article_entities;
DROP TRIGGER IF EXISTS vibe_trigger                  ON news_article_entities; -- historical alias, defensive
DROP FUNCTION IF EXISTS notify_vibe_trigger();
DROP FUNCTION IF EXISTS notify_sentiment_trigger();

-- 2. The enqueue trigger: on a link's transition into vetted=TRUE, enqueue the
--    entity's derive stages into pipeline_work and wake the drain worker.
CREATE OR REPLACE FUNCTION enqueue_derive_on_vetted() RETURNS TRIGGER AS $$
DECLARE
    v_count  integer;
    v_epoch  bigint;
    v_ver    text;
    v_stages text[] := ARRAY['narratives', 'vibe'];
BEGIN
    -- Corpus fingerprint over this entity's vetted, in-lookback (72h) links. This
    -- doubles as the freshness gate: a stale backlog primary (article published
    -- >72h ago, no other fresh vetted corpus) yields count=0 and enqueues nothing,
    -- which bounds the maintenance bulk auto-vet's per-row fan-out.
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
        RETURN NEW; -- no fresh vetted corpus → nothing worth deriving
    END IF;

    v_ver := v_count || ':' || v_epoch;

    -- Teams additionally get transfer analysis (the transfer card's grain).
    IF NEW.entity_type = 'team' THEN
        v_stages := array_append(v_stages, 'transfers');
    END IF;

    -- Enqueue each stage. Conflict policy mirrors go/internal/work Enqueue: a
    -- changed input_version (or a failed row) reopens to 'pending'; an unchanged
    -- pending/running row of the same version is left untouched (dedupe).
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

    -- Low-latency wake-up ONLY (never required for correctness — the worker also
    -- drains on startup + on a safety-net timeout). Constant payload so Postgres
    -- de-dups identical NOTIFYs within a txn: a batched scrub UPDATE (one txn per
    -- article) collapses to a single wake-up regardless of link count.
    PERFORM pg_notify('pipeline_work_ready', '');

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Fire only on the actual NULL/false → TRUE transition. Links are inserted unvetted
-- (thirdparty.persistArticles never sets vetted), so AFTER UPDATE OF vetted is
-- sufficient; the WHEN guard skips rejected (vetted=FALSE) links and TRUE→TRUE
-- re-scrubs, so rejected links produce no derivation work.
CREATE TRIGGER enqueue_derive_on_vetted
    AFTER UPDATE OF vetted ON news_article_entities
    FOR EACH ROW
    WHEN (NEW.vetted IS TRUE AND OLD.vetted IS DISTINCT FROM NEW.vetted)
    EXECUTE FUNCTION enqueue_derive_on_vetted();

-- The trigger's fingerprint subquery filters on (entity_type, entity_id, sport) +
-- vetted; the existing indexes don't cover `vetted`. This partial index keeps the
-- per-row count cheap under the maintenance Phase-1 bulk auto-vet (up to 20000 rows
-- in one UPDATE → 20000 trigger firings).
CREATE INDEX IF NOT EXISTS idx_nae_vetted_lookup
    ON news_article_entities (entity_type, entity_id, sport)
    WHERE vetted IS TRUE;

COMMIT;
