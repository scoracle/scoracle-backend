-- 145_narratives_input_hash.sql
--
-- Debounce hashes for the two cognition stages that still regenerated unconditionally every
-- wake cycle (lens quality plan Phase 1). Same shape as momentum_summaries/sigil_synthesis
-- (migration 144): the Rust harness hashes the stage's material inputs and skips the model
-- call when the latest persisted generation carries the same hash.
--
-- Nullable by design: pre-145 rows have NULL, which never matches, so each entity regenerates
-- exactly once after deploy and stamps its hash from then on.
--
-- Narratives (news_summaries) gets the live gate now. transfer_rumors gets the column so the
-- Rust side CAN stamp provenance, but the transfer skip-gate itself is deliberately deferred:
-- the /transfers read path keys freshness off generated_at (7-day window), so skipping
-- regeneration changes rumor-retirement semantics — that interaction needs a product decision
-- first (see scoracle-wiki progress_docs 2026-07-11_lens-quality-richness-plan.md).

-- Deploy order: ADDITIVE — apply BEFORE the cognition-service deploy (the new binary's
-- narratives INSERT names the column; the running binary ignores it).

BEGIN;

ALTER TABLE public.news_summaries ADD COLUMN IF NOT EXISTS input_hash TEXT;
ALTER TABLE public.transfer_rumors ADD COLUMN IF NOT EXISTS input_hash TEXT;

COMMENT ON COLUMN public.news_summaries.input_hash IS
    'SHA-256 (128-bit hex prefix) of the generation''s material inputs (vetted corpus article ids + transfer-heat facts); the Rust harness debounce key. NULL on pre-145 rows and markers written before the gate.';
COMMENT ON COLUMN public.transfer_rumors.input_hash IS
    'Reserved for the transfer debounce (Phase 1 follow-up); written as provenance once the transfer gate ships.';

INSERT INTO public.schema_migrations(version) VALUES ('145_narratives_input_hash')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.
