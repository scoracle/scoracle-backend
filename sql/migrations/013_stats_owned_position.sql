-- 013_stats_owned_position.sql
--
-- Separate meta and stats. Position lives on the stats tables, never on
-- public.players. Profile views serve stats only — name/headshot/etc. live in
-- the frontend's local meta cache.
--
-- Background: two seeders were writing public.players.position via the same
-- upsert_player(): the per-game stats path (BDL /stats player embed) and the
-- meta path (BDL /players/{id}). The COALESCE(EXCLUDED.position, players.position)
-- meant whichever seeder ran last won, so percentile cohorts flip-flopped.
-- recalculate_percentiles() partitioned on players.position, so the clobber
-- silently corrupted partitions.
--
-- Fix is structural:
--   * Add position to event_box_scores (per-game) and player_stats (season
--     aggregate). Finalize_fixture flows it through with most-recent-non-null.
--   * recalculate_percentiles() reads ps.position directly — no JOIN to players.
--   * Drop players.position and players.detailed_position. Meta seeders stop
--     writing those fields (the columns no longer exist).
--   * Player profile views (nba.player, nfl.player, football.player) become
--     stats-only. The frontend hydrates meta from its local cache.
--
-- Apply with: psql "$DATABASE_PRIVATE_URL" -f sql/migrations/013_stats_owned_position.sql

BEGIN;

-- ============================================================================
-- 1. SCHEMA — add position columns to stats domain
-- ============================================================================

ALTER TABLE event_box_scores ADD COLUMN IF NOT EXISTS position TEXT;
ALTER TABLE player_stats     ADD COLUMN IF NOT EXISTS position TEXT;

CREATE INDEX IF NOT EXISTS idx_player_stats_position ON player_stats(sport, position);

-- ============================================================================
-- 2. BACKFILL — seed player_stats.position from players.position so percentile
-- cohorts don't reset to 'Unknown' until the next finalize_fixture overwrites.
-- ============================================================================

UPDATE player_stats ps
SET position = p.position
FROM players p
WHERE p.id = ps.player_id
  AND p.sport = ps.sport
  AND ps.position IS NULL
  AND p.position IS NOT NULL;

-- ============================================================================
-- 3. DROP everything that references players.position / players.detailed_position
-- before the column drop. Views + matviews get rebuilt below.
-- ============================================================================

DROP VIEW IF EXISTS nba.player;
DROP VIEW IF EXISTS nfl.player;
DROP VIEW IF EXISTS football.player;

DROP MATERIALIZED VIEW IF EXISTS nba.autofill_entities;
DROP MATERIALIZED VIEW IF EXISTS nfl.autofill_entities;
DROP MATERIALIZED VIEW IF EXISTS football.autofill_entities;

DROP INDEX IF EXISTS idx_players_position;
ALTER TABLE players DROP COLUMN IF EXISTS position;
ALTER TABLE players DROP COLUMN IF EXISTS detailed_position;

-- ============================================================================
-- 4. finalize_fixture — thread per-game position into player_stats aggregation
-- ============================================================================

CREATE OR REPLACE FUNCTION finalize_fixture(p_fixture_id INTEGER)
RETURNS TABLE (players_updated INTEGER, teams_updated INTEGER) AS $$
DECLARE
    v_sport TEXT;
    v_season INTEGER;
    v_league_id INTEGER;
    v_home_team_id INTEGER;
    v_away_team_id INTEGER;
    v_home_score INTEGER;
    v_away_score INTEGER;
    v_players INTEGER := 0;
    v_teams INTEGER := 0;
BEGIN
    SELECT f.sport, f.season, COALESCE(f.league_id, 0),
           f.home_team_id, f.away_team_id
    INTO v_sport, v_season, v_league_id, v_home_team_id, v_away_team_id
    FROM fixtures f WHERE f.id = p_fixture_id;

    IF v_sport IS NULL THEN
        RAISE EXCEPTION 'fixture % not found', p_fixture_id;
    END IF;

    IF v_sport = 'NBA' THEN
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, position, updated_at)
        SELECT
            e.player_id,
            'NBA',
            v_season,
            v_league_id,
            MAX(e.team_id) AS team_id,
            COALESCE(nba.aggregate_player_season(e.player_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            (array_agg(e.position ORDER BY e.id DESC) FILTER (WHERE e.position IS NOT NULL))[1] AS position,
            NOW()
        FROM event_box_scores e
        WHERE e.fixture_id = p_fixture_id
        GROUP BY e.player_id
        ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
            team_id  = EXCLUDED.team_id,
            stats    = EXCLUDED.stats,
            position = COALESCE(EXCLUDED.position, player_stats.position),
            updated_at = NOW();

        INSERT INTO team_stats (team_id, sport, season, league_id, stats, updated_at)
        SELECT
            t.team_id, 'NBA', v_season, v_league_id,
            COALESCE(nba.aggregate_team_season(t.team_id, v_season, v_league_id), '{}'::jsonb), NOW()
        FROM (
            SELECT DISTINCT team_id FROM event_team_stats WHERE fixture_id = p_fixture_id
            UNION SELECT DISTINCT home_team_id FROM fixtures WHERE id = p_fixture_id
            UNION SELECT DISTINCT away_team_id FROM fixtures WHERE id = p_fixture_id
        ) t
        ON CONFLICT (team_id, sport, season, league_id) DO UPDATE SET
            stats = EXCLUDED.stats, updated_at = NOW();

    ELSIF v_sport = 'NFL' THEN
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, position, updated_at)
        SELECT
            e.player_id,
            'NFL',
            v_season,
            v_league_id,
            MAX(e.team_id) AS team_id,
            COALESCE(nfl.aggregate_player_season(e.player_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            (array_agg(e.position ORDER BY e.id DESC) FILTER (WHERE e.position IS NOT NULL))[1] AS position,
            NOW()
        FROM event_box_scores e
        WHERE e.fixture_id = p_fixture_id
        GROUP BY e.player_id
        ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
            team_id  = EXCLUDED.team_id,
            stats    = EXCLUDED.stats,
            position = COALESCE(EXCLUDED.position, player_stats.position),
            updated_at = NOW();

        INSERT INTO team_stats (team_id, sport, season, league_id, stats, updated_at)
        SELECT
            t.team_id, 'NFL', v_season, v_league_id,
            COALESCE(nfl.aggregate_team_season(t.team_id, v_season, v_league_id), '{}'::jsonb), NOW()
        FROM (
            SELECT DISTINCT team_id FROM event_team_stats WHERE fixture_id = p_fixture_id
            UNION SELECT DISTINCT home_team_id FROM fixtures WHERE id = p_fixture_id
            UNION SELECT DISTINCT away_team_id FROM fixtures WHERE id = p_fixture_id
        ) t
        ON CONFLICT (team_id, sport, season, league_id) DO UPDATE SET
            stats = EXCLUDED.stats, updated_at = NOW();

    ELSIF v_sport = 'FOOTBALL' THEN
        INSERT INTO player_stats (player_id, sport, season, league_id, team_id, stats, position, updated_at)
        SELECT
            e.player_id,
            'FOOTBALL',
            v_season,
            v_league_id,
            MAX(e.team_id) AS team_id,
            COALESCE(football.aggregate_player_season(e.player_id, v_season, v_league_id), '{}'::jsonb) AS stats,
            (array_agg(e.position ORDER BY e.id DESC) FILTER (WHERE e.position IS NOT NULL))[1] AS position,
            NOW()
        FROM event_box_scores e
        WHERE e.fixture_id = p_fixture_id
        GROUP BY e.player_id
        ON CONFLICT (player_id, sport, season, league_id) DO UPDATE SET
            team_id  = EXCLUDED.team_id,
            stats    = EXCLUDED.stats,
            position = COALESCE(EXCLUDED.position, player_stats.position),
            updated_at = NOW();

        INSERT INTO team_stats (team_id, sport, season, league_id, stats, updated_at)
        SELECT
            t.team_id, 'FOOTBALL', v_season, v_league_id,
            COALESCE(football.aggregate_team_season(t.team_id, v_season, v_league_id), '{}'::jsonb), NOW()
        FROM (
            SELECT DISTINCT team_id FROM event_team_stats WHERE fixture_id = p_fixture_id
            UNION SELECT DISTINCT home_team_id FROM fixtures WHERE id = p_fixture_id
            UNION SELECT DISTINCT away_team_id FROM fixtures WHERE id = p_fixture_id
        ) t
        ON CONFLICT (team_id, sport, season, league_id) DO UPDATE SET
            stats = EXCLUDED.stats, updated_at = NOW();
    END IF;

    SELECT rp.players_updated, rp.teams_updated
    INTO v_players, v_teams
    FROM recalculate_percentiles(v_sport, v_season) rp;

    IF v_sport = 'NBA' THEN
        REFRESH MATERIALIZED VIEW CONCURRENTLY nba.autofill_entities;
    ELSIF v_sport = 'NFL' THEN
        REFRESH MATERIALIZED VIEW CONCURRENTLY nfl.autofill_entities;
    ELSIF v_sport = 'FOOTBALL' THEN
        REFRESH MATERIALIZED VIEW CONCURRENTLY football.autofill_entities;
    END IF;

    SELECT score INTO v_home_score FROM event_team_stats
    WHERE fixture_id = p_fixture_id AND team_id = v_home_team_id;
    SELECT score INTO v_away_score FROM event_team_stats
    WHERE fixture_id = p_fixture_id AND team_id = v_away_team_id;

    PERFORM mark_fixture_seeded(p_fixture_id, v_home_score, v_away_score);

    RETURN QUERY SELECT v_players, v_teams;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- 5. recalculate_percentiles — partition on player_stats.position; no JOIN players
-- ============================================================================

CREATE OR REPLACE FUNCTION recalculate_percentiles(
    p_sport TEXT,
    p_season INTEGER,
    p_inverse_stats TEXT[] DEFAULT ARRAY[]::TEXT[]
)
RETURNS TABLE (players_updated INTEGER, teams_updated INTEGER) AS $$
DECLARE
    v_players INTEGER := 0;
    v_teams INTEGER := 0;
    v_inverse TEXT[];
BEGIN
    SELECT array_agg(DISTINCT key_name) INTO v_inverse
    FROM (
        SELECT key_name FROM stat_definitions WHERE sport = p_sport AND is_inverse = true
        UNION
        SELECT unnest(p_inverse_stats)
    ) combined;
    v_inverse := COALESCE(v_inverse, ARRAY[]::TEXT[]);

    -- Player percentiles (sport-wide, partitioned by ps.position)
    WITH stat_keys AS (
        SELECT DISTINCT key FROM player_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    expanded AS (
        SELECT ps.player_id, COALESCE(ps.position, 'Unknown') AS position,
               sk.key AS stat_key, (ps.stats->>sk.key)::numeric AS stat_value
        FROM player_stats ps CROSS JOIN stat_keys sk
        WHERE ps.sport = p_sport AND ps.season = p_season
          AND ps.stats ? sk.key AND (ps.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT player_id, position, stat_key,
            CASE WHEN stat_key = ANY(v_inverse)
                THEN round((1.0 - percent_rank() OVER (PARTITION BY position, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
                ELSE round((percent_rank() OVER (PARTITION BY position, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
            END AS percentile,
            count(*) OVER (PARTITION BY position, stat_key) AS sample_size
        FROM expanded
    ),
    aggregated AS (
        SELECT player_id, position, max(sample_size) AS max_sample_size,
            jsonb_object_agg(stat_key, percentile) || jsonb_build_object('_position_group', position, '_sample_size', max(sample_size)) AS percentiles_json
        FROM ranked GROUP BY player_id, position
    )
    UPDATE player_stats ps SET percentiles = agg.percentiles_json, updated_at = NOW()
    FROM aggregated agg WHERE ps.player_id = agg.player_id AND ps.sport = p_sport AND ps.season = p_season;
    GET DIAGNOSTICS v_players = ROW_COUNT;

    -- Team percentiles (sport-wide, no position partition)
    WITH stat_keys AS (
        SELECT DISTINCT key FROM team_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    expanded AS (
        SELECT ts.team_id, sk.key AS stat_key, (ts.stats->>sk.key)::numeric AS stat_value
        FROM team_stats ts CROSS JOIN stat_keys sk
        WHERE ts.sport = p_sport AND ts.season = p_season AND ts.stats ? sk.key AND (ts.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT team_id, stat_key,
            CASE WHEN stat_key = ANY(v_inverse)
                THEN round((1.0 - percent_rank() OVER (PARTITION BY stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
                ELSE round((percent_rank() OVER (PARTITION BY stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
            END AS percentile,
            count(*) OVER (PARTITION BY stat_key) AS sample_size
        FROM expanded
    ),
    aggregated AS (
        SELECT team_id, jsonb_object_agg(stat_key, percentile) || jsonb_build_object('_sample_size', max(sample_size)) AS percentiles_json
        FROM ranked GROUP BY team_id
    )
    UPDATE team_stats ts SET percentiles = agg.percentiles_json, updated_at = NOW()
    FROM aggregated agg WHERE ts.team_id = agg.team_id AND ts.sport = p_sport AND ts.season = p_season;
    GET DIAGNOSTICS v_teams = ROW_COUNT;

    -- Player scoped percentiles (ps.position, league|conference)
    WITH stat_keys AS (
        SELECT DISTINCT key FROM player_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    player_meta AS (
        SELECT ps.player_id, ps.league_id,
            COALESCE(ps.position, 'Unknown') AS position,
            CASE WHEN p_sport = 'FOOTBALL' THEN 'league' ELSE 'conference' END AS scope_type,
            CASE WHEN p_sport = 'FOOTBALL' THEN ps.league_id::text
                 ELSE COALESCE(t.conference, 'Unknown') END AS scope_id,
            CASE WHEN p_sport = 'FOOTBALL' THEN COALESCE(l.name, 'Unknown')
                 ELSE COALESCE(t.conference, 'Unknown') END AS scope_name
        FROM player_stats ps
        LEFT JOIN teams t ON t.id = ps.team_id AND t.sport = ps.sport
        LEFT JOIN leagues l ON l.id = ps.league_id
        WHERE ps.sport = p_sport AND ps.season = p_season
    ),
    expanded AS (
        SELECT pm.player_id, pm.league_id, pm.position, pm.scope_type, pm.scope_id, pm.scope_name,
            sk.key AS stat_key, (ps.stats->>sk.key)::numeric AS stat_value
        FROM player_stats ps
        JOIN player_meta pm ON pm.player_id = ps.player_id AND pm.league_id = ps.league_id
        CROSS JOIN stat_keys sk
        WHERE ps.sport = p_sport AND ps.season = p_season
            AND ps.stats ? sk.key AND (ps.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT player_id, league_id, position, scope_type, scope_id, scope_name, stat_key,
            CASE WHEN stat_key = ANY(v_inverse)
                THEN round((1.0 - percent_rank() OVER (PARTITION BY position, scope_type, scope_id, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
                ELSE round((percent_rank() OVER (PARTITION BY position, scope_type, scope_id, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
            END AS percentile,
            count(*) OVER (PARTITION BY position, scope_type, scope_id, stat_key) AS sample_size
        FROM expanded
    ),
    aggregated AS (
        SELECT player_id, league_id, position, scope_type, scope_id, scope_name,
            jsonb_object_agg(stat_key, percentile)
                || jsonb_build_object(
                    '_position_group', position,
                    '_sample_size', max(sample_size),
                    'scope_type', scope_type,
                    'scope_id', scope_id,
                    'scope_name', scope_name
                ) AS scoped_json
        FROM ranked
        GROUP BY player_id, league_id, position, scope_type, scope_id, scope_name
    )
    UPDATE player_stats ps
    SET scoped_percentiles = agg.scoped_json
    FROM aggregated agg
    WHERE ps.player_id = agg.player_id
      AND ps.league_id = agg.league_id
      AND ps.sport = p_sport AND ps.season = p_season;

    -- Team scoped percentiles (league|conference, no position)
    WITH stat_keys AS (
        SELECT DISTINCT key FROM team_stats, jsonb_each(stats) AS kv(key, val)
        WHERE sport = p_sport AND season = p_season AND jsonb_typeof(val) = 'number' AND (val::text)::numeric != 0
    ),
    team_meta AS (
        SELECT ts.team_id, ts.league_id,
            CASE WHEN p_sport = 'FOOTBALL' THEN 'league' ELSE 'conference' END AS scope_type,
            CASE WHEN p_sport = 'FOOTBALL' THEN ts.league_id::text
                 ELSE COALESCE(t.conference, 'Unknown') END AS scope_id,
            CASE WHEN p_sport = 'FOOTBALL' THEN COALESCE(l.name, 'Unknown')
                 ELSE COALESCE(t.conference, 'Unknown') END AS scope_name
        FROM team_stats ts
        JOIN teams t ON t.id = ts.team_id AND t.sport = ts.sport
        LEFT JOIN leagues l ON l.id = ts.league_id
        WHERE ts.sport = p_sport AND ts.season = p_season
    ),
    expanded AS (
        SELECT tm.team_id, tm.league_id, tm.scope_type, tm.scope_id, tm.scope_name,
            sk.key AS stat_key, (ts.stats->>sk.key)::numeric AS stat_value
        FROM team_stats ts
        JOIN team_meta tm ON tm.team_id = ts.team_id AND tm.league_id = ts.league_id
        CROSS JOIN stat_keys sk
        WHERE ts.sport = p_sport AND ts.season = p_season
            AND ts.stats ? sk.key AND (ts.stats->>sk.key)::numeric != 0
    ),
    ranked AS (
        SELECT team_id, league_id, scope_type, scope_id, scope_name, stat_key,
            CASE WHEN stat_key = ANY(v_inverse)
                THEN round((1.0 - percent_rank() OVER (PARTITION BY scope_type, scope_id, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
                ELSE round((percent_rank() OVER (PARTITION BY scope_type, scope_id, stat_key ORDER BY stat_value ASC))::numeric * 100, 1)
            END AS percentile,
            count(*) OVER (PARTITION BY scope_type, scope_id, stat_key) AS sample_size
        FROM expanded
    ),
    aggregated AS (
        SELECT team_id, league_id, scope_type, scope_id, scope_name,
            jsonb_object_agg(stat_key, percentile)
                || jsonb_build_object(
                    '_sample_size', max(sample_size),
                    'scope_type', scope_type,
                    'scope_id', scope_id,
                    'scope_name', scope_name
                ) AS scoped_json
        FROM ranked
        GROUP BY team_id, league_id, scope_type, scope_id, scope_name
    )
    UPDATE team_stats ts
    SET scoped_percentiles = agg.scoped_json
    FROM aggregated agg
    WHERE ts.team_id = agg.team_id
      AND ts.league_id = agg.league_id
      AND ts.sport = p_sport AND ts.season = p_season;

    RETURN QUERY SELECT v_players, v_teams;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- 6. stat_leaders — read position from player_stats (no JOIN players)
-- ============================================================================

CREATE OR REPLACE FUNCTION nba.stat_leaders(
    p_season INTEGER, p_stat_name TEXT,
    p_limit INTEGER DEFAULT 25, p_position TEXT DEFAULT NULL,
    p_league_id INTEGER DEFAULT 0
)
RETURNS TABLE ("rank" BIGINT, player_id INTEGER, "position" TEXT, team_name TEXT, stat_value NUMERIC) AS $$
    SELECT
        ROW_NUMBER() OVER (ORDER BY stat.val DESC) AS rank,
        s.player_id, s.position, t.name AS team_name, stat.val AS stat_value
    FROM public.player_stats s
    CROSS JOIN LATERAL (SELECT (s.stats->>p_stat_name)::NUMERIC AS val) stat
    LEFT JOIN public.teams t ON s.team_id = t.id AND s.sport = t.sport
    WHERE s.sport = 'NBA' AND s.season = p_season AND s.league_id = p_league_id
      AND stat.val IS NOT NULL
      AND (p_position IS NULL OR s.position = p_position)
    ORDER BY stat.val DESC
    LIMIT p_limit;
$$ LANGUAGE sql STABLE;

COMMENT ON FUNCTION nba.stat_leaders IS
    'Returns top N NBA players by stat category with positional filtering. Position sourced from player_stats.';

CREATE OR REPLACE FUNCTION nfl.stat_leaders(
    p_season INTEGER, p_stat_name TEXT,
    p_limit INTEGER DEFAULT 25, p_position TEXT DEFAULT NULL,
    p_league_id INTEGER DEFAULT 0
)
RETURNS TABLE ("rank" BIGINT, player_id INTEGER, "position" TEXT, team_name TEXT, stat_value NUMERIC) AS $$
    SELECT
        ROW_NUMBER() OVER (ORDER BY stat.val DESC) AS rank,
        s.player_id, s.position, t.name AS team_name, stat.val AS stat_value
    FROM public.player_stats s
    CROSS JOIN LATERAL (SELECT (s.stats->>p_stat_name)::NUMERIC AS val) stat
    LEFT JOIN public.teams t ON s.team_id = t.id AND s.sport = t.sport
    WHERE s.sport = 'NFL' AND s.season = p_season AND s.league_id = p_league_id
      AND stat.val IS NOT NULL
      AND (p_position IS NULL OR s.position = p_position)
    ORDER BY stat.val DESC
    LIMIT p_limit;
$$ LANGUAGE sql STABLE;

CREATE OR REPLACE FUNCTION football.stat_leaders(
    p_season INTEGER, p_stat_name TEXT,
    p_limit INTEGER DEFAULT 25, p_position TEXT DEFAULT NULL,
    p_league_id INTEGER DEFAULT 0
)
RETURNS TABLE ("rank" BIGINT, player_id INTEGER, "position" TEXT, team_name TEXT, stat_value NUMERIC) AS $$
    SELECT
        ROW_NUMBER() OVER (ORDER BY stat.val DESC) AS rank,
        s.player_id, s.position, t.name AS team_name, stat.val AS stat_value
    FROM public.player_stats s
    CROSS JOIN LATERAL (SELECT (s.stats->>p_stat_name)::NUMERIC AS val) stat
    LEFT JOIN public.teams t ON s.team_id = t.id AND s.sport = t.sport
    WHERE s.sport = 'FOOTBALL' AND s.season = p_season AND s.league_id = p_league_id
      AND stat.val IS NOT NULL
      AND (p_position IS NULL OR s.position = p_position)
    ORDER BY stat.val DESC
    LIMIT p_limit;
$$ LANGUAGE sql STABLE;

-- ============================================================================
-- 7. Player profile views — stats-only (no JOIN to players or teams).
-- Frontend hydrates name/headshot/etc. from its local meta cache.
-- ============================================================================

CREATE VIEW nba.player AS
SELECT
    ps.player_id AS id,
    ps.season,
    ps.league_id,
    ps.team_id,
    ps.position,
    ps.stats,
    ps.percentiles - '_position_group' - '_sample_size' AS percentiles,
    CASE
        WHEN ps.percentiles IS NOT NULL AND ps.percentiles->>'_position_group' IS NOT NULL
        THEN jsonb_build_object(
            'position_group', ps.percentiles->>'_position_group',
            'sample_size', COALESCE((ps.percentiles->>'_sample_size')::int, 0)
        )
    END AS percentile_metadata,
    ps.scoped_percentiles - '_position_group' - '_sample_size' - 'scope_type' - 'scope_id' - 'scope_name' AS scoped_percentiles,
    CASE
        WHEN ps.scoped_percentiles IS NOT NULL AND ps.scoped_percentiles->>'scope_type' IS NOT NULL
        THEN jsonb_build_object(
            'scope_type', ps.scoped_percentiles->>'scope_type',
            'scope_id', ps.scoped_percentiles->>'scope_id',
            'scope_name', ps.scoped_percentiles->>'scope_name',
            'position_group', ps.scoped_percentiles->>'_position_group',
            'sample_size', COALESCE((ps.scoped_percentiles->>'_sample_size')::int, 0)
        )
    END AS scoped_percentile_metadata,
    ps.updated_at AS stats_updated_at
FROM public.player_stats ps
WHERE ps.sport = 'NBA';

COMMENT ON VIEW nba.player IS
    'NBA player profile — stats only. Name and other meta come from the frontend local cache.';

CREATE VIEW nfl.player AS
SELECT
    ps.player_id AS id,
    ps.season,
    ps.league_id,
    ps.team_id,
    ps.position,
    ps.stats,
    ps.percentiles - '_position_group' - '_sample_size' AS percentiles,
    CASE
        WHEN ps.percentiles IS NOT NULL AND ps.percentiles->>'_position_group' IS NOT NULL
        THEN jsonb_build_object(
            'position_group', ps.percentiles->>'_position_group',
            'sample_size', COALESCE((ps.percentiles->>'_sample_size')::int, 0)
        )
    END AS percentile_metadata,
    ps.scoped_percentiles - '_position_group' - '_sample_size' - 'scope_type' - 'scope_id' - 'scope_name' AS scoped_percentiles,
    CASE
        WHEN ps.scoped_percentiles IS NOT NULL AND ps.scoped_percentiles->>'scope_type' IS NOT NULL
        THEN jsonb_build_object(
            'scope_type', ps.scoped_percentiles->>'scope_type',
            'scope_id', ps.scoped_percentiles->>'scope_id',
            'scope_name', ps.scoped_percentiles->>'scope_name',
            'position_group', ps.scoped_percentiles->>'_position_group',
            'sample_size', COALESCE((ps.scoped_percentiles->>'_sample_size')::int, 0)
        )
    END AS scoped_percentile_metadata,
    ps.updated_at AS stats_updated_at
FROM public.player_stats ps
WHERE ps.sport = 'NFL';

COMMENT ON VIEW nfl.player IS
    'NFL player profile — stats only. Name and other meta come from the frontend local cache.';

CREATE VIEW football.player AS
SELECT
    ps.player_id AS id,
    ps.season,
    ps.league_id,
    ps.team_id,
    ps.position,
    ps.stats,
    ps.percentiles - '_position_group' - '_sample_size' AS percentiles,
    CASE
        WHEN ps.percentiles IS NOT NULL AND ps.percentiles->>'_position_group' IS NOT NULL
        THEN jsonb_build_object(
            'position_group', ps.percentiles->>'_position_group',
            'sample_size', COALESCE((ps.percentiles->>'_sample_size')::int, 0)
        )
    END AS percentile_metadata,
    ps.scoped_percentiles - '_position_group' - '_sample_size' - 'scope_type' - 'scope_id' - 'scope_name' AS scoped_percentiles,
    CASE
        WHEN ps.scoped_percentiles IS NOT NULL AND ps.scoped_percentiles->>'scope_type' IS NOT NULL
        THEN jsonb_build_object(
            'scope_type', ps.scoped_percentiles->>'scope_type',
            'scope_id', ps.scoped_percentiles->>'scope_id',
            'scope_name', ps.scoped_percentiles->>'scope_name',
            'position_group', ps.scoped_percentiles->>'_position_group',
            'sample_size', COALESCE((ps.scoped_percentiles->>'_sample_size')::int, 0)
        )
    END AS scoped_percentile_metadata,
    ps.updated_at AS stats_updated_at
FROM public.player_stats ps
WHERE ps.sport = 'FOOTBALL';

COMMENT ON VIEW football.player IS
    'Football player profile — stats only. Name and other meta come from the frontend local cache.';

-- ============================================================================
-- 8. Autofill materialized views — drop position/detailed_position columns
-- (they no longer exist on players; frontend uses its local meta cache).
-- ============================================================================

CREATE MATERIALIZED VIEW nba.autofill_entities AS
    SELECT
        p.id,
        'player'::text AS type,
        p.name,
        p.first_name,
        p.last_name,
        p.nationality,
        p.date_of_birth::text AS date_of_birth,
        p.height,
        p.weight,
        p.photo_url,
        p.team_id,
        NULL::int AS league_id,
        NULL::text AS league_name,
        t.short_code AS team_abbr,
        t.name AS team_name,
        t.logo_url AS team_logo_url,
        jsonb_build_array(
            LOWER(p.first_name),
            LOWER(p.last_name),
            LOWER(REPLACE(p.name, ' ', '')),
            LOWER(COALESCE(t.short_code, '')),
            LOWER(COALESCE(t.name, '')),
            unaccent(LOWER(p.first_name)),
            unaccent(LOWER(p.last_name)),
            unaccent(LOWER(REPLACE(p.name, ' ', ''))),
            unaccent(LOWER(COALESCE(t.name, '')))
        ) AS search_tokens,
        COALESCE(p.meta, '{}'::jsonb) || jsonb_build_object('display_name', p.name) AS meta
    FROM public.players p
    LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
    WHERE p.sport = 'NBA'
      AND (
          EXISTS (SELECT 1 FROM public.player_stats ps WHERE ps.player_id = p.id AND ps.sport = p.sport)
          OR (p.meta->>'draft_year')::int = (SELECT current_season FROM public.sports WHERE id = 'NBA')
      )
UNION ALL
    SELECT
        t.id,
        'team'::text AS type,
        t.name,
        NULL::text AS first_name,
        NULL::text AS last_name,
        t.country AS nationality,
        NULL::text AS date_of_birth,
        NULL::text AS height,
        NULL::text AS weight,
        t.logo_url AS photo_url,
        NULL::int AS team_id,
        NULL::int AS league_id,
        NULL::text AS league_name,
        t.short_code AS team_abbr,
        NULL::text AS team_name,
        NULL::text AS team_logo_url,
        jsonb_build_array(
            LOWER(REPLACE(t.name, ' ', '')),
            LOWER(t.short_code),
            LOWER(t.city),
            LOWER(t.country),
            unaccent(LOWER(REPLACE(t.name, ' ', ''))),
            unaccent(LOWER(t.city))
        ) AS search_tokens,
        jsonb_build_object(
            'display_name', t.name,
            'abbreviation', t.short_code,
            'city', t.city,
            'country', t.country,
            'conference', t.conference,
            'division', t.division,
            'founded', t.founded,
            'venue_name', t.venue_name,
            'venue_capacity', t.venue_capacity
        ) AS meta
    FROM public.teams t
    WHERE t.sport = 'NBA'
WITH DATA;

CREATE UNIQUE INDEX IF NOT EXISTS idx_nba_autofill_pk ON nba.autofill_entities (id, type);

CREATE MATERIALIZED VIEW nfl.autofill_entities AS
    SELECT
        p.id,
        'player'::text AS type,
        p.name,
        p.first_name,
        p.last_name,
        p.nationality,
        p.date_of_birth::text AS date_of_birth,
        p.height,
        p.weight,
        p.photo_url,
        p.team_id,
        NULL::int AS league_id,
        NULL::text AS league_name,
        t.short_code AS team_abbr,
        t.name AS team_name,
        t.logo_url AS team_logo_url,
        jsonb_build_array(
            LOWER(p.first_name),
            LOWER(p.last_name),
            LOWER(REPLACE(p.name, ' ', '')),
            LOWER(COALESCE(t.short_code, '')),
            LOWER(COALESCE(t.name, '')),
            unaccent(LOWER(p.first_name)),
            unaccent(LOWER(p.last_name)),
            unaccent(LOWER(REPLACE(p.name, ' ', ''))),
            unaccent(LOWER(COALESCE(t.name, '')))
        ) AS search_tokens,
        COALESCE(p.meta, '{}'::jsonb) || jsonb_build_object('display_name', p.name) AS meta
    FROM public.players p
    LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
    WHERE p.sport = 'NFL'
      AND (
          EXISTS (SELECT 1 FROM public.player_stats ps WHERE ps.player_id = p.id AND ps.sport = p.sport)
          OR p.meta->>'experience' ILIKE 'rookie%'
      )
UNION ALL
    SELECT
        t.id,
        'team'::text AS type,
        t.name,
        NULL::text AS first_name,
        NULL::text AS last_name,
        t.country AS nationality,
        NULL::text AS date_of_birth,
        NULL::text AS height,
        NULL::text AS weight,
        t.logo_url AS photo_url,
        NULL::int AS team_id,
        NULL::int AS league_id,
        NULL::text AS league_name,
        t.short_code AS team_abbr,
        NULL::text AS team_name,
        NULL::text AS team_logo_url,
        jsonb_build_array(
            LOWER(REPLACE(t.name, ' ', '')),
            LOWER(t.short_code),
            LOWER(t.city),
            LOWER(t.country),
            unaccent(LOWER(REPLACE(t.name, ' ', ''))),
            unaccent(LOWER(t.city))
        ) AS search_tokens,
        jsonb_build_object(
            'display_name', t.name,
            'abbreviation', t.short_code,
            'city', t.city,
            'country', t.country,
            'conference', t.conference,
            'division', t.division,
            'founded', t.founded,
            'venue_name', t.venue_name,
            'venue_capacity', t.venue_capacity
        ) AS meta
    FROM public.teams t
    WHERE t.sport = 'NFL'
WITH DATA;

CREATE UNIQUE INDEX IF NOT EXISTS idx_nfl_autofill_pk ON nfl.autofill_entities (id, type);

CREATE MATERIALIZED VIEW football.autofill_entities AS
    SELECT * FROM (
        SELECT DISTINCT ON (p.id)
            p.id,
            'player'::text AS type,
            p.name,
            p.first_name,
            p.last_name,
            p.nationality,
            p.date_of_birth::text AS date_of_birth,
            p.height,
            p.weight,
            p.photo_url,
            p.team_id,
            ps.league_id,
            l.name AS league_name,
            t.short_code AS team_abbr,
            t.name AS team_name,
            t.logo_url AS team_logo_url,
            jsonb_build_array(
                LOWER(p.first_name),
                LOWER(p.last_name),
                LOWER(REPLACE(p.name, ' ', '')),
                LOWER(COALESCE(t.short_code, '')),
                LOWER(COALESCE(t.name, '')),
                LOWER(COALESCE(l.name, '')),
                unaccent(LOWER(p.first_name)),
                unaccent(LOWER(p.last_name)),
                unaccent(LOWER(REPLACE(p.name, ' ', ''))),
                unaccent(LOWER(COALESCE(t.name, '')))
            ) AS search_tokens,
            jsonb_build_object(
                'display_name', p.name,
                'jersey_number', p.meta->>'jersey_number',
                'foot', p.meta->>'foot',
                'market_value', (p.meta->>'market_value')::bigint,
                'contract_until', p.meta->>'contract_until'
            ) AS meta
        FROM public.players p
        LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
        LEFT JOIN public.player_stats ps ON ps.player_id = p.id AND ps.sport = p.sport
        LEFT JOIN public.leagues l ON l.id = ps.league_id
        WHERE p.sport = 'FOOTBALL'
        ORDER BY p.id, ps.season DESC NULLS LAST
    ) football_players
UNION ALL
    SELECT * FROM (
        SELECT DISTINCT ON (t.id)
            t.id,
            'team'::text AS type,
            t.name,
            NULL::text AS first_name,
            NULL::text AS last_name,
            t.country AS nationality,
            NULL::text AS date_of_birth,
            NULL::text AS height,
            NULL::text AS weight,
            t.logo_url AS photo_url,
            NULL::int AS team_id,
            ts.league_id,
            l.name AS league_name,
            t.short_code AS team_abbr,
            NULL::text AS team_name,
            NULL::text AS team_logo_url,
            jsonb_build_array(
                LOWER(REPLACE(t.name, ' ', '')),
                LOWER(t.short_code),
                LOWER(t.city),
                LOWER(t.country),
                LOWER(COALESCE(l.name, '')),
                unaccent(LOWER(REPLACE(t.name, ' ', ''))),
                unaccent(LOWER(t.city))
            ) AS search_tokens,
            jsonb_build_object(
                'display_name', t.name,
                'abbreviation', t.short_code,
                'city', t.city,
                'country', t.country,
                'founded', t.founded,
                'venue_name', t.venue_name,
                'venue_capacity', t.venue_capacity
            ) AS meta
        FROM public.teams t
        LEFT JOIN public.team_stats ts ON ts.team_id = t.id AND ts.sport = t.sport
        LEFT JOIN public.leagues l ON l.id = ts.league_id
        WHERE t.sport = 'FOOTBALL'
        ORDER BY t.id, ts.season DESC NULLS LAST
    ) football_teams
WITH DATA;

CREATE UNIQUE INDEX IF NOT EXISTS idx_football_autofill_pk ON football.autofill_entities (id, type);

GRANT SELECT ON nba.autofill_entities, nfl.autofill_entities, football.autofill_entities TO web_anon, web_user;
GRANT SELECT ON nba.player, nfl.player, football.player TO web_anon, web_user;

-- ============================================================================
-- 9. Recalculate percentiles for every existing (sport, season) pair so the
-- new ps.position-based partitions take effect immediately. NOTIFY trigger
-- fires on percentile diffs, which is fine — partitions did shift.
-- ============================================================================

DO $$
DECLARE
    r RECORD;
BEGIN
    FOR r IN
        SELECT DISTINCT sport, season FROM player_stats
        UNION
        SELECT DISTINCT sport, season FROM team_stats
    LOOP
        PERFORM recalculate_percentiles(r.sport, r.season);
    END LOOP;
END $$;

COMMIT;
