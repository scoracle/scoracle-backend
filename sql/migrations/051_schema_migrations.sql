-- ============================================================================
-- 051_schema_migrations.sql
-- Migration TRACKING — the durable-foundation keystone from the 2026-06-09 audit.
--
-- Until now migrations were applied by hand with no record of what ran, which let
-- canonical shared.sql drift from the applied state (root cause of the 049→050
-- regression). This adds a schema_migrations table and BACKFILLS every migration
-- already applied to prod (001–050, plus the renamed 042a). From here, sql/migrate.sh
-- applies only un-applied migrations in lexical order and records each — so the applied
-- state is always known, and new frontends/envs are stood up by cloning the prod schema
-- (sql/build.sh, which carries this table over). See sql/README-migrations.md.
--
-- Apply with: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/051_schema_migrations.sql
-- (or, going forward: sql/migrate.sh)
-- ============================================================================

BEGIN;

CREATE TABLE IF NOT EXISTS public.schema_migrations (
    version    TEXT PRIMARY KEY,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE public.schema_migrations IS
    'Applied migration versions (file basename without .sql). Managed by sql/migrate.sh; '
    'backfilled for 001–051 in migration 051. Cloned to new envs by sql/build.sh.';

-- Backfill: every migration already applied to production, plus this one. ON CONFLICT
-- keeps it safe to re-run. (042a = the renamed auth migration; 042 = rating_modes.)
INSERT INTO public.schema_migrations (version) VALUES
    ('001_add_search_aliases'),
    ('002_add_twitter_cache'),
    ('003_metadata_queue_constraint_non_deferrable'),
    ('004_finalize_fixture_scores'),
    ('005_twitter_usage_telemetry'),
    ('006_news_corpus'),
    ('007_vibe_scores'),
    ('008_entity_tiers'),
    ('009_vibe_sentiment'),
    ('010_autofill_team_logo'),
    ('011_vibe_trigger_news_volume'),
    ('012_per_rate_and_scoped_percentiles'),
    ('013_stats_owned_position'),
    ('014_nfl_per_game'),
    ('015_nba_nfl_team_meta'),
    ('016_stat_unit_metadata'),
    ('017_event_percentiles_and_composite'),
    ('018_event_composite_normalization'),
    ('019_drop_per90_and_reset_stale'),
    ('020_composite_purity_and_outcomes'),
    ('021_season_composite_rank'),
    ('022_season_composite_rank_alltime'),
    ('023_decouple_alltime_rank'),
    ('024_alltime_rank_season_scope'),
    ('025_cold_start_guard'),
    ('026_player_absolute_rank'),
    ('027_rating_engine_z'),
    ('028_rating_engine_starline'),
    ('029_event_rating_percentiles'),
    ('030_rating_breakdown'),
    ('031_transfer_rumors'),
    ('032_transfer_heat'),
    ('033_comention_proximity'),
    ('034_transfer_trigger'),
    ('035_vibe_news_spike_trigger_type'),
    ('036_team_opposition_stats'),
    ('037_team_categories'),
    ('038_rating_breakdown_value'),
    ('039_rating_scoped_ranks'),
    ('040_penalties_rating'),
    ('041_penalties_won'),
    ('042a_auth_refresh_tokens'),
    ('042_rating_modes'),
    ('043_scoped_breakdown'),
    ('044_autofill_position'),
    ('045_uniform_scopes'),
    ('046_fantasy_points'),
    ('047_stat_templates'),
    ('048_backfill_nfl_position'),
    ('049_finalize_position_durability'),
    ('050_restore_finalize_recomputes'),
    ('051_schema_migrations')
ON CONFLICT (version) DO NOTHING;

COMMIT;
