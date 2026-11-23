BEGIN;
SET LOCAL lock_timeout='5s';
-- Incomplete replies may have usable prose but no numeric score or headline.
-- The typed score range and the identity of the reporting window still apply.
ALTER TABLE public.vibe_scores DROP CONSTRAINT vibe_daily_card_check,
 ADD CONSTRAINT vibe_daily_card_check CHECK (scoring_version IS NULL OR (
 input_hash IS NOT NULL AND evidence_cutoff IS NOT NULL
 AND reporting_start IS NOT NULL AND reporting_end IS NOT NULL
 AND reporting_start=reporting_date::timestamp AT TIME ZONE 'America/Detroit'
 AND reporting_end=(reporting_date+1)::timestamp AT TIME ZONE 'America/Detroit')) NOT VALID;
INSERT INTO public.schema_migrations(version) VALUES('342_vibe_output_review');
COMMIT;
