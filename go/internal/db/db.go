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
		),
		entity_season_score AS (
			SELECT season_composite_score, season_composite_rank, season_composite_rank_alltime, season_composite_rank_absolute, season_composite_rank_alltime_absolute FROM (
				SELECT ps.season_composite_score, ps.season_composite_rank, ps.season_composite_rank_alltime, ps.season_composite_rank_absolute, ps.season_composite_rank_alltime_absolute
				FROM public.player_stats ps, req, selected_entity se
				WHERE req.entity_type = 'player' AND ps.sport = 'NBA'
				  AND ps.player_id = req.entity_id AND ps.season = se.season
				  AND COALESCE(ps.league_id, 0) = se.league_id
				UNION ALL
				SELECT ts.season_composite_score, ts.season_composite_rank, ts.season_composite_rank_alltime, NULL::numeric, NULL::numeric
				FROM public.team_stats ts, req, selected_entity se
				WHERE req.entity_type = 'team' AND ts.sport = 'NBA'
				  AND ts.team_id = req.entity_id AND ts.season = se.season
				  AND COALESCE(ts.league_id, 0) = se.league_id
			) sub
			LIMIT 1
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
				'available_seasons', (SELECT seasons FROM entity_seasons),
				'season_composite_score', (SELECT season_composite_score FROM entity_season_score),
				'season_composite_rank', (SELECT season_composite_rank FROM entity_season_score),
				'season_composite_rank_alltime', (SELECT season_composite_rank_alltime FROM entity_season_score),
				'season_composite_rank_absolute', (SELECT season_composite_rank_absolute FROM entity_season_score),
				'season_composite_rank_alltime_absolute', (SELECT season_composite_rank_alltime_absolute FROM entity_season_score)
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
		),
		entity_season_score AS (
			SELECT season_composite_score, season_composite_rank, season_composite_rank_alltime, season_composite_rank_absolute, season_composite_rank_alltime_absolute FROM (
				SELECT ps.season_composite_score, ps.season_composite_rank, ps.season_composite_rank_alltime, ps.season_composite_rank_absolute, ps.season_composite_rank_alltime_absolute
				FROM public.player_stats ps, req, selected_entity se
				WHERE req.entity_type = 'player' AND ps.sport = 'NFL'
				  AND ps.player_id = req.entity_id AND ps.season = se.season
				  AND COALESCE(ps.league_id, 0) = se.league_id
				UNION ALL
				SELECT ts.season_composite_score, ts.season_composite_rank, ts.season_composite_rank_alltime, NULL::numeric, NULL::numeric
				FROM public.team_stats ts, req, selected_entity se
				WHERE req.entity_type = 'team' AND ts.sport = 'NFL'
				  AND ts.team_id = req.entity_id AND ts.season = se.season
				  AND COALESCE(ts.league_id, 0) = se.league_id
			) sub
			LIMIT 1
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
				'available_seasons', (SELECT seasons FROM entity_seasons),
				'season_composite_score', (SELECT season_composite_score FROM entity_season_score),
				'season_composite_rank', (SELECT season_composite_rank FROM entity_season_score),
				'season_composite_rank_alltime', (SELECT season_composite_rank_alltime FROM entity_season_score),
				'season_composite_rank_absolute', (SELECT season_composite_rank_absolute FROM entity_season_score),
				'season_composite_rank_alltime_absolute', (SELECT season_composite_rank_alltime_absolute FROM entity_season_score)
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
		),
		entity_season_score AS (
			SELECT season_composite_score, season_composite_rank, season_composite_rank_alltime, season_composite_rank_absolute, season_composite_rank_alltime_absolute FROM (
				SELECT ps.season_composite_score, ps.season_composite_rank, ps.season_composite_rank_alltime, ps.season_composite_rank_absolute, ps.season_composite_rank_alltime_absolute
				FROM public.player_stats ps, req, selected_entity se
				WHERE req.entity_type = 'player' AND ps.sport = 'FOOTBALL'
				  AND ps.player_id = req.entity_id AND ps.season = se.season
				  AND COALESCE(ps.league_id, 0) = se.league_id
				UNION ALL
				SELECT ts.season_composite_score, ts.season_composite_rank, ts.season_composite_rank_alltime, NULL::numeric, NULL::numeric
				FROM public.team_stats ts, req, selected_entity se
				WHERE req.entity_type = 'team' AND ts.sport = 'FOOTBALL'
				  AND ts.team_id = req.entity_id AND ts.season = se.season
				  AND COALESCE(ts.league_id, 0) = se.league_id
			) sub
			LIMIT 1
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
				'available_seasons', (SELECT seasons FROM entity_seasons),
				'season_composite_score', (SELECT season_composite_score FROM entity_season_score),
				'season_composite_rank', (SELECT season_composite_rank FROM entity_season_score),
				'season_composite_rank_alltime', (SELECT season_composite_rank_alltime FROM entity_season_score),
				'season_composite_rank_absolute', (SELECT season_composite_rank_absolute FROM entity_season_score),
				'season_composite_rank_alltime_absolute', (SELECT season_composite_rank_alltime_absolute FROM entity_season_score)
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
				-- Team-level composite score for this fixture (migration 017).
				-- NULL when the team has no event_team_stats row for the fixture
				-- (e.g. status='scheduled' on a row that slipped past the
				-- status filter, or a finalize_fixture that hasn't run yet).
				'composite_score', ets.composite_score,
				'opponent', json_build_object(
					'id',         t.id,
					'name',       t.name,
					'short_code', t.short_code,
					'logo_url',   t.logo_url
				)
			) ORDER BY tf.start_time DESC)
			FROM team_fixtures tf
			LEFT JOIN teams t ON t.id = tf.opponent_id AND t.sport = '` + sportID + `'
			LEFT JOIN event_team_stats ets
			  ON ets.fixture_id = tf.id
			 AND ets.team_id = (SELECT team_id FROM req)
			 AND ets.sport = '` + sportID + `'
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
//
// Unit handling (migration 016): both sides JOIN stat_definitions and filter
// to keys where comparable=true, so the frontend never has to reconcile
// mismatched units. Peer side additionally normalizes cumulative_total keys
// (e.g. football team `tackles: 350` over a 23-match season) by dividing by
// the sport's per-row games-played key — matches_played for football,
// games_played for NBA/NFL. The entity-recent side is a simple AVG because
// every input row is a single fixture (already per-game).
//
// Known limitation — PLAYER trends on NFL & football: the seeder stores raw
// per-event counts in event_box_scores (e.g. `passing_yards`, `tackles`)
// but derived per-game/per-90 averages in player_stats (e.g.
// `passing_yards_per_game`, `tackles_per_90`). The key names differ, so the
// intersection used by the frontend's trends card is small or empty for
// these sports. Fixing it requires either unifying the seeder schema or
// having this CTE emit per-90/per-game keys synthesized from the raw event
// counts. Tracked separately. TEAM trends (the original Spurs bug) are
// fully comparable.
func trendsStatement(sportTag, sportID string, leagueScoped bool) string {
	// Sport-specific divisor for converting season cumulatives to per-game.
	// Coverage was verified 100% for the tables we actually normalize against
	// (NFL team_stats, football team_stats); player_stats coverage is uneven
	// so player cumulatives are flagged non-comparable in stat_definitions
	// and never reach the divisor branch.
	divisorKey := "games_played"
	if sportID == "FOOTBALL" {
		divisorKey = "matches_played"
	}

	// Entity-side rate_pct filter.
	//
	// Football: SportMonks per-fixture stats include real percentages
	// (pass_accuracy, possession_pct) in the same 0..100 scale as the
	// season-rolled team_stats version, so we keep them with a [0,100]
	// sanity guard that drops the handful of broken keys SportMonks emits
	// as non-normalized aggregates (tackles_won_percentage ≈ 700/fixture).
	//
	// NBA / NFL: the BDL seeder accumulates team event rows by SUMMING the
	// underlying player rows, which turns rate_pct keys into nonsense
	// (e.g. team fg_pct ≈ sum of player fg_pcts ≈ 4.0). Player events
	// have their own mismatch — fg_pct stored as a 0..1 fraction in
	// event_box_scores vs 0..100 in player_stats. Until the seeder writes
	// matching units, drop rate_pct from the entity-recent side for these
	// sports. The peer-season side still surfaces the same keys; the
	// frontend takes the intersection so they fall out of the trends card
	// rather than render as wrong-looking numbers.
	recentRatePctGuard := "AND sd.unit <> 'rate_pct'"
	if sportID == "FOOTBALL" {
		recentRatePctGuard = "AND (sd.unit <> 'rate_pct' OR (kv.value)::numeric BETWEEN 0 AND 100)"
	}

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
		SELECT e.fixture_id, e.stats, f.start_time, f.season AS event_season,
		       e.composite_score, e.minutes_played
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
		SELECT e.fixture_id, e.stats, f.start_time, f.season AS event_season,
		       e.composite_score, NULL::numeric AS minutes_played
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
		SELECT fixture_id, stats, start_time, event_season,
		       composite_score, minutes_played FROM player_events
		UNION ALL
		SELECT fixture_id, stats, start_time, event_season,
		       composite_score, minutes_played FROM team_events
	),
	-- Full-season events (no LIMIT, no prior-season bridge). Drives
	-- entity_event_scores so the frontend sparkline can render a real
	-- season trajectory across every played event rather than just the
	-- last 3. We keep the limit-3 entity_events above intact because
	-- entity_recent_avgs and the window metadata legitimately need
	-- "last 3, bridging prior season if current is sparse" semantics.
	player_season_events AS (
		SELECT e.fixture_id, e.composite_score, e.minutes_played, f.start_time
		FROM event_box_scores e
		JOIN fixtures f ON f.id = e.fixture_id
		CROSS JOIN req
		CROSS JOIN resolved_season rs
		CROSS JOIN effective_league el
		WHERE req.entity_type = 'player'
		  AND e.player_id = req.entity_id
		  AND e.sport = '` + sportID + `'
		  AND f.season = rs.season
		  AND (el.league_id IS NULL OR f.league_id = el.league_id)
	),
	team_season_events AS (
		SELECT e.fixture_id, e.composite_score, NULL::numeric AS minutes_played, f.start_time
		FROM event_team_stats e
		JOIN fixtures f ON f.id = e.fixture_id
		CROSS JOIN req
		CROSS JOIN resolved_season rs
		CROSS JOIN effective_league el
		WHERE req.entity_type = 'team'
		  AND e.team_id = req.entity_id
		  AND e.sport = '` + sportID + `'
		  AND f.season = rs.season
		  AND (el.league_id IS NULL OR f.league_id = el.league_id)
	),
	entity_season_events AS (
		SELECT fixture_id, composite_score, minutes_played, start_time FROM player_season_events
		UNION ALL
		SELECT fixture_id, composite_score, minutes_played, start_time FROM team_season_events
	),
	entity_recent_avgs AS (
		-- Average the entity's last-3 single-fixture values per stat key,
		-- filtered to stat_definitions.comparable so we never emit keys whose
		-- unit doesn't survive an apples-to-apples compare against the peer
		-- side. No divisor here: every event row is already one fixture.
		-- The recentRatePctGuard (see helper preamble) drops rate_pct keys
		-- whose event-row values are written in a different unit than the
		-- season-rolled version by the seeder.
		SELECT COALESCE(jsonb_object_agg(key, avg_val), '{}'::jsonb) AS avgs
		FROM (
			SELECT kv.key, AVG((kv.value)::numeric) AS avg_val
			FROM entity_events e
			CROSS JOIN req
			CROSS JOIN LATERAL jsonb_each(e.stats) kv
			JOIN stat_definitions sd
			  ON sd.sport = '` + sportID + `'
			 AND sd.entity_type = req.entity_type
			 AND sd.key_name = kv.key
			 AND sd.comparable = true
			WHERE jsonb_typeof(kv.value) = 'number'
			  ` + recentRatePctGuard + `
			GROUP BY kv.key
		) s
	),
	entity_self_row AS (
		-- The entity's own season-rolled stats row (player_stats or team_stats),
		-- scoped to the resolved season + effective league. Emits 0 or 1 row.
		-- Drives entity_season_aggregate below, which mirrors peer_aggregate so
		-- the frontend can render a self-delta alongside the peer-delta — useful
		-- specifically for dominant outliers where every peer comparison reads
		-- as a huge positive and the user can't tell which way the entity is
		-- actually trending relative to its own baseline.
		--
		-- season_composite_score comes from migration 017; surfaces directly in
		-- the trends payload as entity_season_score_avg.
		(SELECT ps.stats, ps.season_composite_score, ps.season_composite_rank, ps.season_composite_rank_alltime,
		        ps.season_composite_rank_absolute, ps.season_composite_rank_alltime_absolute
		 FROM player_stats ps, req, resolved_season rs, effective_league el
		 WHERE req.entity_type = 'player'
		   AND ps.sport = '` + sportID + `'
		   AND ps.season = rs.season
		   AND ps.player_id = req.entity_id
		   AND (el.league_id IS NULL OR ps.league_id = el.league_id)
		 ORDER BY ps.updated_at DESC
		 LIMIT 1)
		UNION ALL
		(SELECT ts.stats, ts.season_composite_score, ts.season_composite_rank, ts.season_composite_rank_alltime,
		        NULL::numeric, NULL::numeric
		 FROM team_stats ts, req, resolved_season rs, effective_league el
		 WHERE req.entity_type = 'team'
		   AND ts.sport = '` + sportID + `'
		   AND ts.season = rs.season
		   AND ts.team_id = req.entity_id
		   AND (el.league_id IS NULL OR ts.league_id = el.league_id)
		 ORDER BY ts.updated_at DESC
		 LIMIT 1)
	),
	entity_season_aggregate AS (
		-- Same comparability filter + cumulative-total normalization as
		-- peer_aggregate. Operates on a single row (the entity's own season
		-- stats), so the divisor handling reduces to a per-row division rather
		-- than divide-then-average. Player entities end up with mostly empty
		-- output (cumulative_total is non-comparable for players); that's fine
		-- and the frontend treats {} as "no self-delta to render."
		SELECT COALESCE(jsonb_object_agg(key, val), '{}'::jsonb) AS avgs
		FROM (
			SELECT kv.key,
			       CASE
			           WHEN sd.unit = 'cumulative_total' THEN
			               (kv.value)::numeric
			               / NULLIF((er.stats->>'` + divisorKey + `')::numeric, 0)
			           ELSE
			               (kv.value)::numeric
			       END AS val
			FROM entity_self_row er
			CROSS JOIN req
			CROSS JOIN LATERAL jsonb_each(er.stats) kv
			JOIN stat_definitions sd
			  ON sd.sport = '` + sportID + `'
			 AND sd.entity_type = req.entity_type
			 AND sd.key_name = kv.key
			 AND sd.comparable = true
			WHERE jsonb_typeof(kv.value) = 'number'
		) s
	),
	player_peer_cohort AS (
		SELECT ps.stats, ps.season_composite_score
		FROM player_stats ps, req, resolved_season rs, effective_league el, player_position pp
		WHERE req.entity_type = 'player'
		  AND ps.sport = '` + sportID + `'
		  AND ps.season = rs.season
		  AND ps.position = pp.position
		  AND ps.player_id <> req.entity_id
		  AND (el.league_id IS NULL OR ps.league_id = el.league_id)
	),
	team_peer_cohort AS (
		SELECT ts.stats, ts.season_composite_score
		FROM team_stats ts, req, resolved_season rs, effective_league el
		WHERE req.entity_type = 'team'
		  AND ts.sport = '` + sportID + `'
		  AND ts.season = rs.season
		  AND ts.team_id <> req.entity_id
		  AND (el.league_id IS NULL OR ts.league_id = el.league_id)
	),
	peer_cohort AS (
		SELECT stats, season_composite_score FROM player_peer_cohort
		UNION ALL
		SELECT stats, season_composite_score FROM team_peer_cohort
	),
	peer_aggregate AS (
		-- Average peer-cohort season values per stat key, filtered to
		-- comparable keys. Cumulative totals are normalized to per-game by
		-- dividing each peer's value by their own games-played key — that
		-- per-team division has to happen INSIDE the AVG so we don't bias
		-- toward teams that played more games.
		SELECT
			COALESCE((SELECT jsonb_object_agg(key, avg_val) FROM (
				SELECT kv.key, AVG(
					CASE
						WHEN sd.unit = 'cumulative_total' THEN
							(kv.value)::numeric
							/ NULLIF((pc.stats->>'` + divisorKey + `')::numeric, 0)
						ELSE
							(kv.value)::numeric
					END
				) AS avg_val
				FROM peer_cohort pc
				CROSS JOIN req
				CROSS JOIN LATERAL jsonb_each(pc.stats) kv
				JOIN stat_definitions sd
				  ON sd.sport = '` + sportID + `'
				 AND sd.entity_type = req.entity_type
				 AND sd.key_name = kv.key
				 AND sd.comparable = true
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
	),
	-- Season vibe series: daily-bucketed sentiment averages from the start
	-- of the current sport+league season through NOW(). Drives the frontend's
	-- season-length vibe sparkline. Sibling to vibe_window (recent 7-day raw
	-- snapshot list); the two answer different questions and both stay.
	vibe_season_anchor AS (
		-- Anchor = first kickoff of the most-recently-started season in this
		-- sport+league scope. Per-sport (not per-entity) so two entities in
		-- the same scope share a date axis — frontend can compare them on
		-- aligned sparklines. During the offseason the anchor stays pinned
		-- to the most recent started season's start, so vibes carry through
		-- gap periods (trade rumors, draft news, off-day sentiment); once
		-- the next season's first fixture kicks off, the anchor moves
		-- forward.
		SELECT MIN(f.start_time) AS season_start
		FROM fixtures f, effective_league el
		WHERE f.sport = '` + sportID + `'
		  AND f.season = (
		      SELECT MAX(season) FROM fixtures
		      WHERE sport = '` + sportID + `'
		        AND start_time <= NOW()
		  )
		  AND (el.league_id IS NULL OR f.league_id = el.league_id)
		  AND f.start_time <= NOW()
	),
	vibe_season_series AS (
		-- One row per UTC day with >=1 non-null sentiment snapshot in [anchor, NOW].
		-- Days with zero snapshots are absent — the frontend renders them as
		-- honest gaps in the sparkline rather than zero-sentiment dots.
		SELECT
			DATE_TRUNC('day', vs.generated_at AT TIME ZONE 'UTC')::date AS day,
			ROUND(AVG(vs.sentiment)::numeric, 0)::int AS sentiment_avg,
			COUNT(*)::int AS snapshot_count
		FROM vibe_scores vs, req, vibe_season_anchor anchor
		WHERE vs.entity_type = req.entity_type
		  AND vs.entity_id = req.entity_id
		  AND vs.sport = '` + sportID + `'
		  AND vs.sentiment IS NOT NULL
		  AND anchor.season_start IS NOT NULL
		  AND vs.generated_at >= anchor.season_start
		  AND vs.generated_at <= NOW()
		GROUP BY 1
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
		'entity_season_avgs', (SELECT avgs FROM entity_season_aggregate),
		'peer_season_avgs',   (SELECT avgs FROM peer_aggregate),
		'peer_cohort_size',   (SELECT cohort_size FROM peer_aggregate),
		-- Composite-score block (migrations 017 + 018).
		-- entity_event_scores: per-event composite score across EVERY
		-- played event in the current season — drives the frontend's
		-- full-season sparkline. Each row carries fixture_id, the
		-- normalized composite_score (mean=50 per partition by
		-- construction), minutes_played for hover-tooltip context, and
		-- start_time for date labels and time-bucket grouping.
		-- entity_season_score_avg: the entity's own season composite.
		-- peer_season_score_avg: AVG of peer cohort's season composites
		-- (near 50 by construction; useful as an anchor for tier rendering).
		'entity_event_scores', COALESCE((
			SELECT json_agg(json_build_object(
				'fixture_id',      fixture_id,
				'composite_score', composite_score,
				'minutes_played',  minutes_played,
				'start_time',      start_time
			) ORDER BY start_time DESC)
			FROM entity_season_events
		), '[]'::json),
		'entity_season_score_avg',            (SELECT season_composite_score FROM entity_self_row),
		'entity_season_score_rank',           (SELECT season_composite_rank FROM entity_self_row),
		'entity_alltime_score_rank',          (SELECT season_composite_rank_alltime FROM entity_self_row),
		'entity_season_score_rank_absolute',  (SELECT season_composite_rank_absolute FROM entity_self_row),
		'entity_alltime_score_rank_absolute', (SELECT season_composite_rank_alltime_absolute FROM entity_self_row),
		'peer_season_score_avg',              (SELECT ROUND(AVG(season_composite_score), 1) FROM peer_cohort),
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
		-- Season-length daily vibe series (sibling to vibes.snapshots).
		-- Same date range as entity_event_scores: anchored at the entity's
		-- oldest scored event in the season, ends at NOW(). One JSON row
		-- per UTC day with >=1 non-null sentiment snapshot; days with zero
		-- snapshots are omitted so the frontend renders honest gaps rather
		-- than zero-sentiment dots. [] when the entity has no scored events
		-- in scope.
		'entity_season_vibe_series', COALESCE((
			SELECT json_agg(json_build_object(
				'date',           day,
				'sentiment_avg',  sentiment_avg,
				'snapshot_count', snapshot_count
			) ORDER BY day ASC)
			FROM vibe_season_series
		), '[]'::json),
		'meta', json_build_object(
			'season',    (SELECT season FROM resolved_season),
			'league_id', NULLIF((SELECT league_id FROM effective_league), 0),
			'position',  (SELECT position FROM player_position)
		)
	)
	FROM req`
}
