-- 135_drop_headlines.sql
--
-- C2 (DATA_FLOW_FRICTION_PRUNE_PLAN, Wave 2): drop the inert headlines table.
--
-- 121_fold_headlines_into_narratives.sql folded headlines into news_summaries and
-- left the table as "inert history" — but every INSERT/UPDATE still maintained five
-- indexes nobody reads (idx_headlines_category, idx_headlines_entity,
-- idx_headlines_published, and the two unique dedup indexes
-- idx_headlines_entity_source_url_uniq / idx_headlines_entity_source_title_uniq).
-- The /headlines API routes were retired (server_test.go asserts 404); no live Go or
-- Rust code reads the table.
--
-- CASCADE drops the table + its pkey + all five indexes in one shot.
--
-- Not archived: the 789 inert rows predate the mig-121 fold; the live narrative data
-- lives in news_summaries, so the history has no operator value worth a
-- headlines_archive table (plan C2: "archive first ONLY if the inert history has
-- operator value").

BEGIN;

DROP TABLE IF EXISTS public.headlines CASCADE;

INSERT INTO public.schema_migrations(version) VALUES ('135_drop_headlines')
    ON CONFLICT DO NOTHING;

COMMIT;
