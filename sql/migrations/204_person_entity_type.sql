-- 204_person_entity_type.sql
--
-- WHAT: extend entity_type CHECKs — `news_article_entities` and `entity_name_surfaces` admit
-- 'person'; `cognition_ledger` admits 'candidate' + 'fixture'; `pipeline_work` admits
-- 'candidate' (PLAN-one-rail.md 1.5).
--
-- WHY: the rail's resolver links persons the same way it links players and teams (exact nrm()
-- surface match), the Investigator's queue items are keyed 'candidate', and its ledger entries
-- span 'candidate' and 'fixture' work. Only what v1 writes is admitted — no speculative
-- admissions, and no other entity_type CHECK in the schema is touched (only these four tables
-- are on the rail).
--
-- Pattern per CHECK: DROP CONSTRAINT; ADD CONSTRAINT ... NOT VALID; VALIDATE CONSTRAINT.
-- Row counts here validate in milliseconds, but the pattern is the habit — VALIDATE takes only
-- a SHARE UPDATE EXCLUSIVE lock, so a big table never sits behind a full-table scan under
-- ACCESS EXCLUSIVE.
--
-- Deploy order: additive (constraints only widen; every existing row stays valid). Nothing
-- writes the new values until Phases 3–5 ship.

BEGIN;

-- These four tables are written by the LIVE rail. DROP/ADD CONSTRAINT queue for ACCESS
-- EXCLUSIVE; if a long writer holds the table, waiting would stall every later arrival
-- behind us. Fail fast instead — the transaction rolls back whole, retry in a rest window.
SET LOCAL lock_timeout = '5s';

-- news_article_entities: ('player','team') + person
ALTER TABLE public.news_article_entities
    DROP CONSTRAINT news_article_entities_entity_type_check;
ALTER TABLE public.news_article_entities
    ADD CONSTRAINT news_article_entities_entity_type_check
    CHECK (entity_type IN ('player','team','person')) NOT VALID;
ALTER TABLE public.news_article_entities
    VALIDATE CONSTRAINT news_article_entities_entity_type_check;

-- entity_name_surfaces: ('team','player') + person
ALTER TABLE public.entity_name_surfaces
    DROP CONSTRAINT entity_name_surfaces_entity_type_check;
ALTER TABLE public.entity_name_surfaces
    ADD CONSTRAINT entity_name_surfaces_entity_type_check
    CHECK (entity_type IN ('team','player','person')) NOT VALID;
ALTER TABLE public.entity_name_surfaces
    VALIDATE CONSTRAINT entity_name_surfaces_entity_type_check;

-- cognition_ledger: ('player','team','article') + candidate + fixture
-- (pair_entity_type's CHECK is deliberately untouched.)
ALTER TABLE public.cognition_ledger
    DROP CONSTRAINT cognition_ledger_entity_type_check;
ALTER TABLE public.cognition_ledger
    ADD CONSTRAINT cognition_ledger_entity_type_check
    CHECK (entity_type IN ('player','team','article','candidate','fixture')) NOT VALID;
ALTER TABLE public.cognition_ledger
    VALIDATE CONSTRAINT cognition_ledger_entity_type_check;

-- pipeline_work: ('player','team','article','fixture') + candidate
ALTER TABLE public.pipeline_work
    DROP CONSTRAINT pipeline_work_entity_type_check;
ALTER TABLE public.pipeline_work
    ADD CONSTRAINT pipeline_work_entity_type_check
    CHECK (entity_type IN ('player','team','article','fixture','candidate')) NOT VALID;
ALTER TABLE public.pipeline_work
    VALIDATE CONSTRAINT pipeline_work_entity_type_check;

INSERT INTO public.schema_migrations(version) VALUES ('204_person_entity_type')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.
