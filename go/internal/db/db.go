// Package db provides a pgxpool-based connection pool with prepared statement
// registration and health checking.
package db

import (
	"context"
	"fmt"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/config"
)

// Pool wraps pgxpool.Pool with application-specific helpers.
type Pool struct {
	*pgxpool.Pool
}

// New creates and validates a new connection pool.
func New(ctx context.Context, cfg *config.Config) (*Pool, error) {
	poolCfg, err := pgxpool.ParseConfig(cfg.DatabaseURL)
	if err != nil {
		return nil, fmt.Errorf("parse database URL: %w", err)
	}

	poolCfg.MinConns = int32(cfg.DBPoolMinConns)
	poolCfg.MaxConns = int32(cfg.DBPoolMaxConns)
	poolCfg.MaxConnLifetime = cfg.DBPoolMaxLife
	poolCfg.MaxConnIdleTime = 5 * time.Minute

	// Register prepared statements on every new connection.
	poolCfg.AfterConnect = func(ctx context.Context, conn *pgx.Conn) error {
		return registerPreparedStatements(ctx, conn)
	}

	pool, err := pgxpool.NewWithConfig(ctx, poolCfg)
	if err != nil {
		return nil, fmt.Errorf("create pool: %w", err)
	}

	// Verify connectivity
	if err := pool.Ping(ctx); err != nil {
		pool.Close()
		return nil, fmt.Errorf("ping database: %w", err)
	}

	return &Pool{Pool: pool}, nil
}

// registerPreparedStatements registers all statements the API and notification
// layers use. Prepared statements eliminate parse overhead on every request.
// Seeding-related statements have moved to the Python seeder (psycopg auto-prepares).
func registerPreparedStatements(ctx context.Context, conn *pgx.Conn) error {
	stmts := map[string]string{
		// Health
		"health_check": "SELECT 1",

		// Data API (canonical sport routes)
		"nba_profile_page": `WITH req AS (
			SELECT $1::text AS entity_type, $2::int AS entity_id, $3::int AS season, $4::int AS league_id
		),
		selected_entity AS (
			SELECT * FROM (
				SELECT row_to_json(p)::json AS entity, p.season, COALESCE(p.league_id, 0) AS league_id
				FROM nba.player p, req
				WHERE req.entity_type = 'player'
				  AND p.id = req.entity_id
				  AND (req.season IS NULL OR p.season = req.season)
				  AND (req.league_id IS NULL OR COALESCE(p.league_id, 0) = req.league_id)
				ORDER BY p.season DESC NULLS LAST
				LIMIT 1
			) player_entity
			UNION ALL
			SELECT * FROM (
				SELECT row_to_json(t)::json AS entity, t.season, COALESCE(t.league_id, 0) AS league_id
				FROM nba.team t, req
				WHERE req.entity_type = 'team'
				  AND t.id = req.entity_id
				  AND (req.season IS NULL OR t.season = req.season)
				  AND (req.league_id IS NULL OR COALESCE(t.league_id, 0) = req.league_id)
				ORDER BY t.season DESC NULLS LAST
				LIMIT 1
			) team_entity
		)
		SELECT json_build_object(
			'page', 'profile',
			'sport', 'nba',
			'entity_type', req.entity_type,
			'entity', se.entity,
			'stat_definitions', COALESCE((
				SELECT json_agg(row_to_json(sd) ORDER BY sd.entity_type, sd.sort_order)
				FROM nba.stat_definitions sd
			), '[]'::json),
			'meta', json_build_object(
				'season', se.season,
				'league_id', NULLIF(se.league_id, 0)
			),
			'league_context', CASE
				WHEN se.league_id > 0 THEN (
					SELECT row_to_json(lc)
					FROM (
						SELECT l.id, l.name, l.country, l.logo_url, l.is_benchmark, l.is_active
						FROM public.leagues l
						WHERE l.id = se.league_id AND l.sport = 'NBA'
					) lc
				)
				ELSE NULL
			END
		)
		FROM req
		JOIN selected_entity se ON true`,
		"nfl_profile_page": `WITH req AS (
			SELECT $1::text AS entity_type, $2::int AS entity_id, $3::int AS season, $4::int AS league_id
		),
		selected_entity AS (
			SELECT * FROM (
				SELECT row_to_json(p)::json AS entity, p.season, COALESCE(p.league_id, 0) AS league_id
				FROM nfl.player p, req
				WHERE req.entity_type = 'player'
				  AND p.id = req.entity_id
				  AND (req.season IS NULL OR p.season = req.season)
				  AND (req.league_id IS NULL OR COALESCE(p.league_id, 0) = req.league_id)
				ORDER BY p.season DESC NULLS LAST
				LIMIT 1
			) player_entity
			UNION ALL
			SELECT * FROM (
				SELECT row_to_json(t)::json AS entity, t.season, COALESCE(t.league_id, 0) AS league_id
				FROM nfl.team t, req
				WHERE req.entity_type = 'team'
				  AND t.id = req.entity_id
				  AND (req.season IS NULL OR t.season = req.season)
				  AND (req.league_id IS NULL OR COALESCE(t.league_id, 0) = req.league_id)
				ORDER BY t.season DESC NULLS LAST
				LIMIT 1
			) team_entity
		)
		SELECT json_build_object(
			'page', 'profile',
			'sport', 'nfl',
			'entity_type', req.entity_type,
			'entity', se.entity,
			'stat_definitions', COALESCE((
				SELECT json_agg(row_to_json(sd) ORDER BY sd.entity_type, sd.sort_order)
				FROM nfl.stat_definitions sd
			), '[]'::json),
			'meta', json_build_object(
				'season', se.season,
				'league_id', NULLIF(se.league_id, 0)
			),
			'league_context', CASE
				WHEN se.league_id > 0 THEN (
					SELECT row_to_json(lc)
					FROM (
						SELECT l.id, l.name, l.country, l.logo_url, l.is_benchmark, l.is_active
						FROM public.leagues l
						WHERE l.id = se.league_id AND l.sport = 'NFL'
					) lc
				)
				ELSE NULL
			END
		)
		FROM req
		JOIN selected_entity se ON true`,
		"football_profile_page": `WITH req AS (
			SELECT $1::text AS entity_type, $2::int AS entity_id, $3::int AS season, $4::int AS league_id
		),
		selected_entity AS (
			SELECT * FROM (
				SELECT row_to_json(p)::json AS entity, p.season, COALESCE(p.league_id, 0) AS league_id
				FROM football.player p, req
				WHERE req.entity_type = 'player'
				  AND p.id = req.entity_id
				  AND (req.season IS NULL OR p.season = req.season)
				  AND (req.league_id IS NULL OR COALESCE(p.league_id, 0) = req.league_id)
				ORDER BY p.season DESC NULLS LAST
				LIMIT 1
			) player_entity
			UNION ALL
			SELECT * FROM (
				SELECT row_to_json(t)::json AS entity, t.season, COALESCE(t.league_id, 0) AS league_id
				FROM football.team t, req
				WHERE req.entity_type = 'team'
				  AND t.id = req.entity_id
				  AND (req.season IS NULL OR t.season = req.season)
				  AND (req.league_id IS NULL OR COALESCE(t.league_id, 0) = req.league_id)
				ORDER BY t.season DESC NULLS LAST
				LIMIT 1
			) team_entity
		)
		SELECT json_build_object(
			'page', 'profile',
			'sport', 'football',
			'entity_type', req.entity_type,
			'entity', se.entity,
			'stat_definitions', COALESCE((
				SELECT json_agg(row_to_json(sd) ORDER BY sd.entity_type, sd.sort_order)
				FROM football.stat_definitions sd
			), '[]'::json),
			'meta', json_build_object(
				'season', se.season,
				'league_id', NULLIF(se.league_id, 0)
			),
			'league_context', CASE
				WHEN se.league_id > 0 THEN (
					SELECT row_to_json(lc)
					FROM (
						SELECT l.id, l.name, l.country, l.logo_url, l.is_benchmark, l.is_active
						FROM public.leagues l
						WHERE l.id = se.league_id AND l.sport = 'FOOTBALL'
					) lc
				)
				ELSE NULL
			END
		)
		FROM req
		JOIN selected_entity se ON true`,
		"nba_meta_page": `SELECT json_build_object(
			'page', 'meta',
			'sport', 'nba',
			'scope', json_build_object('league_id', $1::int),
			'meta_version', COALESCE((SELECT value FROM public.meta WHERE key = 'schema_version'), 'unknown'),
			'generated_at', NOW(),
			'items', COALESCE((
				SELECT json_agg(row_to_json(t) ORDER BY t.type, t.name)
				FROM nba.autofill_entities t
				WHERE ($1::int IS NULL OR COALESCE(t.league_id, 0) = $1::int)
			), '[]'::json),
			'stat_definitions', COALESCE((
				SELECT json_agg(row_to_json(sd) ORDER BY sd.entity_type, sd.sort_order)
				FROM nba.stat_definitions sd
			), '[]'::json),
			'leagues', COALESCE((
				SELECT json_agg(row_to_json(l) ORDER BY l.name)
				FROM (
					SELECT id, name, country, logo_url, is_benchmark, is_active
					FROM public.leagues
					WHERE sport = 'NBA'
					  AND ($1::int IS NULL OR id = $1::int)
				) l
			), '[]'::json)
		)`,
		"nfl_meta_page": `SELECT json_build_object(
			'page', 'meta',
			'sport', 'nfl',
			'scope', json_build_object('league_id', $1::int),
			'meta_version', COALESCE((SELECT value FROM public.meta WHERE key = 'schema_version'), 'unknown'),
			'generated_at', NOW(),
			'items', COALESCE((
				SELECT json_agg(row_to_json(t) ORDER BY t.type, t.name)
				FROM nfl.autofill_entities t
				WHERE ($1::int IS NULL OR COALESCE(t.league_id, 0) = $1::int)
			), '[]'::json),
			'stat_definitions', COALESCE((
				SELECT json_agg(row_to_json(sd) ORDER BY sd.entity_type, sd.sort_order)
				FROM nfl.stat_definitions sd
			), '[]'::json),
			'leagues', COALESCE((
				SELECT json_agg(row_to_json(l) ORDER BY l.name)
				FROM (
					SELECT id, name, country, logo_url, is_benchmark, is_active
					FROM public.leagues
					WHERE sport = 'NFL'
					  AND ($1::int IS NULL OR id = $1::int)
				) l
			), '[]'::json)
		)`,
		"football_meta_page": `SELECT json_build_object(
			'page', 'meta',
			'sport', 'football',
			'scope', json_build_object('league_id', $1::int),
			'meta_version', COALESCE((SELECT value FROM public.meta WHERE key = 'schema_version'), 'unknown'),
			'generated_at', NOW(),
			'items', COALESCE((
				SELECT json_agg(row_to_json(t) ORDER BY t.type, t.name)
				FROM football.autofill_entities t
				WHERE ($1::int IS NULL OR COALESCE(t.league_id, 0) = $1::int)
			), '[]'::json),
			'stat_definitions', COALESCE((
				SELECT json_agg(row_to_json(sd) ORDER BY sd.entity_type, sd.sort_order)
				FROM football.stat_definitions sd
			), '[]'::json),
			'leagues', COALESCE((
				SELECT json_agg(row_to_json(l) ORDER BY l.name)
				FROM football.leagues l
				WHERE ($1::int IS NULL OR l.id = $1::int)
			), '[]'::json)
		)`,
		"nba_health_page": `SELECT json_build_object(
			'page', 'health',
			'sport', 'nba',
			'scope', json_build_object('league_id', $1::int),
			'status', CASE WHEN health.player_stats_count + health.team_stats_count > 0 THEN 'healthy' ELSE 'degraded' END,
			'counts', json_build_object(
				'player_stats', health.player_stats_count,
				'team_stats', health.team_stats_count
			),
			'freshness', json_build_object(
				'player_stats_updated_at', health.player_stats_updated_at,
				'team_stats_updated_at', health.team_stats_updated_at,
				'latest_updated_at', GREATEST(
					COALESCE(health.player_stats_updated_at, to_timestamp(0)),
					COALESCE(health.team_stats_updated_at, to_timestamp(0))
				)
			),
			'league_context', CASE
				WHEN $1::int IS NOT NULL THEN (
					SELECT row_to_json(lc)
					FROM (
						SELECT id, name, country, logo_url, is_benchmark, is_active
						FROM public.leagues
						WHERE id = $1::int AND sport = 'NBA'
					) lc
				)
				ELSE NULL
			END
		)
		FROM (
			SELECT
				(SELECT COUNT(*)::int
				 FROM public.player_stats ps
				 WHERE ps.sport = 'NBA'
				   AND ($1::int IS NULL OR ps.league_id = $1::int)) AS player_stats_count,
				(SELECT COUNT(*)::int
				 FROM public.team_stats ts
				 WHERE ts.sport = 'NBA'
				   AND ($1::int IS NULL OR ts.league_id = $1::int)) AS team_stats_count,
				(SELECT MAX(ps.updated_at)
				 FROM public.player_stats ps
				 WHERE ps.sport = 'NBA'
				   AND ($1::int IS NULL OR ps.league_id = $1::int)) AS player_stats_updated_at,
				(SELECT MAX(ts.updated_at)
				 FROM public.team_stats ts
				 WHERE ts.sport = 'NBA'
				   AND ($1::int IS NULL OR ts.league_id = $1::int)) AS team_stats_updated_at
		) health`,
		"nfl_health_page": `SELECT json_build_object(
			'page', 'health',
			'sport', 'nfl',
			'scope', json_build_object('league_id', $1::int),
			'status', CASE WHEN health.player_stats_count + health.team_stats_count > 0 THEN 'healthy' ELSE 'degraded' END,
			'counts', json_build_object(
				'player_stats', health.player_stats_count,
				'team_stats', health.team_stats_count
			),
			'freshness', json_build_object(
				'player_stats_updated_at', health.player_stats_updated_at,
				'team_stats_updated_at', health.team_stats_updated_at,
				'latest_updated_at', GREATEST(
					COALESCE(health.player_stats_updated_at, to_timestamp(0)),
					COALESCE(health.team_stats_updated_at, to_timestamp(0))
				)
			),
			'league_context', CASE
				WHEN $1::int IS NOT NULL THEN (
					SELECT row_to_json(lc)
					FROM (
						SELECT id, name, country, logo_url, is_benchmark, is_active
						FROM public.leagues
						WHERE id = $1::int AND sport = 'NFL'
					) lc
				)
				ELSE NULL
			END
		)
		FROM (
			SELECT
				(SELECT COUNT(*)::int
				 FROM public.player_stats ps
				 WHERE ps.sport = 'NFL'
				   AND ($1::int IS NULL OR ps.league_id = $1::int)) AS player_stats_count,
				(SELECT COUNT(*)::int
				 FROM public.team_stats ts
				 WHERE ts.sport = 'NFL'
				   AND ($1::int IS NULL OR ts.league_id = $1::int)) AS team_stats_count,
				(SELECT MAX(ps.updated_at)
				 FROM public.player_stats ps
				 WHERE ps.sport = 'NFL'
				   AND ($1::int IS NULL OR ps.league_id = $1::int)) AS player_stats_updated_at,
				(SELECT MAX(ts.updated_at)
				 FROM public.team_stats ts
				 WHERE ts.sport = 'NFL'
				   AND ($1::int IS NULL OR ts.league_id = $1::int)) AS team_stats_updated_at
		) health`,
		"football_health_page": `SELECT json_build_object(
			'page', 'health',
			'sport', 'football',
			'scope', json_build_object('league_id', $1::int),
			'status', CASE WHEN health.player_stats_count + health.team_stats_count > 0 THEN 'healthy' ELSE 'degraded' END,
			'counts', json_build_object(
				'player_stats', health.player_stats_count,
				'team_stats', health.team_stats_count
			),
			'freshness', json_build_object(
				'player_stats_updated_at', health.player_stats_updated_at,
				'team_stats_updated_at', health.team_stats_updated_at,
				'latest_updated_at', GREATEST(
					COALESCE(health.player_stats_updated_at, to_timestamp(0)),
					COALESCE(health.team_stats_updated_at, to_timestamp(0))
				)
			),
			'league_context', CASE
				WHEN $1::int IS NOT NULL THEN (
					SELECT row_to_json(lc)
					FROM (
						SELECT id, name, country, logo_url, is_benchmark, is_active
						FROM public.leagues
						WHERE id = $1::int AND sport = 'FOOTBALL'
					) lc
				)
				ELSE NULL
			END
		)
		FROM (
			SELECT
				(SELECT COUNT(*)::int
				 FROM public.player_stats ps
				 WHERE ps.sport = 'FOOTBALL'
				   AND ($1::int IS NULL OR ps.league_id = $1::int)) AS player_stats_count,
				(SELECT COUNT(*)::int
				 FROM public.team_stats ts
				 WHERE ts.sport = 'FOOTBALL'
				   AND ($1::int IS NULL OR ts.league_id = $1::int)) AS team_stats_count,
				(SELECT MAX(ps.updated_at)
				 FROM public.player_stats ps
				 WHERE ps.sport = 'FOOTBALL'
				   AND ($1::int IS NULL OR ps.league_id = $1::int)) AS player_stats_updated_at,
				(SELECT MAX(ts.updated_at)
				 FROM public.team_stats ts
				 WHERE ts.sport = 'FOOTBALL'
				   AND ($1::int IS NULL OR ts.league_id = $1::int)) AS team_stats_updated_at
		) health`,

		// Entity name lookup (news handlers + notifications)
		"team_name_lookup": "SELECT name FROM teams WHERE id = $1 AND sport = $2",

		// Notifications (used by listener + notification pipeline)
		"get_entity_followers":     "SELECT uf.user_id, u.timezone FROM user_follows uf JOIN users u ON u.id = uf.user_id WHERE uf.entity_type = $1 AND uf.entity_id = $2 AND uf.sport = $3",
		"notification_player_name": "SELECT name FROM players WHERE id = $1 AND sport = $2",
		"stat_display_name":        "SELECT display_name FROM stat_definitions WHERE sport = $1 AND key_name = $2 AND entity_type = $3",
		"get_user_device_tokens":   "SELECT token FROM user_devices WHERE user_id = $1 AND is_active = true",
	}

	for name, sql := range stmts {
		if _, err := conn.Prepare(ctx, name, sql); err != nil {
			return fmt.Errorf("prepare %q: %w", name, err)
		}
	}
	return nil
}
