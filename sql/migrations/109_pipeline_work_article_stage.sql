-- 109_pipeline_work_article_stage.sql  (Rust Cognition Harness L6 — scrub as a first-class queue stage)
--
-- Option (i) of the L6 live-scrub flip (vault Plan §8.5): scrub becomes a `pipeline_work`
-- stage instead of running inline in the Go maintenance ticker. Scrub is ARTICLE-keyed (not
-- per-entity like vibe/sigil), so its work rows are
--   (stage='scrub', entity_type='article', entity_id=<news_articles.id>, sport).
-- This migration relaxes the entity_type CHECK to admit 'article'. entity_id stays INTEGER:
-- news_articles.id maxes at ~90k today (bigserial, but ~23,000x of headroom before the INTEGER
-- ceiling), so no column widening is needed now — a bigint widening is a future migration if it
-- ever approaches the ceiling.
--
-- The Rust ScrubHandler is the ONLY drainer of 'scrub' (the Go Drainer names its stages explicitly
-- — transfers/narratives/vibe/sigil — and has no scrub handler, so there is no double-claim). It
-- writes news_article_entities.vetted, which fires the mig-103 `AFTER UPDATE OF vetted` trigger →
-- the per-entity derive stages enqueue exactly as they do today.
--
-- INERT until the cutover: the Go maintenance enqueue path is flag-gated; until the flag flips, Go
-- keeps scrubbing inline and no 'article'/'scrub' rows are ever written. This migration only widens
-- a CHECK — it changes no behavior on its own.
--
-- Additive + idempotent.

BEGIN;

ALTER TABLE pipeline_work DROP CONSTRAINT IF EXISTS pipeline_work_entity_type_check;
ALTER TABLE pipeline_work ADD CONSTRAINT pipeline_work_entity_type_check
    CHECK (entity_type IN ('player', 'team', 'article'));

COMMIT;
