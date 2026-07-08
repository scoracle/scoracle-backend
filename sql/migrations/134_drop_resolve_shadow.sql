-- 134_drop_resolve_shadow.sql
--
-- C1 (DATA_FLOW_FRICTION_PRUNE_PLAN, Wave 2): drop the dead resolve_shadow table.
--
-- Of the six *_shadow parity tables, resolve_shadow is the only one that is truly
-- dead: its writer bin was deleted in e1bdcd5 (last write 2026-06-24, pre-cutover),
-- and grep across go/ and rust/ finds zero remaining references. The other five
-- shadows (vibe_scores_shadow, sigil_synthesis_shadow, transfer_rumors_shadow,
-- stat_summaries_shadow, news_summaries_shadow) STILL have active parity-bin writers
-- and stay in place — they retire together with the five bins in the post-Wave-3
-- parity-retirement PR (plan C1, DECIDED). Do NOT drop them here.

BEGIN;

DROP TABLE IF EXISTS public.resolve_shadow CASCADE;

INSERT INTO public.schema_migrations(version) VALUES ('134_drop_resolve_shadow')
    ON CONFLICT DO NOTHING;

COMMIT;
