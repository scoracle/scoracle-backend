-- 136_drop_idx_news_entities_lookup.sql
--
-- C3 (DATA_FLOW_FRICTION_PRUNE_PLAN, Wave 2): drop the redundant, never-used
-- idx_news_entities_lookup. DIRECTION INVERTED from plan v1.
--
-- Live evidence (pg_stat_user_indexes, never reset, 2026-07-08):
--   * idx_news_entities_lookup            (entity_type, entity_id, sport)
--       — ZERO lifetime scans. A strict prefix-subset of _created below, redundant
--         while that index exists.
--   * idx_news_entities_lookup_created    (entity_type, entity_id, sport, created_at)
--       — 2,752 scans / 439,500 tuples. Serves the LIVE transfers-stage query
--         load_candidates (transfer.rs:387, `... AND te.created_at > NOW() - INTERVAL
--         '14 days'`) on every drain. KEPT.
--   * idx_nae_vetted_lookup               (entity_type, entity_id, sport) WHERE vetted
--       — 29,711 scans. Serves the mig-103 enqueue function. KEPT.
--
-- EXPLAIN ANALYZE with real params, before AND after this drop, confirmed the planner
-- picks idx_nae_vetted_lookup for the enqueue function and a Bitmap Index Scan on
-- idx_news_entities_lookup_created for load_candidates — neither touched the dropped
-- index (it had zero scans, so the plans are unchanged by the drop).
--
-- Plain DROP INDEX (not CONCURRENTLY): the index is dead (0 scans) and this migration
-- self-records inside its own transaction, so atomicity is preserved. The brief
-- ACCESS EXCLUSIVE lock on news_article_entities is acceptable for a dead index.

BEGIN;

DROP INDEX IF EXISTS public.idx_news_entities_lookup;

INSERT INTO public.schema_migrations(version) VALUES ('136_drop_idx_news_entities_lookup')
    ON CONFLICT DO NOTHING;

COMMIT;
