-- 094_rename_divined_sigil_to_divined_peak.sql
--
-- Sigil convergence — the tail. Renames the Gemma-divined peak-strength label
-- column on stat_summaries from the legacy "sigil = strength" vocabulary to the
-- aligned "peak" vocabulary. Post-convergence, "sigil" means ONLY the crown (the
-- three-signal synthesis); this column is the Rating card's hero peak-skill label
-- (e.g. "Rim Protection"), so it becomes divined_peak — parallel to the D1
-- rating_specialist* → rating_peak* wire rename.
--
--   stat_summaries.divined_sigil → divined_peak   (the column was added in 090)
--
-- Safe as a plain RENAME COLUMN: no SQL function, trigger, or view references this
-- column (it is Gemma-WRITTEN, not engine-computed — verified), so there is no
-- late-bound PL/pgSQL surface to drift (contrast the engine rating columns — L1 of
-- the convergence plan, never physically renamed). The RENAME moves all existing
-- rows in place; no backfill.
--
-- Coordinated, same deploy, with:
--   * ml/rating.go  — writer: INSERT divined_peak, prompt marker SIGIL: → PEAK: (s5)
--   * ml/sigil.go   — reader + crown input-component key "divined_peak" (s4); pre-s4
--                     synthesis rows re-stamped to the new hash by
--                     `vibesynth --restamp-vocab` (no Gemma re-synthesis)
--   * db/db.go      — the /rating commentary row_to_json key follows the column name
--
-- Apply this migration BEFORE restarting the API: db.New prepares every statement
-- against the new column at boot, so a restart on the old schema fails fast.
--
-- Reverse (manual, if ever needed):
--   ALTER TABLE stat_summaries RENAME COLUMN divined_peak TO divined_sigil;

BEGIN;

DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'stat_summaries'
          AND column_name = 'divined_sigil'
    ) THEN
        ALTER TABLE stat_summaries RENAME COLUMN divined_sigil TO divined_peak;
    END IF;
END $$;

COMMENT ON COLUMN stat_summaries.divined_peak IS
    'Gemma-divined peak-strength label (e.g. "Rim Protection"), parsed from the rating prompt''s "PEAK: <label>" first line; the Rating card hero label. Was divined_sigil pre-convergence.';

COMMIT;
