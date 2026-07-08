-- 139_drop_news_summaries_source_attribution.sql
--
-- C7 (DATA_FLOW_FRICTION_PRUNE_PLAN, Wave 2): drop the dead
-- news_summaries.source_attribution column.
--
-- The column is dead as DATA — 0 of 27,541 rows non-NULL. Both writers bind it to a
-- literal NULL (rust narratives.rs persist_narratives, mirroring the retired Go
-- news_narratives.go). It has no twin in news_summaries_shadow. But it WAS read live:
-- Go's entity_news profile statement selected ns.source_attribution (db.go:972, :1004)
-- and surfaced it in the JSON payload (always as null). No client reads the JSON key
-- (grep across go/ *.ts *.tsx *.js *.json finds zero non-db.go references).
--
-- Grep trap (NOT this column): transfer_rumors.source_attribution is a DIFFERENT, LIVE
-- column (db.go:820/842/1049/1073/1097, rust transfer.rs:985, transfer_parity.rs:256).
-- Untouched here.
--
-- DEPLOY ORDER — this is a column DROP, so the backward-compatible binaries ship
-- FIRST, then this migration (template F-022 inversion). Both live binaries reference
-- the column and were rebuilt + restarted (scripts/hosting/release.sh) BEFORE this
-- migration was applied:
--   * rust narratives.rs — removed source_attribution from the news_summaries INSERT.
--   * go db.go entity_news — removed ns.source_attribution from the SELECT + final
--     projection.
-- Applying this against the OLD binaries would error at runtime (the Go prepared
-- statement re-plans to "column does not exist"; the Rust INSERT fails the same way).

BEGIN;

ALTER TABLE public.news_summaries DROP COLUMN source_attribution;

INSERT INTO public.schema_migrations(version) VALUES ('139_drop_news_summaries_source_attribution')
    ON CONFLICT DO NOTHING;

COMMIT;
