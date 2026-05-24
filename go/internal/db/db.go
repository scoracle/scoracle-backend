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
		),
		entity_seasons AS (
			SELECT COALESCE(array_agg(season ORDER BY season DESC), ARRAY[]::int[]) AS seasons
			FROM (
				SELECT DISTINCT ps.season
				FROM public.player_stats ps, req
				WHERE req.entity_type = 'player'
				  AND ps.sport = 'NBA'
				  AND ps.player_id = req.entity_id
				  AND (req.league_id IS NULL OR COALESCE(ps.league_id, 0) = req.league_id)
				UNION
				SELECT DISTINCT ts.season
				FROM public.team_stats ts, req
				WHERE req.entity_type = 'team'
				  AND ts.sport = 'NBA'
				  AND ts.team_id = req.entity_id
				  AND (req.league_id IS NULL OR COALESCE(ts.league_id, 0) = req.league_id)
			) s
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
				'league_id', NULLIF(se.league_id, 0),
				'available_seasons', (SELECT seasons FROM entity_seasons)
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
		),
		entity_seasons AS (
			SELECT COALESCE(array_agg(season ORDER BY season DESC), ARRAY[]::int[]) AS seasons
			FROM (
				SELECT DISTINCT ps.season
				FROM public.player_stats ps, req
				WHERE req.entity_type = 'player'
				  AND ps.sport = 'NFL'
				  AND ps.player_id = req.entity_id
				  AND (req.league_id IS NULL OR COALESCE(ps.league_id, 0) = req.league_id)
				UNION
				SELECT DISTINCT ts.season
				FROM public.team_stats ts, req
				WHERE req.entity_type = 'team'
				  AND ts.sport = 'NFL'
				  AND ts.team_id = req.entity_id
				  AND (req.league_id IS NULL OR COALESCE(ts.league_id, 0) = req.league_id)
			) s
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
				'league_id', NULLIF(se.league_id, 0),
				'available_seasons', (SELECT seasons FROM entity_seasons)
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
		),
		entity_seasons AS (
			SELECT COALESCE(array_agg(season ORDER BY season DESC), ARRAY[]::int[]) AS seasons
			FROM (
				SELECT DISTINCT ps.season
				FROM public.player_stats ps, req
				WHERE req.entity_type = 'player'
				  AND ps.sport = 'FOOTBALL'
				  AND ps.player_id = req.entity_id
				  AND (req.league_id IS NULL OR COALESCE(ps.league_id, 0) = req.league_id)
				UNION
				SELECT DISTINCT ts.season
				FROM public.team_stats ts, req
				WHERE req.entity_type = 'team'
				  AND ts.sport = 'FOOTBALL'
				  AND ts.team_id = req.entity_id
				  AND (req.league_id IS NULL OR COALESCE(ts.league_id, 0) = req.league_id)
			) s
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
				'league_id', NULLIF(se.league_id, 0),
				'available_seasons', (SELECT seasons FROM entity_seasons)
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
		// Trends — last-3 entity events vs peer-cohort season averages.
		// Pure read-only: aggregates event_box_scores / event_team_stats over a
		// rolling 3-fixture window, and averages peer player_stats / team_stats for
		// the same season. Structured so the CTE chain can be lifted into a SQL
		// function for PostgREST (data.scoracle) without reshaping.
		"nba_trends_page":      trendsStatement("nba", "NBA", false),
		"nfl_trends_page":      trendsStatement("nfl", "NFL", false),
		"football_trends_page": trendsStatement("football", "FOOTBALL", true),

		// Team season results — list of final scorelines from fixtures for one
		// team in a season, with opponent identity and home/away framing. Final
		// scores only (status IN ('completed','seeded')); upcoming fixtures live
		// elsewhere if/when that endpoint exists.
		"nba_team_results":      teamResultsStatement("nba", "NBA", false),
		"nfl_team_results":      teamResultsStatement("nfl", "NFL", false),
		"football_team_results": teamResultsStatement("football", "FOOTBALL", true),

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
			    counters_date = CASE
			        WHEN counters_date = (now() AT TIME ZONE 'UTC')::date THEN counters_date
			        ELSE (now() AT TIME ZONE 'UTC')::date
			    END,
			    calls_today = CASE
			        WHEN counters_date = (now() AT TIME ZONE 'UTC')::date THEN calls_today + 1
			        ELSE 1
			    END,
			    tweets_today = CASE
			        WHEN counters_date = (now() AT TIME ZONE 'UTC')::date THEN tweets_today + $3::int
			        ELSE $3::int
			    END,
			    updated_at = now()
			WHERE sport = $1`,
		"twitter_list_mark_error": `UPDATE twitter_lists
			SET last_error = $2, last_error_at = now(),
			    counters_date = CASE
			        WHEN counters_date = (now() AT TIME ZONE 'UTC')::date THEN counters_date
			        ELSE (now() AT TIME ZONE 'UTC')::date
			    END,
			    calls_today = CASE
			        WHEN counters_date = (now() AT TIME ZONE 'UTC')::date THEN calls_today + 1
			        ELSE 1
			    END,
			    updated_at = now()
			WHERE sport = $1`,
		"twitter_list_status_all": `SELECT sport, list_id, ttl_seconds, since_id, last_fetched_at,
			last_error, last_error_at, counters_date, calls_today, tweets_today
			FROM twitter_lists ORDER BY sport`,
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
				SELECT json_agg(json_build_object(
					'id', id,
					'text', text,
					'author', author_name,
					'author_username', author_username,
					'created_at', posted_at,
					'verified', author_verified,
					'profile_image_url', author_profile_image_url,
					'metrics', json_build_object(
						'like_count', likes,
						'retweet_count', retweets,
						'reply_count', replies
					),
					'url', 'https://twitter.com/' || author_username || '/status/' || id
				) ORDER BY posted_at DESC)
				FROM (
					SELECT id, author_username, author_name, author_verified,
						author_profile_image_url, text, posted_at, likes, retweets, replies
					FROM tweets
					WHERE sport = $1
					  AND posted_at > NOW() - INTERVAL '48 hours'
					ORDER BY posted_at DESC
					LIMIT $2
				) t
			), '[]'::json),
			'meta', json_build_object(
				'feed_size', (SELECT COUNT(*)::int FROM tweets WHERE sport = $1 AND posted_at > NOW() - INTERVAL '48 hours'),
				'last_fetched_at', (SELECT last_fetched_at FROM twitter_lists WHERE sport = $1),
				'ttl_seconds', (SELECT ttl_seconds FROM twitter_lists WHERE sport = $1)
			)
		)`,
		"twitter_feed_by_entity": `SELECT json_build_object(
			'sport', $1::text,
			'entity_type', $2::text,
			'entity_id', $3::int,
			'tweets', COALESCE((
				SELECT json_agg(json_build_object(
					'id', id,
					'text', text,
					'author', author_name,
					'author_username', author_username,
					'created_at', posted_at,
					'verified', author_verified,
					'profile_image_url', author_profile_image_url,
					'metrics', json_build_object(
						'like_count', likes,
						'retweet_count', retweets,
						'reply_count', replies
					),
					'url', 'https://twitter.com/' || author_username || '/status/' || id
				) ORDER BY posted_at DESC)
				FROM (
					SELECT tw.id, tw.author_username, tw.author_name, tw.author_verified,
						tw.author_profile_image_url, tw.text, tw.posted_at,
						tw.likes, tw.retweets, tw.replies
					FROM tweets tw
					JOIN tweet_entities te ON te.tweet_id = tw.id
					WHERE te.sport = $1 AND te.entity_type = $2 AND te.entity_id = $3
					  AND tw.posted_at > NOW() - INTERVAL '48 hours'
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

// teamResultsStatement builds the per-sport team season results prepared
// statement. It returns the team's finalized scorelines (status IN
// ('completed','seeded')) for one season, framed from the team's perspective:
// each row carries opponent identity, home/away, the team's own score, the
// opponent's score, and a W/L/D result derived from the two scores.
//
// Args: $1::int team_id, $2::int season (nullable → sports.current_season),
// $3::int league_id (nullable; football falls back to the team's natural
// league via team_stats so we don't return rows from other leagues' cups).
//
// W/L/D is a one-line comparison of two columns the row already carries — not
// a percentile or derived stat — so it lives here rather than forcing every
// consumer to recompute it. Results are ordered newest first.
func teamResultsStatement(sportTag, sportID string, leagueScoped bool) string {
	effectiveLeague := "req.league_id"
	if leagueScoped {
		effectiveLeague = `COALESCE(
			req.league_id,
			(SELECT ts.league_id FROM team_stats ts
			 WHERE ts.team_id = req.team_id
			   AND ts.sport = '` + sportID + `'
			   AND ts.season = (SELECT season FROM resolved_season)
			 ORDER BY ts.updated_at DESC LIMIT 1)
		)`
	}

	return `WITH req AS (
		SELECT $1::int AS team_id, $2::int AS season, $3::int AS league_id
	),
	resolved_season AS (
		SELECT COALESCE(
			(SELECT season FROM req),
			(SELECT current_season FROM public.sports WHERE id = '` + sportID + `')
		) AS season
	),
	effective_league AS (
		SELECT ` + effectiveLeague + ` AS league_id FROM req
	),
	team_fixtures AS (
		SELECT f.id, f.start_time, f.status, f.round,
		       f.home_team_id, f.away_team_id, f.home_score, f.away_score,
		       (f.home_team_id = req.team_id) AS is_home,
		       CASE WHEN f.home_team_id = req.team_id
		            THEN f.away_team_id ELSE f.home_team_id END AS opponent_id,
		       CASE WHEN f.home_team_id = req.team_id
		            THEN f.home_score ELSE f.away_score END AS team_score,
		       CASE WHEN f.home_team_id = req.team_id
		            THEN f.away_score ELSE f.home_score END AS opponent_score
		FROM fixtures f, req, resolved_season rs, effective_league el
		WHERE f.sport = '` + sportID + `'
		  AND f.season = rs.season
		  AND f.status IN ('completed', 'seeded')
		  AND (f.home_team_id = req.team_id OR f.away_team_id = req.team_id)
		  AND (el.league_id IS NULL OR f.league_id = el.league_id)
	)
	SELECT json_build_object(
		'page', 'results',
		'sport', '` + sportTag + `',
		'team_id', (SELECT team_id FROM req),
		'results', COALESCE((
			SELECT json_agg(json_build_object(
				'fixture_id',     tf.id,
				'start_time',     tf.start_time,
				'status',         tf.status,
				'round',          tf.round,
				'home_away',      CASE WHEN tf.is_home THEN 'home' ELSE 'away' END,
				'team_score',     tf.team_score,
				'opponent_score', tf.opponent_score,
				'result',         CASE
				                    WHEN tf.team_score IS NULL OR tf.opponent_score IS NULL THEN NULL
				                    WHEN tf.team_score >  tf.opponent_score THEN 'W'
				                    WHEN tf.team_score <  tf.opponent_score THEN 'L'
				                    ELSE 'D'
				                  END,
				'opponent', json_build_object(
					'id',         t.id,
					'name',       t.name,
					'short_code', t.short_code,
					'logo_url',   t.logo_url
				)
			) ORDER BY tf.start_time DESC)
			FROM team_fixtures tf
			LEFT JOIN teams t ON t.id = tf.opponent_id AND t.sport = '` + sportID + `'
		), '[]'::json),
		'meta', json_build_object(
			'season',        (SELECT season FROM resolved_season),
			'league_id',     NULLIF((SELECT league_id FROM effective_league), 0),
			'games_played',  (SELECT COUNT(*) FROM team_fixtures)
		)
	)`
}

// trendsStatement builds the per-sport trends prepared statement. It returns
// raw last-3-event averages for the entity (player or team) alongside the
// matching peer-cohort season averages, plus window metadata. No derived
// signals — the frontend reads raw values and computes direction itself.
//
// sportTag is the lowercase URL sport ("nba", "nfl", "football"); sportID is
// the uppercase sports.id used in joins. leagueScoped=true (football) resolves
// the entity's natural league when no league_id is supplied, so the peer
// cohort stays inside one league instead of spanning all of them.
func trendsStatement(sportTag, sportID string, leagueScoped bool) string {
	// Football: when no league_id is supplied, fall back to the entity's own
	// league_id so the peer cohort isn't a meaningless multi-league mix.
	// NBA/NFL: league_id is effectively 0 across the dataset; no fallback.
	effectiveLeague := "req.league_id"
	if leagueScoped {
		effectiveLeague = `COALESCE(
			req.league_id,
			(SELECT ps.league_id FROM player_stats ps
			 WHERE req.entity_type = 'player'
			   AND ps.player_id = req.entity_id
			   AND ps.sport = '` + sportID + `'
			   AND ps.season = (SELECT season FROM resolved_season)
			 ORDER BY ps.updated_at DESC LIMIT 1),
			(SELECT ts.league_id FROM team_stats ts
			 WHERE req.entity_type = 'team'
			   AND ts.team_id = req.entity_id
			   AND ts.sport = '` + sportID + `'
			   AND ts.season = (SELECT season FROM resolved_season)
			 ORDER BY ts.updated_at DESC LIMIT 1)
		)`
	}

	return `WITH req AS (
		SELECT $1::text AS entity_type, $2::int AS entity_id,
		       $3::int AS season, $4::int AS league_id
	),
	resolved_season AS (
		SELECT COALESCE(
			(SELECT season FROM req),
			(SELECT current_season FROM public.sports WHERE id = '` + sportID + `')
		) AS season
	),
	effective_league AS (
		SELECT ` + effectiveLeague + ` AS league_id FROM req
	),
	player_position AS (
		SELECT ps.position
		FROM player_stats ps, req, resolved_season rs, effective_league el
		WHERE req.entity_type = 'player'
		  AND ps.player_id = req.entity_id
		  AND ps.sport = '` + sportID + `'
		  AND ps.season = rs.season
		  AND (el.league_id IS NULL OR ps.league_id = el.league_id)
		ORDER BY ps.updated_at DESC
		LIMIT 1
	),
	player_events AS (
		SELECT e.fixture_id, e.stats, f.start_time, f.season AS event_season
		FROM event_box_scores e
		JOIN fixtures f ON f.id = e.fixture_id
		CROSS JOIN req
		CROSS JOIN resolved_season rs
		CROSS JOIN effective_league el
		WHERE req.entity_type = 'player'
		  AND e.player_id = req.entity_id
		  AND e.sport = '` + sportID + `'
		  AND f.season IN (rs.season, rs.season - 1)
		  AND (el.league_id IS NULL OR f.league_id = el.league_id)
		ORDER BY f.start_time DESC
		LIMIT 3
	),
	team_events AS (
		SELECT e.fixture_id, e.stats, f.start_time, f.season AS event_season
		FROM event_team_stats e
		JOIN fixtures f ON f.id = e.fixture_id
		CROSS JOIN req
		CROSS JOIN resolved_season rs
		CROSS JOIN effective_league el
		WHERE req.entity_type = 'team'
		  AND e.team_id = req.entity_id
		  AND e.sport = '` + sportID + `'
		  AND f.season IN (rs.season, rs.season - 1)
		  AND (el.league_id IS NULL OR f.league_id = el.league_id)
		ORDER BY f.start_time DESC
		LIMIT 3
	),
	entity_events AS (
		SELECT fixture_id, stats, start_time, event_season FROM player_events
		UNION ALL
		SELECT fixture_id, stats, start_time, event_season FROM team_events
	),
	entity_recent_avgs AS (
		SELECT COALESCE(jsonb_object_agg(key, avg_val), '{}'::jsonb) AS avgs
		FROM (
			SELECT kv.key, AVG((kv.value)::numeric) AS avg_val
			FROM entity_events e, LATERAL jsonb_each(e.stats) kv
			WHERE jsonb_typeof(kv.value) = 'number'
			GROUP BY kv.key
		) s
	),
	player_peer_cohort AS (
		SELECT ps.stats
		FROM player_stats ps, req, resolved_season rs, effective_league el, player_position pp
		WHERE req.entity_type = 'player'
		  AND ps.sport = '` + sportID + `'
		  AND ps.season = rs.season
		  AND ps.position = pp.position
		  AND ps.player_id <> req.entity_id
		  AND (el.league_id IS NULL OR ps.league_id = el.league_id)
	),
	team_peer_cohort AS (
		SELECT ts.stats
		FROM team_stats ts, req, resolved_season rs, effective_league el
		WHERE req.entity_type = 'team'
		  AND ts.sport = '` + sportID + `'
		  AND ts.season = rs.season
		  AND ts.team_id <> req.entity_id
		  AND (el.league_id IS NULL OR ts.league_id = el.league_id)
	),
	peer_cohort AS (
		SELECT stats FROM player_peer_cohort
		UNION ALL
		SELECT stats FROM team_peer_cohort
	),
	peer_aggregate AS (
		SELECT
			COALESCE((SELECT jsonb_object_agg(key, avg_val) FROM (
				SELECT kv.key, AVG((kv.value)::numeric) AS avg_val
				FROM peer_cohort pc, LATERAL jsonb_each(pc.stats) kv
				WHERE jsonb_typeof(kv.value) = 'number'
				GROUP BY kv.key
			) t), '{}'::jsonb) AS avgs,
			(SELECT COUNT(*) FROM peer_cohort) AS cohort_size
	),
	vibe_window AS (
		-- Last 7 days of Gemma sentiment scores (1-100) for this entity.
		-- vibe_scores is append-only (BIGSERIAL PK + INSERT-only writes), so
		-- this is a faithful history snapshot. Legacy blurb-only rows have
		-- sentiment IS NULL — exclude them for consistency with the latest-vibe
		-- handler. Uppercase sport literal matches vibe_scores.sport.
		SELECT vs.sentiment, vs.generated_at, vs.trigger_type
		FROM vibe_scores vs, req
		WHERE vs.entity_type = req.entity_type
		  AND vs.entity_id = req.entity_id
		  AND vs.sport = '` + sportID + `'
		  AND vs.sentiment IS NOT NULL
		  AND vs.generated_at >= NOW() - INTERVAL '7 days'
	)
	SELECT json_build_object(
		'page', 'trends',
		'sport', '` + sportTag + `',
		'entity_type', req.entity_type,
		'entity_id', req.entity_id,
		'window', json_build_object(
			'games_used',         (SELECT COUNT(*) FROM entity_events),
			'fixture_ids',        COALESCE((SELECT json_agg(fixture_id ORDER BY start_time DESC) FROM entity_events), '[]'::json),
			'spans_prior_season', EXISTS (
				SELECT 1 FROM entity_events e, resolved_season rs WHERE e.event_season <> rs.season
			)
		),
		'entity_recent_avgs', (SELECT avgs FROM entity_recent_avgs),
		'peer_season_avgs',   (SELECT avgs FROM peer_aggregate),
		'peer_cohort_size',   (SELECT cohort_size FROM peer_aggregate),
		'vibes', json_build_object(
			'window_days', 7,
			'snapshots', COALESCE((
				SELECT json_agg(json_build_object(
					'sentiment',    sentiment,
					'generated_at', generated_at,
					'trigger_type', trigger_type
				) ORDER BY generated_at DESC)
				FROM vibe_window
			), '[]'::json)
		),
		'meta', json_build_object(
			'season',    (SELECT season FROM resolved_season),
			'league_id', NULLIF((SELECT league_id FROM effective_league), 0),
			'position',  (SELECT position FROM player_position)
		)
	)
	FROM req`
}
