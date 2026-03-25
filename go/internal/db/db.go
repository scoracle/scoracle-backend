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

		// Data API (page-shaped resource routes)
		"nba_player_page": `SELECT json_build_object(
			'page', 'player',
			'sport', 'nba',
			'entity', row_to_json(p),
			'stat_definitions', COALESCE((
				SELECT json_agg(row_to_json(sd) ORDER BY sd.entity_type, sd.sort_order)
				FROM nba.stat_definitions sd
			), '[]'::json),
			'meta', json_build_object('season', p.season)
		)
		FROM nba.player p
		WHERE p.id = $1
		  AND ($2::int IS NULL OR p.season = $2)
		ORDER BY p.season DESC NULLS LAST
		LIMIT 1`,
		"nfl_player_page": `SELECT json_build_object(
			'page', 'player',
			'sport', 'nfl',
			'entity', row_to_json(p),
			'stat_definitions', COALESCE((
				SELECT json_agg(row_to_json(sd) ORDER BY sd.entity_type, sd.sort_order)
				FROM nfl.stat_definitions sd
			), '[]'::json),
			'meta', json_build_object('season', p.season)
		)
		FROM nfl.player p
		WHERE p.id = $1
		  AND ($2::int IS NULL OR p.season = $2)
		ORDER BY p.season DESC NULLS LAST
		LIMIT 1`,
		"football_player_page": `SELECT json_build_object(
			'page', 'player',
			'sport', 'football',
			'entity', row_to_json(p),
			'stat_definitions', COALESCE((
				SELECT json_agg(row_to_json(sd) ORDER BY sd.entity_type, sd.sort_order)
				FROM football.stat_definitions sd
			), '[]'::json),
			'meta', json_build_object('season', p.season, 'league_id', p.league_id)
		)
		FROM football.player p
		WHERE p.id = $1
		  AND ($2::int IS NULL OR p.season = $2)
		  AND ($3::int IS NULL OR p.league_id = $3)
		ORDER BY p.season DESC NULLS LAST
		LIMIT 1`,
		"nba_team_page": `SELECT json_build_object(
			'page', 'team',
			'sport', 'nba',
			'entity', row_to_json(t),
			'stat_definitions', COALESCE((
				SELECT json_agg(row_to_json(sd) ORDER BY sd.entity_type, sd.sort_order)
				FROM nba.stat_definitions sd
			), '[]'::json),
			'meta', json_build_object('season', t.season)
		)
		FROM nba.team t
		WHERE t.id = $1
		  AND ($2::int IS NULL OR t.season = $2)
		ORDER BY t.season DESC NULLS LAST
		LIMIT 1`,
		"nfl_team_page": `SELECT json_build_object(
			'page', 'team',
			'sport', 'nfl',
			'entity', row_to_json(t),
			'stat_definitions', COALESCE((
				SELECT json_agg(row_to_json(sd) ORDER BY sd.entity_type, sd.sort_order)
				FROM nfl.stat_definitions sd
			), '[]'::json),
			'meta', json_build_object('season', t.season)
		)
		FROM nfl.team t
		WHERE t.id = $1
		  AND ($2::int IS NULL OR t.season = $2)
		ORDER BY t.season DESC NULLS LAST
		LIMIT 1`,
		"football_team_page": `SELECT json_build_object(
			'page', 'team',
			'sport', 'football',
			'entity', row_to_json(t),
			'stat_definitions', COALESCE((
				SELECT json_agg(row_to_json(sd) ORDER BY sd.entity_type, sd.sort_order)
				FROM football.stat_definitions sd
			), '[]'::json),
			'meta', json_build_object('season', t.season, 'league_id', t.league_id)
		)
		FROM football.team t
		WHERE t.id = $1
		  AND ($2::int IS NULL OR t.season = $2)
		  AND ($3::int IS NULL OR t.league_id = $3)
		ORDER BY t.season DESC NULLS LAST
		LIMIT 1`,
		"nba_standings_page": `SELECT json_build_object(
			'page', 'standings',
			'sport', 'nba',
			'filters', json_build_object('season', $1::int, 'conference', $2::text, 'division', $3::text),
			'items', COALESCE((
				SELECT json_agg(row_to_json(t) ORDER BY t.win_pct DESC NULLS LAST)
				FROM nba.standings t
				WHERE t.season = $1::int
				  AND ($2::text IS NULL OR t.conference = $2)
				  AND ($3::text IS NULL OR t.division = $3)
			), '[]'::json)
		)`,
		"nfl_standings_page": `SELECT json_build_object(
			'page', 'standings',
			'sport', 'nfl',
			'filters', json_build_object('season', $1::int, 'conference', $2::text, 'division', $3::text),
			'items', COALESCE((
				SELECT json_agg(row_to_json(t) ORDER BY t.win_pct DESC NULLS LAST)
				FROM nfl.standings t
				WHERE t.season = $1::int
				  AND ($2::text IS NULL OR t.conference = $2)
				  AND ($3::text IS NULL OR t.division = $3)
			), '[]'::json)
		)`,
		"football_standings_page": `SELECT json_build_object(
			'page', 'standings',
			'sport', 'football',
			'filters', json_build_object('season', $1::int, 'league_id', $2::int),
			'items', COALESCE((
				SELECT json_agg(row_to_json(t) ORDER BY t.sort_points DESC NULLS LAST, t.sort_goal_diff DESC NULLS LAST)
				FROM football.standings t
				WHERE t.season = $1::int
				  AND ($2::int IS NULL OR t.league_id = $2)
			), '[]'::json)
		)`,
		"nba_leaders_page": `SELECT json_build_object(
			'page', 'leaders',
			'sport', 'nba',
			'filters', json_build_object('season', $1::int, 'stat', $2::text, 'limit', $3::int, 'position', $4::text, 'league_id', $5::int),
			'items', COALESCE((
				SELECT json_agg(row_to_json(t))
				FROM nba.stat_leaders($1::int, $2::text, $3::int, $4::text, $5::int) t
			), '[]'::json)
		)`,
		"nfl_leaders_page": `SELECT json_build_object(
			'page', 'leaders',
			'sport', 'nfl',
			'filters', json_build_object('season', $1::int, 'stat', $2::text, 'limit', $3::int, 'position', $4::text, 'league_id', $5::int),
			'items', COALESCE((
				SELECT json_agg(row_to_json(t))
				FROM nfl.stat_leaders($1::int, $2::text, $3::int, $4::text, $5::int) t
			), '[]'::json)
		)`,
		"football_leaders_page": `SELECT json_build_object(
			'page', 'leaders',
			'sport', 'football',
			'filters', json_build_object('season', $1::int, 'stat', $2::text, 'limit', $3::int, 'position', $4::text, 'league_id', $5::int),
			'items', COALESCE((
				SELECT json_agg(row_to_json(t))
				FROM football.stat_leaders($1::int, $2::text, $3::int, $4::text, $5::int) t
			), '[]'::json)
		)`,
		"nba_search_page": `SELECT json_build_object(
			'page', 'search',
			'sport', 'nba',
			'query', $1::text,
			'items', COALESCE((
				SELECT json_agg(row_to_json(t))
				FROM (
					SELECT *
					FROM nba.autofill_entities
					WHERE name ILIKE '%' || $1::text || '%'
					ORDER BY type, name
					LIMIT 20
				) t
			), '[]'::json)
		)`,
		"nfl_search_page": `SELECT json_build_object(
			'page', 'search',
			'sport', 'nfl',
			'query', $1::text,
			'items', COALESCE((
				SELECT json_agg(row_to_json(t))
				FROM (
					SELECT *
					FROM nfl.autofill_entities
					WHERE name ILIKE '%' || $1::text || '%'
					ORDER BY type, name
					LIMIT 20
				) t
			), '[]'::json)
		)`,
		"football_search_page": `SELECT json_build_object(
			'page', 'search',
			'sport', 'football',
			'query', $1::text,
			'items', COALESCE((
				SELECT json_agg(row_to_json(t))
				FROM (
					SELECT *
					FROM football.autofill_entities
					WHERE name ILIKE '%' || $1::text || '%'
					ORDER BY type, name
					LIMIT 20
				) t
			), '[]'::json)
		)`,
		"nba_stat_definitions_page": `SELECT json_build_object(
			'page', 'stat_definitions',
			'sport', 'nba',
			'entity_type', $1::text,
			'items', COALESCE((
				SELECT json_agg(row_to_json(t) ORDER BY t.entity_type, t.sort_order)
				FROM nba.stat_definitions t
				WHERE ($1::text IS NULL OR t.entity_type = $1)
			), '[]'::json)
		)`,
		"nfl_stat_definitions_page": `SELECT json_build_object(
			'page', 'stat_definitions',
			'sport', 'nfl',
			'entity_type', $1::text,
			'items', COALESCE((
				SELECT json_agg(row_to_json(t) ORDER BY t.entity_type, t.sort_order)
				FROM nfl.stat_definitions t
				WHERE ($1::text IS NULL OR t.entity_type = $1)
			), '[]'::json)
		)`,
		"football_stat_definitions_page": `SELECT json_build_object(
			'page', 'stat_definitions',
			'sport', 'football',
			'entity_type', $1::text,
			'items', COALESCE((
				SELECT json_agg(row_to_json(t) ORDER BY t.entity_type, t.sort_order)
				FROM football.stat_definitions t
				WHERE ($1::text IS NULL OR t.entity_type = $1)
			), '[]'::json)
		)`,
		"football_leagues_page": `SELECT json_build_object(
			'page', 'leagues',
			'sport', 'football',
			'filters', json_build_object('active', $1::bool, 'benchmark', $2::bool),
			'items', COALESCE((
				SELECT json_agg(row_to_json(t) ORDER BY t.name)
				FROM football.leagues t
				WHERE ($1::bool IS NULL OR t.is_active = $1)
				  AND ($2::bool IS NULL OR t.is_benchmark = $2)
			), '[]'::json)
		)`,

		// Autofill database endpoints (full entity metadata for frontend caching)
		"nba_autofill_page": `SELECT json_build_object(
			'page', 'autofill',
			'sport', 'nba',
			'items', COALESCE((
				SELECT json_agg(row_to_json(t) ORDER BY t.type, t.name)
				FROM nba.autofill_entities t
			), '[]'::json)
		)`,
		"nfl_autofill_page": `SELECT json_build_object(
			'page', 'autofill',
			'sport', 'nfl',
			'items', COALESCE((
				SELECT json_agg(row_to_json(t) ORDER BY t.type, t.name)
				FROM nfl.autofill_entities t
			), '[]'::json)
		)`,
		"football_autofill_page": `SELECT json_build_object(
			'page', 'autofill',
			'sport', 'football',
			'items', COALESCE((
				SELECT json_agg(row_to_json(t) ORDER BY t.type, t.name)
				FROM football.autofill_entities t
			), '[]'::json)
		)`,

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
