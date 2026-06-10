-- ============================================================================
-- 059 — Cleanup after 058 (multi-scope percentiles)
--
-- Two pieces of post-058 drift:
--   1. The superseded single-scope block arities template_block(text,text,jsonb,jsonb)
--      and team_template_block(text,jsonb,jsonb) are dead — the live binary calls the
--      new p_scoped (jsonb) forms after the 058 restart. Drop them.
--   2. The per-sport profile views (nba/nfl/football . player/team) built a
--      `scoped_percentile_metadata` object from FLAT scoped_percentiles keys
--      (scope_type/scope_id/scope_name/_position_group/_sample_size) that 058's nested
--      {scope:{key:pct}} format removed — so it was always NULL, and the
--      `scoped_percentiles` column carried now-pointless `- 'key'` meta-strips. Rebuild
--      both columns: expose the nested object directly, and surface the list of
--      available cohort scopes instead of the obsolete single-scope identity. Nothing
--      consumes these columns (verified — frontend reads the sparkline blocks), so this
--      is purely de-drifting; CREATE OR REPLACE keeps the column names/types/order so no
--      API/prepared-statement change and no restart.
--
-- Apply with: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/059_cleanup_scope_views.sql
-- ============================================================================

BEGIN;

-- ── 1. Drop the dead single-scope block arities ─────────────────────────────
DROP FUNCTION IF EXISTS public.template_block(text, text, jsonb, jsonb);
DROP FUNCTION IF EXISTS public.team_template_block(text, jsonb, jsonb);

-- ── 2. Rebuild the profile views' two scope columns (all 3 sports) ───────────
-- Player views — identical shape across sports (only the sport filter differs).
CREATE OR REPLACE VIEW nba.player AS
SELECT
    ps.player_id AS id, ps.season, ps.league_id, ps.team_id, ps.position, ps.stats,
    ps.percentiles - '_position_group' - '_sample_size' AS percentiles,
    CASE WHEN ps.percentiles IS NOT NULL AND ps.percentiles->>'_position_group' IS NOT NULL
        THEN jsonb_build_object('position_group', ps.percentiles->>'_position_group',
            'sample_size', COALESCE((ps.percentiles->>'_sample_size')::int, 0)) END AS percentile_metadata,
    ps.scoped_percentiles AS scoped_percentiles,
    CASE WHEN ps.scoped_percentiles IS NOT NULL AND ps.scoped_percentiles <> '{}'::jsonb
        THEN jsonb_build_object('scopes',
            (SELECT COALESCE(jsonb_agg(k ORDER BY k), '[]'::jsonb) FROM jsonb_object_keys(ps.scoped_percentiles) k))
        END AS scoped_percentile_metadata,
    ps.updated_at AS stats_updated_at
FROM public.player_stats ps WHERE ps.sport = 'NBA';

CREATE OR REPLACE VIEW nfl.player AS
SELECT
    ps.player_id AS id, ps.season, ps.league_id, ps.team_id, ps.position, ps.stats,
    ps.percentiles - '_position_group' - '_sample_size' AS percentiles,
    CASE WHEN ps.percentiles IS NOT NULL AND ps.percentiles->>'_position_group' IS NOT NULL
        THEN jsonb_build_object('position_group', ps.percentiles->>'_position_group',
            'sample_size', COALESCE((ps.percentiles->>'_sample_size')::int, 0)) END AS percentile_metadata,
    ps.scoped_percentiles AS scoped_percentiles,
    CASE WHEN ps.scoped_percentiles IS NOT NULL AND ps.scoped_percentiles <> '{}'::jsonb
        THEN jsonb_build_object('scopes',
            (SELECT COALESCE(jsonb_agg(k ORDER BY k), '[]'::jsonb) FROM jsonb_object_keys(ps.scoped_percentiles) k))
        END AS scoped_percentile_metadata,
    ps.updated_at AS stats_updated_at
FROM public.player_stats ps WHERE ps.sport = 'NFL';

CREATE OR REPLACE VIEW football.player AS
SELECT
    ps.player_id AS id, ps.season, ps.league_id, ps.team_id, ps.position, ps.stats,
    ps.percentiles - '_position_group' - '_sample_size' AS percentiles,
    CASE WHEN ps.percentiles IS NOT NULL AND ps.percentiles->>'_position_group' IS NOT NULL
        THEN jsonb_build_object('position_group', ps.percentiles->>'_position_group',
            'sample_size', COALESCE((ps.percentiles->>'_sample_size')::int, 0)) END AS percentile_metadata,
    ps.scoped_percentiles AS scoped_percentiles,
    CASE WHEN ps.scoped_percentiles IS NOT NULL AND ps.scoped_percentiles <> '{}'::jsonb
        THEN jsonb_build_object('scopes',
            (SELECT COALESCE(jsonb_agg(k ORDER BY k), '[]'::jsonb) FROM jsonb_object_keys(ps.scoped_percentiles) k))
        END AS scoped_percentile_metadata,
    ps.updated_at AS stats_updated_at
FROM public.player_stats ps WHERE ps.sport = 'FOOTBALL';

-- Team views — nba/nfl share a shape; football adds a `league` json column.
CREATE OR REPLACE VIEW nba.team AS
SELECT
    t.id, t.name, t.short_code, t.logo_url, t.country, t.city,
    t.founded, t.league_id, t.conference, t.division, t.venue_name, t.venue_capacity,
    ts.season, ts.stats,
    ts.percentiles - '_sample_size' AS percentiles,
    CASE WHEN ts.percentiles IS NOT NULL AND ts.percentiles->>'_sample_size' IS NOT NULL
        THEN jsonb_build_object('sample_size', COALESCE((ts.percentiles->>'_sample_size')::int, 0)) END AS percentile_metadata,
    ts.scoped_percentiles AS scoped_percentiles,
    CASE WHEN ts.scoped_percentiles IS NOT NULL AND ts.scoped_percentiles <> '{}'::jsonb
        THEN jsonb_build_object('scopes',
            (SELECT COALESCE(jsonb_agg(k ORDER BY k), '[]'::jsonb) FROM jsonb_object_keys(ts.scoped_percentiles) k))
        END AS scoped_percentile_metadata,
    ts.updated_at AS stats_updated_at
FROM public.teams t
LEFT JOIN public.team_stats ts ON ts.team_id = t.id AND ts.sport = t.sport
WHERE t.sport = 'NBA';

CREATE OR REPLACE VIEW nfl.team AS
SELECT
    t.id, t.name, t.short_code, t.logo_url, t.country, t.city,
    t.founded, t.league_id, t.conference, t.division, t.venue_name, t.venue_capacity,
    ts.season, ts.stats,
    ts.percentiles - '_sample_size' AS percentiles,
    CASE WHEN ts.percentiles IS NOT NULL AND ts.percentiles->>'_sample_size' IS NOT NULL
        THEN jsonb_build_object('sample_size', COALESCE((ts.percentiles->>'_sample_size')::int, 0)) END AS percentile_metadata,
    ts.scoped_percentiles AS scoped_percentiles,
    CASE WHEN ts.scoped_percentiles IS NOT NULL AND ts.scoped_percentiles <> '{}'::jsonb
        THEN jsonb_build_object('scopes',
            (SELECT COALESCE(jsonb_agg(k ORDER BY k), '[]'::jsonb) FROM jsonb_object_keys(ts.scoped_percentiles) k))
        END AS scoped_percentile_metadata,
    ts.updated_at AS stats_updated_at
FROM public.teams t
LEFT JOIN public.team_stats ts ON ts.team_id = t.id AND ts.sport = t.sport
WHERE t.sport = 'NFL';

CREATE OR REPLACE VIEW football.team AS
SELECT
    t.id, t.name, t.short_code, t.logo_url, t.country, t.city,
    t.founded, t.league_id, t.venue_name, t.venue_capacity,
    CASE WHEN l.id IS NOT NULL THEN json_build_object(
        'id', l.id, 'name', l.name, 'country', l.country, 'logo_url', l.logo_url) END AS league,
    ts.season, ts.stats,
    ts.percentiles - '_sample_size' AS percentiles,
    CASE WHEN ts.percentiles IS NOT NULL AND ts.percentiles->>'_sample_size' IS NOT NULL
        THEN jsonb_build_object('sample_size', COALESCE((ts.percentiles->>'_sample_size')::int, 0)) END AS percentile_metadata,
    ts.scoped_percentiles AS scoped_percentiles,
    CASE WHEN ts.scoped_percentiles IS NOT NULL AND ts.scoped_percentiles <> '{}'::jsonb
        THEN jsonb_build_object('scopes',
            (SELECT COALESCE(jsonb_agg(k ORDER BY k), '[]'::jsonb) FROM jsonb_object_keys(ts.scoped_percentiles) k))
        END AS scoped_percentile_metadata,
    ts.updated_at AS stats_updated_at
FROM public.teams t
LEFT JOIN public.leagues l ON l.id = t.league_id
LEFT JOIN public.team_stats ts ON ts.team_id = t.id AND ts.sport = t.sport
WHERE t.sport = 'FOOTBALL';

-- ── 3. Gates ─────────────────────────────────────────────────────────────────
DO $$
DECLARE v_arities INT; v_meta BIGINT;
BEGIN
    -- the dead arities are gone (only the 5-arg template_block / 4-arg team_template_block remain)
    SELECT count(*) INTO v_arities FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
    WHERE n.nspname='public' AND ((p.proname='template_block' AND p.pronargs=4)
                               OR (p.proname='team_template_block' AND p.pronargs=3));
    IF v_arities <> 0 THEN RAISE EXCEPTION '059 gate FAIL: % dead block arities still present', v_arities; END IF;

    -- the rebuilt metadata surfaces the scope list (non-null for scoped NBA players)
    SELECT count(*) INTO v_meta FROM nba.player WHERE scoped_percentile_metadata->'scopes' IS NOT NULL;
    IF v_meta = 0 THEN RAISE EXCEPTION '059 gate FAIL: scoped_percentile_metadata.scopes empty'; END IF;
    RAISE NOTICE '059 OK: dead arities dropped; scoped_percentile_metadata.scopes populated for % NBA players', v_meta;
END $$;

INSERT INTO public.schema_migrations (version) VALUES ('059_cleanup_scope_views')
ON CONFLICT (version) DO NOTHING;

COMMIT;
