-- 138_rename_sigil_synthesis_constraints.sql
--
-- C5 (DATA_FLOW_FRICTION_PRUNE_PLAN, Wave 2): finish the sigil rename.
--
-- 093_sigil_convergence_rename renamed the table vibe_synthesis → sigil_synthesis and
-- its indexes, but not its CONSTRAINTS. All 14 still carried vibe_synthesis_* names
-- (enumerated from pg_constraint on the live PG18 DB at authoring time, 2026-07-08 —
-- the plan's "7" was an undercount): 8 named NOT NULLs (PG18 gives NOT NULL
-- constraints real catalog entries, so RENAME CONSTRAINT applies — verified in a
-- rollback probe), 4 CHECKs, the pkey, and the sport fkey.
--
-- Pure cosmetic rename: constraint names appear in no query, so nothing at runtime
-- depends on them; this only removes a confusion trap for future schema work.

BEGIN;

ALTER TABLE public.sigil_synthesis RENAME CONSTRAINT vibe_synthesis_pkey                        TO sigil_synthesis_pkey;
ALTER TABLE public.sigil_synthesis RENAME CONSTRAINT vibe_synthesis_sport_fkey                  TO sigil_synthesis_sport_fkey;
ALTER TABLE public.sigil_synthesis RENAME CONSTRAINT vibe_synthesis_entity_type_check           TO sigil_synthesis_entity_type_check;
ALTER TABLE public.sigil_synthesis RENAME CONSTRAINT vibe_synthesis_previous_score_check        TO sigil_synthesis_previous_score_check;
ALTER TABLE public.sigil_synthesis RENAME CONSTRAINT vibe_synthesis_score_check                 TO sigil_synthesis_score_check;
ALTER TABLE public.sigil_synthesis RENAME CONSTRAINT vibe_synthesis_trigger_type_check          TO sigil_synthesis_trigger_type_check;
ALTER TABLE public.sigil_synthesis RENAME CONSTRAINT vibe_synthesis_id_not_null                 TO sigil_synthesis_id_not_null;
ALTER TABLE public.sigil_synthesis RENAME CONSTRAINT vibe_synthesis_entity_type_not_null        TO sigil_synthesis_entity_type_not_null;
ALTER TABLE public.sigil_synthesis RENAME CONSTRAINT vibe_synthesis_entity_id_not_null          TO sigil_synthesis_entity_id_not_null;
ALTER TABLE public.sigil_synthesis RENAME CONSTRAINT vibe_synthesis_sport_not_null              TO sigil_synthesis_sport_not_null;
ALTER TABLE public.sigil_synthesis RENAME CONSTRAINT vibe_synthesis_generated_at_not_null       TO sigil_synthesis_generated_at_not_null;
ALTER TABLE public.sigil_synthesis RENAME CONSTRAINT vibe_synthesis_input_components_not_null    TO sigil_synthesis_input_components_not_null;
ALTER TABLE public.sigil_synthesis RENAME CONSTRAINT vibe_synthesis_trigger_type_not_null       TO sigil_synthesis_trigger_type_not_null;
ALTER TABLE public.sigil_synthesis RENAME CONSTRAINT vibe_synthesis_trigger_payload_not_null    TO sigil_synthesis_trigger_payload_not_null;

INSERT INTO public.schema_migrations(version) VALUES ('138_rename_sigil_synthesis_constraints')
    ON CONFLICT DO NOTHING;

COMMIT;
