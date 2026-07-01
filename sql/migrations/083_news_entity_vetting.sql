-- 083_news_entity_vetting.sql
--
-- Scrub vetting columns on the fuzzy link table. The local model ID-gate (Stage 1,
-- ml/news_scrub.go) is the PRECISION pass over the wide, high-recall fuzzy
-- matcher: per article it judges which linked entities the piece is GENUINELY
-- about (disambiguating same-name people). Rather than DELETE the links it drops
-- — which would burn the recall we may want to re-judge — it records a verdict:
--
--   vetted = TRUE   genuinely the subject (kept)
--   vetted = FALSE  fuzzy false positive (local model dropped it)
--   vetted = NULL   not yet scrubbed (scrubbed_at IS NULL)
--
-- Non-destructive + auditable: consumers read `vetted IS TRUE OR scrubbed_at IS
-- NULL` during the transition (unscrubbed links still show), tightening to
-- `vetted IS TRUE` once coverage is high. Lets us compare fuzzy-vs-vetted before
-- trusting the gate, and roll back by ignoring the flag. The primary link
-- (match_confidence = 1.0 — the entity the article was fetched for) is always
-- vetted TRUE; ml/news_scrub.go never lets local model drop it.
--
-- See vault wiki/Plan - News pipeline integration.md (Phase 2).

BEGIN;

ALTER TABLE news_article_entities
    ADD COLUMN IF NOT EXISTS vetted      BOOLEAN,
    ADD COLUMN IF NOT EXISTS scrubbed_at TIMESTAMPTZ;

COMMENT ON COLUMN news_article_entities.vetted IS
    'local model scrub verdict: TRUE genuinely the subject, FALSE fuzzy false positive, NULL not yet scrubbed. See ml/news_scrub.go.';
COMMENT ON COLUMN news_article_entities.scrubbed_at IS
    'When the local model scrub last judged this link (NULL = unscrubbed). Drives the async scrub worker backlog query.';

-- The async scrub worker (Phase 2 wiring) finds links not yet judged; this
-- partial index keeps that backlog scan cheap and shrinks as coverage catches up.
CREATE INDEX IF NOT EXISTS idx_news_entities_unscrubbed
    ON news_article_entities(article_id)
    WHERE scrubbed_at IS NULL;

COMMIT;
