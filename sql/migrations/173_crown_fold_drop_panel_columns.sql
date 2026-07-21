-- 173_crown_fold_drop_panel_columns.sql
--
-- Cognition refactor, Phase 1 (the crown fold, 2026-07-21): the panel (SynthesisLogic) is
-- retired. The crown is now ONE call (the Oracle lens, Role::OracleLogic) that reads the five
-- pillar cards + the entity's own prior reads and emits {reading, score}; the READING is the
-- served voice. compute_omen + convergence are deterministic in code. That leaves three
-- panel-only columns with no writer and no reader: blurb (the panel's synthesis prose — never
-- served; the reading replaced it), disagreement, and why_now (they fold into the reading).
--
-- Deploy order (F-022 — a column DROP inverts the additive order): release the backward-
-- compatible binaries FIRST (the Rust crown stops WRITING these columns; the Go API stops
-- SELECTing them in entity_sigil + the leaderboard), THEN apply this migration. Applying before
-- the API restart would drift the prepared statements the OLD binary still needs.
--
-- The partial index idx_sigil_synthesis_sport_score has `blurb IS NOT NULL` in its predicate
-- (the leaderboard's scored-row marker). Repoint it to `reading IS NOT NULL` — the crown's new
-- marker (a no-pillar marker row carries a NULL reading) — BEFORE dropping the column it depends
-- on. Non-CONCURRENTLY: sigil_synthesis is append-only crown rows; the brief rebuild lock is fine.

BEGIN;

DROP INDEX IF EXISTS public.idx_sigil_synthesis_sport_score;
CREATE INDEX idx_sigil_synthesis_sport_score
    ON public.sigil_synthesis USING btree (sport, score DESC, generated_at DESC)
    WHERE ((score IS NOT NULL) AND (reading IS NOT NULL));

ALTER TABLE public.sigil_synthesis DROP COLUMN IF EXISTS disagreement;
ALTER TABLE public.sigil_synthesis DROP COLUMN IF EXISTS why_now;
ALTER TABLE public.sigil_synthesis DROP COLUMN IF EXISTS blurb;

COMMENT ON TABLE public.sigil_synthesis IS
    'The Sigil: the crown. The Oracle lens reads the five pillar cards (PEAK, Vibe, Momentum, '
    'narratives, transfers) + the entity''s own prior reads and emits one holistic score + reading '
    '(mig 173 crown fold — the panel blurb/disagreement/why_now were retired). omen + convergence '
    'are computed deterministically in code.';

INSERT INTO public.schema_migrations(version) VALUES ('173_crown_fold_drop_panel_columns')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.
