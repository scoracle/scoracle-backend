-- 152_sigil_voice_merge.sql
--
-- Session B of the sigil pipeline cleanup plan (2026-07-16): Oracle is the LENS of the
-- Sigil product, not a 7th stage. The voice folds into the sigil stage as an in-process
-- second step (decide → voice), and the decided card and its voice persist as ONE
-- sigil_synthesis row.
--
-- New columns (all nullable — markers and pre-merge rows carry NULL):
--   reading              the 2-4 sentence Oracle voice over this decided card
--   omen                 computed omen (closed set, same CHECK as oracle_readings)
--   voiced_score         the sigil score the CURRENT reading was drawn under — the
--                        re-voice rule's hysteresis baseline (North Star #8: re-voice
--                        only on omen flip / archetype band cross ±2pt / first reading)
--   voiced_at            when the CURRENT reading was actually drawn. A voice-skipped
--                        row CARRIES FORWARD reading/omen/voiced_score/voiced_at from
--                        the prior row, so voiced_at < generated_at is normal and the
--                        client's "drawn <date>" stays honest.
--   voice_model_version / voice_prompt_version — the voice call's provenance (the row's
--                        own model_version/prompt_version stay SynthesisLogic's).
--
-- oracle_readings: KEPT and still written (dual-write) while Go serving reads it; it
-- freezes read-only when Session C flips entity_sigil to the merged columns. Do not drop.
--
-- Deploy order: ADDITIVE — apply BEFORE the cognition-service deploy (the new binary's
-- sigil INSERT names these columns; the running binary ignores them). After the new
-- binary is live, sweep the retired queue stage:
--   DELETE FROM pipeline_work WHERE stage = 'oracle';
-- (run it post-restart so the old binary can't re-enqueue stragglers afterwards).

BEGIN;

ALTER TABLE public.sigil_synthesis ADD COLUMN IF NOT EXISTS reading TEXT;
ALTER TABLE public.sigil_synthesis ADD COLUMN IF NOT EXISTS omen TEXT;
ALTER TABLE public.sigil_synthesis ADD COLUMN IF NOT EXISTS voiced_score SMALLINT;
ALTER TABLE public.sigil_synthesis ADD COLUMN IF NOT EXISTS voiced_at TIMESTAMPTZ;
ALTER TABLE public.sigil_synthesis ADD COLUMN IF NOT EXISTS voice_model_version TEXT;
ALTER TABLE public.sigil_synthesis ADD COLUMN IF NOT EXISTS voice_prompt_version TEXT;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'sigil_synthesis_omen_check'
          AND conrelid = 'public.sigil_synthesis'::regclass
    ) THEN
        ALTER TABLE public.sigil_synthesis ADD CONSTRAINT sigil_synthesis_omen_check
            CHECK (omen IS NULL OR omen IN ('ascendant', 'steady', 'waning', 'crossroads'));
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conname = 'sigil_synthesis_voiced_score_check'
          AND conrelid = 'public.sigil_synthesis'::regclass
    ) THEN
        ALTER TABLE public.sigil_synthesis ADD CONSTRAINT sigil_synthesis_voiced_score_check
            CHECK (voiced_score IS NULL OR voiced_score BETWEEN 1 AND 100);
    END IF;
END $$;

COMMENT ON COLUMN public.sigil_synthesis.reading IS
    'Oracle voice of this decided card (2-4 sentences). NULL = marker or pre-merge row. '
    'Carried forward verbatim from the prior row when the re-voice rule skips the model.';
COMMENT ON COLUMN public.sigil_synthesis.omen IS
    'Computed omen the current reading was drawn under (closed set; code-computed, never model output).';
COMMENT ON COLUMN public.sigil_synthesis.voiced_score IS
    'Sigil score the current reading was drawn under — the re-voice hysteresis baseline (North Star #8).';
COMMENT ON COLUMN public.sigil_synthesis.voiced_at IS
    'When the current reading was actually drawn; older than generated_at on carried-forward rows.';

INSERT INTO public.schema_migrations(version) VALUES ('152_sigil_voice_merge')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.
