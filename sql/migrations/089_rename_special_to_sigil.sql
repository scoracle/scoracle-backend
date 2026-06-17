-- 089_rename_special_to_sigil.sql  (SUPERSEDED — now a self-healing no-op)
--
-- ORIGINAL INTENT (reversed 2026-06-16): rename the rating_specialist* columns
-- to rating_sigil* across player_stats, team_stats, and the event tables.
--
-- WHY REVERSED: the rating engine computes those columns by NAME inside many
-- CREATE OR REPLACE FUNCTIONs whose current definitions live in later migrations
-- (compute_rating → 067, compute_team_rating → 068, _compute_rating_bundle → 080,
-- the event starline/pct functions → 028/029/063, apply_rate_siblings → 054).
-- PL/pgSQL late-binds column names, so renaming the columns WITHOUT redefining
-- every one of those functions passes `go build`, the typecheck sweep, and even
-- db.New()'s prepared-statement validation — then fails on the next
-- finalize_fixture() with `column "rating_specialist" does not exist`, halting
-- all rating recomputes in production. A second gap: migration 089 only renamed
-- rating_specialist_pct on the event tables, never rating_specialist/_specialty,
-- which the entity_stats event series also reads.
--
-- DECISION: keep "Specialist" as the engine's internal column name and present
-- the "Sigil" surface by ALIASING in the read layer — go/internal/db/db.go and
-- ml/stat_commentary.go select `rating_specialist AS rating_sigil`, etc., and
-- remap the rating_modes JSON keys so the whole API contract is sigil-consistent.
-- This mirrors how `is_specialty` (the per-datapoint flag) stays engine-internal
-- while the card the user sees is "Sigil". Zero engine-function risk.
--
-- SELF-HEALING: if an earlier copy of this migration already renamed the columns
-- on some environment (e.g. a dev DB), rename them back so the engine functions
-- resolve again. On a clean prod DB (never renamed) every branch is a no-op.

DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM information_schema.columns
             WHERE table_schema='public' AND table_name='player_stats' AND column_name='rating_sigil') THEN
    ALTER TABLE public.player_stats RENAME COLUMN rating_sigil       TO rating_specialist;
    ALTER TABLE public.player_stats RENAME COLUMN rating_sigil_label TO rating_specialty;
    ALTER TABLE public.player_stats RENAME COLUMN rating_sigil_rank  TO rating_specialist_rank;
    ALTER TABLE public.player_stats RENAME COLUMN rating_sigil_score TO rating_specialist_score;
  END IF;

  IF EXISTS (SELECT 1 FROM information_schema.columns
             WHERE table_schema='public' AND table_name='team_stats' AND column_name='rating_sigil') THEN
    ALTER TABLE public.team_stats RENAME COLUMN rating_sigil       TO rating_specialist;
    ALTER TABLE public.team_stats RENAME COLUMN rating_sigil_label TO rating_specialty;
    ALTER TABLE public.team_stats RENAME COLUMN rating_sigil_rank  TO rating_specialist_rank;
    ALTER TABLE public.team_stats RENAME COLUMN rating_sigil_score TO rating_specialist_score;
  END IF;

  IF EXISTS (SELECT 1 FROM information_schema.columns
             WHERE table_schema='public' AND table_name='event_box_scores' AND column_name='rating_sigil_pct') THEN
    ALTER TABLE public.event_box_scores RENAME COLUMN rating_sigil_pct TO rating_specialist_pct;
  END IF;

  IF EXISTS (SELECT 1 FROM information_schema.columns
             WHERE table_schema='public' AND table_name='event_team_stats' AND column_name='rating_sigil_pct') THEN
    ALTER TABLE public.event_team_stats RENAME COLUMN rating_sigil_pct TO rating_specialist_pct;
  END IF;
END $$;
