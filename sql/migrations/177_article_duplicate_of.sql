-- 177_article_duplicate_of.sql
--
-- Cognition refactor, Phase 2 (narrow the candle): the source-aware NOVELTY gate.
--
-- WHAT. Two changes, both in service of the scrub-time novelty gate (rust/src/novelty.rs):
--   1. news_articles.duplicate_of — a self-referential nullable FK. When the gate finds a
--      newly-scrubbed article X is a near-dup REPOST of an already-canonical article R (same
--      story AND either the same outlet reposting or near-verbatim syndication), it points
--      X.duplicate_of at R. NULL = canonical (first-seen, or genuine cross-outlet corroboration
--      that must be counted). ON DELETE SET NULL so deleting an original re-canonicalizes its
--      copies rather than cascading them away. A partial index carries the reverse lookup
--      ("the duplicates of R") that the Phase 3 canonical membership count will use.
--   2. topic_heat_embeddings — REPURPOSED from the retired topic-heat cache into the novelty
--      gate's article-embedding store (canonical articles only). The columns are unchanged
--      (article_id, fingerprint, model, embedding, embedded_at); only the meaning changes, so
--      old topic-heat rows are TRUNCATEd for a clean semantic break (the table is documented
--      "safe to truncate; reproducible from news_articles" — the store re-warms as canonical
--      articles are scrubbed, and a cold start under-suppresses, which is the safe direction).
--
-- WHY. The old dedup (narratives::dedup_corpus) clustered source-BLIND and dropped every outlet
-- but one — destroying the corroboration signal. This gate suppresses only reposts/syndication
-- and lets independent coverage through; duplicate_of is the durable record of that judgment.
--
-- Consumption of duplicate_of (narratives corpus filter; vetted-trigger canonical membership
-- counting) lands in Phase 3 + its trigger migrations — NOT here. Until then this column
-- accumulates the signal (and scopes the gate's own comparison set) without acting downstream.
--
-- Deploy order: ADDITIVE — apply this BEFORE deploying the binary whose novelty gate SELECTs /
-- UPDATEs na.duplicate_of and writes topic_heat_embeddings (db statement prep validates columns
-- against the live schema). The Go serving path never reads duplicate_of; no API restart needed.

BEGIN;

-- 1. The canonical/duplicate link. Self-referential; ON DELETE SET NULL keeps copies canonical.
ALTER TABLE public.news_articles
    ADD COLUMN IF NOT EXISTS duplicate_of bigint
        REFERENCES public.news_articles(id) ON DELETE SET NULL;

COMMENT ON COLUMN public.news_articles.duplicate_of IS
    'Cognition refactor Phase 2 (mig 177): set by the scrub source-aware novelty gate to the '
    'canonical article this one is a near-dup REPOST of (same story AND same-outlet repost or '
    'near-verbatim syndication). NULL = canonical: first-seen, or genuine cross-outlet '
    'corroboration that is counted. Suppression sets this ONLY; vetted membership is untouched, '
    'so no derive re-fires. Consumed by narratives corpus filtering + canonical membership '
    'counting in Phase 3.';

-- Reverse lookup (duplicates of a given canonical) + ON DELETE SET NULL support. Partial on the
-- minority (only actual duplicates), so it stays small; the canonical read filters IS NULL via
-- the entity join, not this index.
CREATE INDEX IF NOT EXISTS idx_news_articles_duplicate_of
    ON public.news_articles (duplicate_of)
    WHERE duplicate_of IS NOT NULL;

-- 2. Repurpose the embedding store. Clean break from the topic-heat cache's rows.
TRUNCATE public.topic_heat_embeddings;

COMMENT ON TABLE public.topic_heat_embeddings IS
    'Cognition refactor Phase 2 (mig 177): repurposed into the novelty gate''s article-embedding '
    'store — the CANONICAL article context embeddings the scrub gate compares new arrivals '
    'against (article_id, fingerprint=text hash, model=embedder identity, embedding=LE f32 bytes). '
    'Was the topic-heat write-through cache (mig 151). Reproducible from news_articles text; '
    'safe to truncate.';

INSERT INTO public.schema_migrations(version) VALUES ('177_article_duplicate_of')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.
