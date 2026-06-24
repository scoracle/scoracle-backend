-- NNN_short_description.sql
--
-- <One-paragraph WHAT + WHY. If you rebuild a function, derive it from the CURRENT
--  prod definition (\sf <fn> / pg_get_functiondef), never a possibly-stale canonical
--  file, then mirror the change into sql/{shared,nba,nfl,football}.sql.>
--
-- Deploy order: ADDITIVE migrations apply BEFORE the API restart (db.New prepares every
-- statement at boot and fail-fasts on a drifted schema). A column/param DROP inverts the
-- order — release the backward-compatible binary FIRST, then migrate (F-022).
--
-- NOT a migration the runner applies: this lives in sql/ (the runner globs
-- sql/migrations/*.sql). Copy it to sql/migrations/NNN_… to use it.

BEGIN;

-- 1. Schema change(s). Prefer idempotent DDL.
--    CREATE TABLE IF NOT EXISTS … ;
--    ALTER TABLE … ADD COLUMN IF NOT EXISTS … ;
--    CREATE OR REPLACE FUNCTION … ;

-- 2. (Data backfills only) add a parity/smoke gate that RAISES on mismatch — see 045.

-- 3. Self-record INSIDE this transaction so apply + record are atomic (a crash leaves
--    NEITHER applied nor recorded). REQUIRED — keep this the last statement before COMMIT.
INSERT INTO public.schema_migrations(version) VALUES ('NNN_short_description')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.
