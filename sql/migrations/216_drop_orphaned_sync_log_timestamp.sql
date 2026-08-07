-- 216_drop_orphaned_sync_log_timestamp.sql
--
-- Tail of migration 215. Dropping `metadata_sync_log` took its BEFORE UPDATE trigger
-- (`trg_sync_log_timestamp`) with it, but the trigger FUNCTION it called —
-- `update_sync_log_timestamp()` — is not owned by the table and survived the drop with
-- nothing left to fire it. 215 should have carried this; it did not, so it lands here
-- rather than being back-dated into an already-applied and already-recorded migration.
--
-- This is the same class of leftover the audit was chartered to find: an object that no
-- grep of the Rust or Go would ever surface, kept alive by nothing.
--
-- PROOF (2026-08-06, after 215 applied):
--
--   -- no trigger fires it:
--   SELECT p.proname, (SELECT count(*) FROM pg_trigger t
--                      WHERE t.tgfoid=p.oid AND NOT t.tgisinternal) AS triggers_using
--   FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
--   WHERE n.nspname='public' AND p.proname='update_sync_log_timestamp';
--   -> update_sync_log_timestamp | 0
--
--   -- no other function names it:
--   SELECT p.proname FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
--   WHERE n.nspname='public' AND p.prokind IN ('f','p') AND p.prolang<>12
--     AND p.proname <> 'update_sync_log_timestamp'
--     AND pg_get_functiondef(p.oid) ~* 'update_sync_log_timestamp';
--   -> (none)
--
--   -- no application code names it:
--   grep -rn update_sync_log_timestamp --include=*.rs --include=*.go --include=*.py --include=*.sh .
--   -> (no hits)
--
-- Deploy order: no application caller, so no release ordering applies.

BEGIN;

DROP FUNCTION IF EXISTS public.update_sync_log_timestamp();

INSERT INTO public.schema_migrations(version) VALUES ('216_drop_orphaned_sync_log_timestamp')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.
