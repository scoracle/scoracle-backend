-- 093_sigil_convergence_rename.sql
--
-- Workstream B / Phase 1 (foundation) — the Sigil convergence vocabulary rotation
-- (Rating / Vibe / Momentum / Sigil). See wiki "Plan - Sigil convergence".
--
-- This migration renames the two GO-WRITTEN tables (safe — no PL/pgSQL function or
-- view references either; verified) to the new vocabulary, and gives the Vibe
-- end product its prompt column:
--
--   vibe_synthesis   → sigil_synthesis   (the crown synthesis = the new Sigil)
--   sentiment_scores → vibe_scores       (the emotional end product = the new Vibe;
--                                          reverts migration 088's table rename)
--   + vibe_scores.prompt TEXT            (D4: the Vibe's "prompt"; 088 dropped blurb)
--
-- Engine rating columns are NOT renamed here (late-bound PL/pgSQL — that surface is
-- handled by read-layer aliasing in db.go, per the 089 reversal lesson).
--
-- NOTIFY (L2): the news-volume trigger's wire string stays 'vibe_trigger' — now
-- consistent again with the Vibe product. notify_sentiment_trigger() does not touch
-- either renamed table, so it is unaffected.
--
-- The measure column on vibe_scores stays named `sentiment` (the product is Vibe; the
-- measure is sentiment — a deliberate distinction).

BEGIN;

-- 1. Crown synthesis: vibe_synthesis → sigil_synthesis
ALTER TABLE IF EXISTS vibe_synthesis RENAME TO sigil_synthesis;
ALTER INDEX IF EXISTS idx_vibe_synthesis_entity_recent RENAME TO idx_sigil_synthesis_entity_recent;
ALTER INDEX IF EXISTS idx_vibe_synthesis_sport_score   RENAME TO idx_sigil_synthesis_sport_score;

-- 2. Emotional end product: sentiment_scores → vibe_scores (reverts 088)
ALTER TABLE IF EXISTS sentiment_scores RENAME TO vibe_scores;
ALTER INDEX IF EXISTS idx_sentiment_scores_entity_recent  RENAME TO idx_vibe_scores_entity_recent;
ALTER INDEX IF EXISTS idx_sentiment_scores_recent          RENAME TO idx_vibe_scores_recent;
ALTER INDEX IF EXISTS idx_sentiment_scores_sport_sentiment RENAME TO idx_vibe_scores_sport_sentiment;
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'sentiment_scores_trigger_type_check') THEN
        ALTER TABLE vibe_scores RENAME CONSTRAINT sentiment_scores_trigger_type_check
            TO vibe_scores_trigger_type_check;
    END IF;
END $$;

-- 3. The Vibe end product gains its prompt (D4). 088 dropped the legacy blurb; this is
--    the short text local model will emit alongside the sentiment score and feed into the Sigil.
ALTER TABLE vibe_scores ADD COLUMN IF NOT EXISTS prompt TEXT;

COMMENT ON TABLE sigil_synthesis IS
    'The Sigil: the crown synthesis (was vibe_synthesis). Fuses the three end products — Rating, Vibe, Momentum — into one holistic score + blurb.';
COMMENT ON TABLE vibe_scores IS
    'The Vibe: the emotional rail end product (was sentiment_scores). sentiment (1-100) + prompt; feeds the Sigil and the Momentum vibe-trajectory. Append-only.';

COMMIT;
