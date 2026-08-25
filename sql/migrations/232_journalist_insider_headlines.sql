-- 232_journalist_insider_headlines.sql
--
-- The uniform card contract closes (score + headline + body, all six consumer seats): the
-- two seats mig 226 passed over gain their ENTITY-LEVEL headline. 226's rationale was that
-- news_summaries.narrative_title and the Insider's per-pair model_summary "serve as their own
-- headlines" — Scott overruled it 2026-08-24: those are per-item titles, and the card's hook
-- is the seat's read of the WHOLE day ("a busy day of narratives is headline theme material
-- for the Journalist; no transfer movement is a good headline theme for the Insider").
--
-- news_summaries.headline is GENERATION-LEVEL: the same value on every row of a generation,
-- the card_score pattern exactly (n12). Persisted on the called-empty marker row too — a
-- quiet week's honest hook is the product, not an absence.
--
-- ADDITIVE + nullable, no backfill: pre-contract rows stay NULL and the cards render without
-- a hook until each seat naturally regenerates (the mig 226 lazy-invalidation norm). The
-- Insider self-backfills one wrap per live-wire entity via the is5 bump (the is4 precedent);
-- the Journalist heals with material movement (no n-bump — Scott's no-fleet-regen rule).
--
-- Deploy order: additive ⇒ migrate BEFORE the release.

BEGIN;

ALTER TABLE public.news_summaries
    ADD COLUMN IF NOT EXISTS headline text;

ALTER TABLE public.insider_scores
    ADD COLUMN IF NOT EXISTS headline text;

COMMENT ON COLUMN public.news_summaries.headline IS
    'The Journalist''s entity-level card hook (tweet contract: 140 chars, guards::settle_title). '
    'Generation-level — identical on every row of a generation, marker rows included (a quiet '
    'week hooks as a quiet week). NULL = no-corpus marker, a pre-232 row, or a dropped title '
    '(a junk title costs the title, never the card).';
COMMENT ON COLUMN public.insider_scores.headline IS
    'The Insider''s entity-level card hook for this wire wrap (tweet contract: 140 chars, '
    'guards::settle_title). NULL = pre-232/is4 row or a dropped title. A dead wire still has '
    'no row at all — the Veil stays the no-board answer.';

INSERT INTO public.schema_migrations(version) VALUES ('232_journalist_insider_headlines')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.
