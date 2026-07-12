-- 147_vibe_debounce.sql
--
-- F2 of the flow-friction plan (2026-07-12): give vibe a debounce. VibeHandler had no gate —
-- every claim was a GPU call, and vibe's temp-0.7 prose is what seeded the F1 cascade
-- (vibe re-run → momentum → sigil → oracle on zero material change). The Rust harness now
-- hashes vibe's MATERIAL inputs (latest narrative titles/impacts/trajectories + transfer-heat
-- facts; no prose, no timestamps) and skips the model call when the latest persisted row
-- carries the same hash. Same shape as momentum_summaries/news_summaries (migrations 144/145).
--
-- Nullable by design: pre-147 rows have NULL, which never matches, so each entity regenerates
-- exactly once after deploy and stamps its hash from then on. Marker (no-corpus) rows also
-- carry a real hash (the empty-material pre-image), so quiet entities debounce instead of
-- re-marking every cycle — the narratives (145) precedent.
--
-- vibe_scores_shadow gets the same column so the vibe parity harness (bin/parity) stays
-- column-compatible and records the new deterministic axis.

-- Deploy order: ADDITIVE — apply BEFORE the cognition-service deploy (the new binary's
-- vibe INSERT names the column; the running binary ignores it).

BEGIN;

ALTER TABLE public.vibe_scores ADD COLUMN IF NOT EXISTS input_hash TEXT;
ALTER TABLE public.vibe_scores_shadow ADD COLUMN IF NOT EXISTS input_hash TEXT;

-- The momentum/oracle partial-index pattern; vibe is entity-scoped (no season).
CREATE INDEX IF NOT EXISTS idx_vibe_scores_input_hash
    ON public.vibe_scores (entity_type, entity_id, sport, input_hash)
    WHERE input_hash IS NOT NULL;

COMMENT ON COLUMN public.vibe_scores.input_hash IS
    'SHA-256 (128-bit hex prefix) of the generation''s material inputs (latest narrative titles/impacts/trajectories + transfer-heat facts); the Rust harness debounce key. NULL on pre-147 rows.';
COMMENT ON COLUMN public.vibe_scores_shadow.input_hash IS
    'Parity capture of vibe_scores.input_hash (migration 147) — a deterministic diff axis.';

INSERT INTO public.schema_migrations(version) VALUES ('147_vibe_debounce')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.
