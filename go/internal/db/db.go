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
			'league_context', NULL
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
			'league_context', NULL
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
						FROM football.leagues l
						WHERE l.id = se.league_id
					) lc
				)
				ELSE NULL
			END
		)
		FROM req
		JOIN selected_entity se ON true`,
		"nba_meta_page": `WITH meta_info AS (
			SELECT
				GREATEST(
					COALESCE((SELECT MAX(updated_at) FROM public.players WHERE sport = 'NBA'), '1970-01-01'::timestamptz),
					COALESCE((SELECT MAX(updated_at) FROM public.teams WHERE sport = 'NBA'), '1970-01-01'::timestamptz)
				) AS last_updated,
				(SELECT current_season FROM public.sports WHERE id = 'NBA') AS current_season,
				(SELECT COUNT(*)::int FROM nba.autofill_entities) AS total_entities
		)
		SELECT json_build_object(
			'page', 'meta',
			'sport', 'nba',
			'scope', json_build_object('league_id', $1::int),
			'meta_version', (SELECT EXTRACT(EPOCH FROM last_updated)::text FROM meta_info),
			'generated_at', NOW(),
			'current_season', (SELECT current_season FROM meta_info),
			'total_entities', (SELECT total_entities FROM meta_info),
			'items', COALESCE((
				SELECT json_agg(row_to_json(t) ORDER BY t.type, t.name)
				FROM nba.autofill_entities t
			), '[]'::json),
			'stat_definitions', COALESCE((
				SELECT json_agg(row_to_json(sd) ORDER BY sd.entity_type, sd.sort_order)
				FROM nba.stat_definitions sd
			), '[]'::json),
			'leagues', '[]'::json
		)`,
		"nfl_meta_page": `WITH meta_info AS (
			SELECT
				GREATEST(
					COALESCE((SELECT MAX(updated_at) FROM public.players WHERE sport = 'NFL'), '1970-01-01'::timestamptz),
					COALESCE((SELECT MAX(updated_at) FROM public.teams WHERE sport = 'NFL'), '1970-01-01'::timestamptz)
				) AS last_updated,
				(SELECT current_season FROM public.sports WHERE id = 'NFL') AS current_season,
				(SELECT COUNT(*)::int FROM nfl.autofill_entities) AS total_entities
		)
		SELECT json_build_object(
			'page', 'meta',
			'sport', 'nfl',
			'scope', json_build_object('league_id', $1::int),
			'meta_version', (SELECT EXTRACT(EPOCH FROM last_updated)::text FROM meta_info),
			'generated_at', NOW(),
			'current_season', (SELECT current_season FROM meta_info),
			'total_entities', (SELECT total_entities FROM meta_info),
			'items', COALESCE((
				SELECT json_agg(row_to_json(t) ORDER BY t.type, t.name)
				FROM nfl.autofill_entities t
			), '[]'::json),
			'stat_definitions', COALESCE((
				SELECT json_agg(row_to_json(sd) ORDER BY sd.entity_type, sd.sort_order)
				FROM nfl.stat_definitions sd
			), '[]'::json),
			'leagues', '[]'::json
		)`,
		"football_meta_page": `WITH meta_info AS (
			SELECT
				GREATEST(
					COALESCE((SELECT MAX(updated_at) FROM public.players WHERE sport = 'FOOTBALL'), '1970-01-01'::timestamptz),
					COALESCE((SELECT MAX(updated_at) FROM public.teams WHERE sport = 'FOOTBALL'), '1970-01-01'::timestamptz)
				) AS last_updated,
				(SELECT current_season FROM public.sports WHERE id = 'FOOTBALL') AS current_season,
				(SELECT COUNT(*)::int FROM football.autofill_entities 
				 WHERE ($1::int IS NULL OR COALESCE(league_id, 0) = $1::int)) AS total_entities
		)
		SELECT json_build_object(
			'page', 'meta',
			'sport', 'football',
			'scope', json_build_object('league_id', $1::int),
			'meta_version', (SELECT EXTRACT(EPOCH FROM last_updated)::text FROM meta_info),
			'generated_at', NOW(),
			'current_season', (SELECT current_season FROM meta_info),
			'total_entities', (SELECT total_entities FROM meta_info),
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
			'status', CASE WHEN health.player_profiles + health.team_profiles > 0 THEN 'healthy' ELSE 'degraded' END,
			'counts', json_build_object(
				'player_profiles', health.player_profiles,
				'team_profiles', health.team_profiles
			),
			'freshness', json_build_object(
				'player_stats_updated_at', NULL,
				'team_stats_updated_at', NULL,
				'latest_updated_at', NULL
			),
			'league_context', NULL
		)
		FROM (
			SELECT
				(SELECT COUNT(*)::int FROM nba.player) AS player_profiles,
				(SELECT COUNT(*)::int FROM nba.team) AS team_profiles
		) health`,
		"nfl_health_page": `SELECT json_build_object(
			'page', 'health',
			'sport', 'nfl',
			'scope', json_build_object('league_id', $1::int),
			'status', CASE WHEN health.player_profiles + health.team_profiles > 0 THEN 'healthy' ELSE 'degraded' END,
			'counts', json_build_object(
				'player_profiles', health.player_profiles,
				'team_profiles', health.team_profiles
			),
			'freshness', json_build_object(
				'player_stats_updated_at', NULL,
				'team_stats_updated_at', NULL,
				'latest_updated_at', NULL
			),
			'league_context', NULL
		)
		FROM (
			SELECT
				(SELECT COUNT(*)::int FROM nfl.player) AS player_profiles,
				(SELECT COUNT(*)::int FROM nfl.team) AS team_profiles
		) health`,
		"football_health_page": `SELECT json_build_object(
			'page', 'health',
			'sport', 'football',
			'scope', json_build_object('league_id', $1::int),
			'status', CASE WHEN health.player_profiles + health.team_profiles > 0 THEN 'healthy' ELSE 'degraded' END,
			'counts', json_build_object(
				'player_profiles', health.player_profiles,
				'team_profiles', health.team_profiles
			),
			'freshness', json_build_object(
				'player_stats_updated_at', NULL,
				'team_stats_updated_at', NULL,
				'latest_updated_at', NULL
			),
			'league_context', CASE
				WHEN $1::int IS NOT NULL THEN (
					SELECT row_to_json(lc)
					FROM (
						SELECT id, name, country, logo_url, is_benchmark, is_active
						FROM football.leagues
						WHERE id = $1::int
					) lc
				)
				ELSE NULL
			END
		)
		FROM (
			SELECT
				(SELECT COUNT(*)::int FROM football.player) AS player_profiles,
				(SELECT COUNT(*)::int FROM football.team) AS team_profiles
		) health`,

		// Entity name lookup (news handlers + notifications)
		"team_name_lookup":   "SELECT name FROM teams WHERE id = $1 AND sport = $2",
		"team_news_lookup":   "SELECT name, search_aliases FROM teams WHERE id = $1 AND sport = $2",
		"player_news_lookup": "SELECT name, first_name, last_name, team_id, search_aliases FROM players WHERE id = $1 AND sport = $2",

		// Twitter lazy cache (see sql/migrations/002_add_twitter_cache.sql)
		"twitter_list_get": `SELECT list_id, ttl_seconds, since_id, last_fetched_at
			FROM twitter_lists WHERE sport = $1`,
		"twitter_list_upsert": `INSERT INTO twitter_lists (sport, list_id, ttl_seconds, updated_at)
			VALUES ($1, $2, $3, now())
			ON CONFLICT (sport) DO UPDATE SET
				list_id     = EXCLUDED.list_id,
				ttl_seconds = EXCLUDED.ttl_seconds,
				updated_at  = now()`,
		"twitter_list_mark_fetched": `UPDATE twitter_lists
			SET since_id = COALESCE($2, since_id),
			    last_fetched_at = now(),
			    last_error = NULL,
			    last_error_at = NULL,
			    updated_at = now()
			WHERE sport = $1`,
		"twitter_list_mark_error": `UPDATE twitter_lists
			SET last_error = $2, last_error_at = now(), updated_at = now()
			WHERE sport = $1`,
		"twitter_list_status_all": `SELECT sport, list_id, ttl_seconds, since_id, last_fetched_at,
			last_error, last_error_at FROM twitter_lists ORDER BY sport`,
		"twitter_tweet_upsert": `INSERT INTO tweets
			(id, sport, author_id, author_username, author_name, author_verified,
			 author_profile_image_url, text, posted_at, likes, retweets, replies, fetched_at)
			VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12, now())
			ON CONFLICT (id) DO UPDATE SET
				likes = EXCLUDED.likes,
				retweets = EXCLUDED.retweets,
				replies = EXCLUDED.replies,
				fetched_at = now()`,
		"twitter_feed_by_sport": `SELECT json_build_object(
			'sport', $1::text,
			'tweets', COALESCE((
				SELECT json_agg(row_to_json(t))
				FROM (
					SELECT id, author_username AS username, author_name AS name,
						author_verified AS verified,
						author_profile_image_url AS profile_image_url,
						text, posted_at AS created_at, likes, retweets, replies,
						'https://twitter.com/' || author_username || '/status/' || id AS url
					FROM tweets
					WHERE sport = $1
					ORDER BY posted_at DESC
					LIMIT $2
				) t
			), '[]'::json),
			'meta', json_build_object(
				'feed_size', (SELECT COUNT(*)::int FROM tweets WHERE sport = $1),
				'last_fetched_at', (SELECT last_fetched_at FROM twitter_lists WHERE sport = $1),
				'ttl_seconds', (SELECT ttl_seconds FROM twitter_lists WHERE sport = $1)
			)
		)`,
		"twitter_feed_by_entity": `SELECT json_build_object(
			'sport', $1::text,
			'entity_type', $2::text,
			'entity_id', $3::int,
			'tweets', COALESCE((
				SELECT json_agg(row_to_json(t))
				FROM (
					SELECT tw.id, tw.author_username AS username, tw.author_name AS name,
						tw.author_verified AS verified,
						tw.author_profile_image_url AS profile_image_url,
						tw.text, tw.posted_at AS created_at,
						tw.likes, tw.retweets, tw.replies,
						'https://twitter.com/' || tw.author_username || '/status/' || tw.id AS url
					FROM tweets tw
					JOIN tweet_entities te ON te.tweet_id = tw.id
					WHERE te.sport = $1 AND te.entity_type = $2 AND te.entity_id = $3
					ORDER BY tw.posted_at DESC
					LIMIT $4
				) t
			), '[]'::json)
		)`,
		"twitter_entity_link": `INSERT INTO tweet_entities (tweet_id, sport, entity_type, entity_id)
			VALUES ($1, $2, $3, $4)
			ON CONFLICT DO NOTHING`,
		"twitter_entities_for_sport": `SELECT
				'player'::text AS entity_type, id, name,
				COALESCE(first_name, '') AS first_name,
				COALESCE(last_name, '')  AS last_name,
				search_aliases
			FROM players WHERE sport = $1
			UNION ALL
			SELECT 'team'::text, id, name, '' AS first_name, '' AS last_name, search_aliases
			FROM teams WHERE sport = $1`,
		"twitter_tweets_purge": `DELETE FROM tweets
			WHERE sport = $1 AND fetched_at < now() - make_interval(secs => $2::int)`,

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
