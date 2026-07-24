-- 186_journalist_card_score.sql
--
-- Tarot deck Phase 4a (the Journalist's card score): every Narratives generation now carries
-- the Journalist's own 1-99 busyness verdict — model-assigned at generation time (n12),
-- grounded by the deterministic SIGNALS tally and the Journalist's prior card reads. The
-- generation (shared generated_at + input_hash) IS the per-entity reading, so the score lives
-- on news_summaries: the SAME value on every row of a generation, including the model-called
-- empty marker (a quiet week gets the Journalist's own low number). Only the no-corpus marker
-- (no model call) stays NULL — the frontend draws the Veil. `card_score_prev` mirrors
-- sigil_synthesis.previous_score: the prior generation's score as fed to the prompt's memory
-- line (continuity audit + future reversal cue) — audit-only, never served.
--
-- ADDITIVE + nullable, no backfill: pre-n12 rows stay NULL; the n11→n12 prompt_version bump
-- sits inside the generation input_hash, so every news-active entity regenerates exactly once
-- on the next sweep and scores itself (rollout is free — no reconcile binary).
--
-- Deploy order: additive ⇒ migrate BEFORE the API restart (db.New validates statements at
-- boot; the serving subquery in this release reads card_score).

BEGIN;

ALTER TABLE public.news_summaries
    ADD COLUMN IF NOT EXISTS card_score smallint
        CHECK (card_score IS NULL OR card_score BETWEEN 1 AND 99),
    ADD COLUMN IF NOT EXISTS card_score_prev smallint;

COMMENT ON COLUMN public.news_summaries.card_score IS
    'The Journalist''s 1-99 busyness verdict for this generation (n12, tarot deck Phase 4): '
    'model-assigned, uniform across the generation''s rows (called-empty marker included). '
    'NULL = no-corpus marker (no model call) or a pre-n12 row → the card draws the Veil. '
    'Served as the Narratives card''s score (latest non-NULL, scope-independent).';

COMMENT ON COLUMN public.news_summaries.card_score_prev IS
    'The previous generation''s card_score as fed to the n12 prompt''s memory line — the '
    'continuity audit (mirrors sigil_synthesis.previous_score). Audit-only, never served.';

INSERT INTO public.schema_migrations(version) VALUES ('186_journalist_card_score')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.
