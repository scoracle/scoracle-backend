-- 148_transfer_debounce_comment.sql
--
-- F3 of the flow-friction plan (2026-07-12): the per-pair transfer debounce ships, so the
-- mig-145 "Reserved for the transfer debounce" placeholder comment on transfer_rumors.input_hash
-- is stale. Comment-only — the column itself has existed since 145; the new cognition binary
-- stamps it on every row and gates on it before each pair's model call.
--
-- The product decision mig 145 deferred (skipping regeneration changes rumor-retirement
-- semantics: an unchanged pair's row ages out of the /transfers 7-day current-week window
-- instead of being kept fresh by redundant re-vets) is RESOLVED by the 2026-07-12 plan:
-- unchanged pairs cooling off naturally with their sources IS the source-freshness doctrine
-- working. A pair with new material (corpus grew, an article aged out of the 14-day co-mention
-- window, a source was re-tiered, the deterministic relationship flipped) re-vets because its
-- fingerprint moves.
--
-- Deploy order: any time — no behavior, no new objects. The F3 binary works against the
-- pre-148 schema (it only needs the mig-145 column).

BEGIN;

COMMENT ON COLUMN public.transfer_rumors.input_hash IS
    'SHA-256 (128-bit hex prefix) of the pair''s material vetting inputs (sorted pair-corpus news ids + distinct_sources/tier_weight from the heat components + the deterministic team relationship); the per-pair transfer debounce key (F3, flow-friction plan 2026-07-12). Time-decay heat components are deliberately excluded. NULL on pre-F3 rows, which never match — each pair regenerates once post-deploy, then stamps.';

INSERT INTO public.schema_migrations(version) VALUES ('148_transfer_debounce_comment')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.
