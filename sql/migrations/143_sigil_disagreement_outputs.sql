-- 143_sigil_disagreement_outputs.sql
--
-- Phase 5.3 (Multi-Lens Cognition Panel): expose panel DISAGREEMENT as first-class
-- Sigil output. Sigil already synthesizes five pillars (PEAK, narratives, vibe,
-- momentum, transfer heat) into one score + blurb; this adds three additive, NULLABLE
-- columns the synthesis model now emits alongside the blurb (prompt s10->s11 in the
-- Rust stage):
--   - convergence : how strongly the lenses agree (1-100).
--   - disagreement: a short summary of where the rails diverge.
--   - why_now     : a one-line breaking-news freshness note.
--
-- Additive + nullable, so every existing row stays valid. No backfill: the new fields
-- are prompt-only (NOT in the pillar input_hash), so nothing re-synthesizes on deploy —
-- rows populate lazily on their next real re-synthesis. A NULL value means the model did
-- not emit that field (a pre-s11 row, or an omitted field this run); the two are told
-- apart by prompt_version.
--
-- Deploy order: ADDITIVE — apply BEFORE the API restart. db.New prepares entity_sigil,
-- which now SELECTs these columns, and fail-fasts on a drifted schema (F-022).

BEGIN;

ALTER TABLE public.sigil_synthesis
    ADD COLUMN IF NOT EXISTS convergence smallint
        CHECK (convergence IS NULL OR (convergence >= 1 AND convergence <= 100)),
    ADD COLUMN IF NOT EXISTS disagreement text,
    ADD COLUMN IF NOT EXISTS why_now text;

COMMENT ON COLUMN public.sigil_synthesis.convergence IS
    'Phase 5.3 panel output: how strongly the lenses (PEAK/narrative/vibe/momentum/transfer) agree, 1-100. NULL = model did not emit it (pre-s11 row or omitted this run).';
COMMENT ON COLUMN public.sigil_synthesis.disagreement IS
    'Phase 5.3 panel output: short summary of where the rails diverge (e.g. strong stats vs negative narrative). NULL when the lenses agree or it was unstated.';
COMMENT ON COLUMN public.sigil_synthesis.why_now IS
    'Phase 5.3 panel output: one-line breaking-news freshness note explaining why the read moved now. NULL when nothing is fresh or it was unstated.';

INSERT INTO public.schema_migrations(version) VALUES ('143_sigil_disagreement_outputs')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.
