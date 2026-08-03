-- 209_fixtures_meta.sql
--
-- WHAT: fixtures.meta jsonb — the provenance/flag column PLAN-one-rail 4.4 names
-- ("upsert a fixture row ... flagged `meta needs_verification`"). The Editor's box-score
-- nomination writes {"needs_verification": true, "nominated_by": "editor", "article_id": N}
-- on rows it creates or corrects from a parsed result_line; the Investigator's promotion
-- (4.7) clears the flag when a validated box score lands.
--
-- WHY a new column: fixtures has no meta today; last_incomplete_reason is the seeder's
-- error channel and repurposing it would conflate two writers (the two-writer lesson, §1a).
--
-- Deploy order: ADDITIVE and INERT — nothing reads or writes it until the Phase 4.9 deploy.

BEGIN;

ALTER TABLE public.fixtures
    ADD COLUMN IF NOT EXISTS meta jsonb NOT NULL DEFAULT '{}'::jsonb;

COMMENT ON COLUMN public.fixtures.meta IS
    'Nomination/verification provenance (PLAN-one-rail 4.4): the Editor''s result_line '
    'nomination sets {"needs_verification": true, "nominated_by": "editor", "article_id"}; '
    'the Investigator''s validated promotion (4.7) sets needs_verification false. Never '
    'read by the legacy rail.';

INSERT INTO public.schema_migrations(version) VALUES ('209_fixtures_meta')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.
