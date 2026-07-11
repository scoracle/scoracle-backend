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

const universalEntitiesStatement = `WITH player_rows AS (
	SELECT
		p.id::text AS id,
		'player'::text AS type,
		lower(p.sport) AS sport,
		p.name,
		t.name AS team,
		NULLIF(COALESCE(cur.position, p.meta->>'position', p.meta->>'pos'), '') AS position,
		aliases.tokens AS aliases,
		search.tokens AS search_tokens
	FROM public.players p
	LEFT JOIN public.player_current_identity cur ON cur.player_id = p.id AND cur.sport = p.sport
	LEFT JOIN public.teams t ON t.id = cur.team_id AND t.sport = p.sport
	LEFT JOIN public.leagues l ON l.id = COALESCE(NULLIF(cur.league_id, 0), p.league_id) AND l.sport = p.sport
	LEFT JOIN LATERAL (
		SELECT COALESCE(array_agg(token ORDER BY token), ARRAY[]::text[]) AS tokens
		FROM (
			SELECT DISTINCT token
			FROM (
				SELECT NULLIF(lower(trim(v)), '') AS token
				FROM unnest(COALESCE(p.search_aliases, ARRAY[]::text[]) || ARRAY[p.first_name, p.last_name, p.name, replace(p.name, ' ', '')]) AS v
				UNION ALL
				SELECT NULLIF(unaccent(lower(trim(v))), '') AS token
				FROM unnest(COALESCE(p.search_aliases, ARRAY[]::text[]) || ARRAY[p.first_name, p.last_name, p.name, replace(p.name, ' ', '')]) AS v
			) normalized
			WHERE token IS NOT NULL
		) deduped
	) aliases ON true
	LEFT JOIN LATERAL (
		SELECT COALESCE(array_agg(token ORDER BY token), ARRAY[]::text[]) AS tokens
		FROM (
			SELECT DISTINCT token
			FROM (
				SELECT NULLIF(lower(trim(v)), '') AS token
				FROM unnest(aliases.tokens || ARRAY[t.short_code, t.name, replace(COALESCE(t.name, ''), ' ', ''), t.city, t.country, l.name]) AS v
				UNION ALL
				SELECT NULLIF(unaccent(lower(trim(v))), '') AS token
				FROM unnest(aliases.tokens || ARRAY[t.short_code, t.name, replace(COALESCE(t.name, ''), ' ', ''), t.city, t.country, l.name]) AS v
			) normalized
			WHERE token IS NOT NULL
		) deduped
	) search ON true
	WHERE p.sport IN ('NBA', 'NFL', 'FOOTBALL')
	  AND NULLIF(p.name, '') IS NOT NULL
),
team_rows AS (
	SELECT
		t.id::text AS id,
		'team'::text AS type,
		lower(t.sport) AS sport,
		t.name,
		NULL::text AS team,
		NULL::text AS position,
		aliases.tokens AS aliases,
		search.tokens AS search_tokens
	FROM public.teams t
	LEFT JOIN LATERAL (
		SELECT ts.league_id
		FROM public.team_stats ts
		WHERE ts.team_id = t.id AND ts.sport = t.sport
		ORDER BY ts.season DESC NULLS LAST, ts.updated_at DESC NULLS LAST
		LIMIT 1
	) cur ON true
	LEFT JOIN public.leagues l ON l.id = COALESCE(NULLIF(cur.league_id, 0), t.league_id) AND l.sport = t.sport
	LEFT JOIN LATERAL (
		SELECT COALESCE(array_agg(token ORDER BY token), ARRAY[]::text[]) AS tokens
		FROM (
			SELECT DISTINCT token
			FROM (
				SELECT NULLIF(lower(trim(v)), '') AS token
				FROM unnest(COALESCE(t.search_aliases, ARRAY[]::text[]) || ARRAY[t.name, replace(t.name, ' ', ''), t.short_code]) AS v
				UNION ALL
				SELECT NULLIF(unaccent(lower(trim(v))), '') AS token
				FROM unnest(COALESCE(t.search_aliases, ARRAY[]::text[]) || ARRAY[t.name, replace(t.name, ' ', ''), t.short_code]) AS v
			) normalized
			WHERE token IS NOT NULL
		) deduped
	) aliases ON true
	LEFT JOIN LATERAL (
		SELECT COALESCE(array_agg(token ORDER BY token), ARRAY[]::text[]) AS tokens
		FROM (
			SELECT DISTINCT token
			FROM (
				SELECT NULLIF(lower(trim(v)), '') AS token
				FROM unnest(aliases.tokens || ARRAY[t.city, t.country, t.conference, t.division, l.name]) AS v
				UNION ALL
				SELECT NULLIF(unaccent(lower(trim(v))), '') AS token
				FROM unnest(aliases.tokens || ARRAY[t.city, t.country, t.conference, t.division, l.name]) AS v
			) normalized
			WHERE token IS NOT NULL
		) deduped
	) search ON true
	WHERE t.sport IN ('NBA', 'NFL', 'FOOTBALL')
	  AND NULLIF(t.name, '') IS NOT NULL
),
entities AS (
	SELECT * FROM player_rows
	UNION ALL
	SELECT * FROM team_rows
),
entity_json AS (
	SELECT
		type,
		sport,
		name,
		jsonb_strip_nulls(jsonb_build_object(
			'id', id,
			'type', type,
			'sport', sport,
			'name', name,
			'team', team,
			'position', position,
			'aliases', to_jsonb(aliases),
			'search_tokens', to_jsonb(search_tokens)
		)) AS entity
	FROM entities
)
SELECT json_build_object(
	'page', 'entities',
	'generated_at', NOW(),
	'total_entities', (SELECT COUNT(*)::int FROM entity_json),
	'entities', COALESCE(
		(SELECT jsonb_agg(entity ORDER BY type, sport, name) FROM entity_json),
		'[]'::jsonb
	)
)`

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

		// Universal home-page autofill directory. Cross-sport and intentionally
		// text-only; profile hydration and stat-definition payloads stay on
		// sport-scoped /{sport}/autofill.
		"entities_directory": universalEntitiesStatement,

		// Data API (canonical sport routes)
		// Rating leaderboard (migrations 027/028 — the z-score rating engine).
		// Type-aware: entity_type=player ⇒ player board (player_stats); entity_type=team
		// ⇒ team board (team_stats, flat scarcity-z). Composite + Specialist (+ specialty)
		// in ONE payload per row. Reads the shared *_stats/players/teams tables — join
		// caveat: players & teams are keyed by (id, sport), so every join needs AND .sport=.
		// $1 sport · $2 season (NULL ⇒ latest rated) · $3 scope (composite|specialist|
		// <skill label>) · $4 position (player only) · $5 league_id · $6 limit (NULL ⇒ 50) ·
		// $7 entity_type (NULL ⇒ player).
		"leaderboard": `WITH req AS (
			SELECT upper($1::text) AS sport,
			       $2::int AS season,
			       COALESCE(NULLIF(lower($3::text), ''), 'composite') AS scope,
			       NULLIF($4::text, '') AS position,
			       $5::int AS league_id,
			       COALESCE($6::int, 50) AS lim,
			       COALESCE(NULLIF(lower($7::text), ''), 'player') AS entity_type,
			       NULLIF($8::text, '') AS conference,
			       NULLIF($9::text, '') AS division,
			       $10::int AS team_id,
			       NULLIF($11::text, '') AS position_group
		),
		season_pick AS (
			SELECT COALESCE(
				(SELECT season FROM req WHERE season IS NOT NULL),
				(SELECT MAX(tr.season) FROM public.team_rosters tr, req
				  WHERE req.entity_type = 'player' AND req.team_id IS NOT NULL
				    AND tr.sport = req.sport AND tr.team_id = req.team_id AND tr.is_active),
				(SELECT MAX(s) FROM (
					SELECT ps.season AS s FROM public.player_stats ps, req
					 WHERE req.entity_type = 'player' AND ps.sport = req.sport AND ps.rating_composite IS NOT NULL
					UNION ALL
					SELECT ts.season FROM public.team_stats ts, req
					 WHERE req.entity_type = 'team' AND ts.sport = req.sport AND ts.rating_composite IS NOT NULL
				) ss)
			) AS season
		),
		avail_seasons AS (
			SELECT COALESCE(array_agg(s ORDER BY s DESC), ARRAY[]::int[]) AS seasons FROM (
				SELECT DISTINCT ps.season AS s FROM public.player_stats ps, req
				 WHERE req.entity_type = 'player' AND ps.sport = req.sport AND ps.rating_composite IS NOT NULL
				UNION
				SELECT DISTINCT tr.season FROM public.team_rosters tr, req
				 WHERE req.entity_type = 'player' AND req.team_id IS NOT NULL
				   AND tr.sport = req.sport AND tr.team_id = req.team_id AND tr.is_active
				UNION
				SELECT DISTINCT ts.season FROM public.team_stats ts, req
				 WHERE req.entity_type = 'team' AND ts.sport = req.sport AND ts.rating_composite IS NOT NULL
			) ss
		),
		player_base AS (
			SELECT * FROM (
					SELECT 'player'::text AS entity_type, p.id, p.name, p.photo_url AS image,
						COALESCE(ps.position, tr.position) AS position,
						tr.team_id AS team_id,
						t.name AS team_name, t.short_code AS team_code, t.logo_url AS team_logo,
					COALESCE(NULLIF(ps.league_id, 0), t.league_id) AS league_id,
					ps.rating_composite,
					CASE WHEN req.scope IN ('composite', 'fantasy') OR lower(ps.rating_specialty) = req.scope THEN ps.rating_specialist END AS rating_peak,
					CASE WHEN req.scope IN ('composite', 'fantasy') OR lower(ps.rating_specialty) = req.scope THEN ps.rating_specialty END AS rating_peak_label,
					ps.rating_composite_rank,
					CASE WHEN req.scope IN ('composite', 'fantasy') OR lower(ps.rating_specialty) = req.scope THEN ps.rating_specialist_rank END AS rating_peak_rank,
					ps.rating_composite_score,
					CASE WHEN req.scope IN ('composite', 'fantasy') OR lower(ps.rating_specialty) = req.scope THEN ps.rating_specialist_score END AS rating_peak_score,
					(ps.stats->>'fantasy_points')::numeric AS fantasy_points,
					(ps.percentiles->>'fantasy_points')::numeric AS fantasy_rank,
					CASE WHEN req.scope = 'composite' THEN ps.rating_composite
					     WHEN req.scope = 'fantasy' THEN (ps.stats->>'fantasy_points')::numeric
					     WHEN lower(ps.rating_specialty) = req.scope THEN ps.rating_specialist END AS sort_metric
				FROM public.team_rosters tr
				JOIN public.players p ON p.id = tr.player_id AND p.sport = tr.sport
				LEFT JOIN public.player_stats ps ON ps.player_id = tr.player_id AND ps.sport = tr.sport AND ps.season = (SELECT season FROM season_pick)
				LEFT JOIN public.teams t ON t.id = tr.team_id AND t.sport = tr.sport
				CROSS JOIN req
				WHERE req.entity_type = 'player' AND req.team_id IS NOT NULL
				  AND tr.sport = req.sport AND tr.season = (SELECT season FROM season_pick)
				  AND tr.team_id = req.team_id AND tr.is_active
				  AND (req.position IS NULL OR COALESCE(ps.position, tr.position) = req.position)
				  AND (req.position_group IS NULL OR COALESCE(public.position_group(tr.sport, COALESCE(ps.position, tr.position)), tr.position_group) = req.position_group)
				  AND (req.league_id IS NULL OR COALESCE(NULLIF(ps.league_id, 0), t.league_id, 0) = req.league_id)
			) roster_players
			UNION ALL
			SELECT * FROM (
				SELECT 'player'::text AS entity_type, p.id, p.name, p.photo_url AS image,
					ps.position, ps.team_id, t.name AS team_name, t.short_code AS team_code, t.logo_url AS team_logo,
					NULLIF(ps.league_id, 0) AS league_id,
					ps.rating_composite, ps.rating_specialist AS rating_peak, ps.rating_specialty AS rating_peak_label,
					ps.rating_composite_rank, ps.rating_specialist_rank AS rating_peak_rank,
					ps.rating_composite_score, ps.rating_specialist_score AS rating_peak_score,
					(ps.stats->>'fantasy_points')::numeric AS fantasy_points,
					(ps.percentiles->>'fantasy_points')::numeric AS fantasy_rank,
					CASE WHEN req.scope = 'composite' THEN ps.rating_composite
					     WHEN req.scope = 'fantasy' THEN (ps.stats->>'fantasy_points')::numeric
					     ELSE ps.rating_specialist END AS sort_metric
				FROM public.player_stats ps
				JOIN public.players p ON p.id = ps.player_id AND p.sport = ps.sport
				LEFT JOIN public.teams t ON t.id = ps.team_id AND t.sport = ps.sport
				CROSS JOIN req
				WHERE req.entity_type = 'player' AND req.team_id IS NULL
				  AND ps.sport = req.sport AND ps.season = (SELECT season FROM season_pick)
				  AND ps.rating_composite IS NOT NULL
				  AND (req.position IS NULL OR ps.position = req.position)
				  AND (req.position_group IS NULL OR public.position_group(ps.sport, ps.position) = req.position_group)
				  AND (req.league_id IS NULL OR COALESCE(ps.league_id, 0) = req.league_id)
				  AND (req.scope IN ('composite', 'fantasy') OR lower(ps.rating_specialty) = req.scope)
				  AND (req.scope <> 'fantasy' OR COALESCE((ps.stats->>'fantasy_points')::numeric, 0) > 0)
			) rated_players
		),
		team_base AS (
			SELECT 'team'::text AS entity_type, t.id, t.name, t.logo_url AS image,
				NULL::text AS position, t.id AS team_id, t.name AS team_name, t.short_code AS team_code, t.logo_url AS team_logo,
				NULLIF(ts.league_id, 0) AS league_id,
				ts.rating_composite, ts.rating_specialist AS rating_peak, ts.rating_specialty AS rating_peak_label,
				ts.rating_composite_rank, ts.rating_specialist_rank AS rating_peak_rank,
				ts.rating_composite_score, ts.rating_specialist_score AS rating_peak_score,
				NULL::numeric AS fantasy_points,
				NULL::numeric AS fantasy_rank,
				CASE WHEN req.scope = 'composite' THEN ts.rating_composite ELSE ts.rating_specialist END AS sort_metric
			FROM public.team_stats ts
			JOIN public.teams t ON t.id = ts.team_id AND t.sport = ts.sport
			CROSS JOIN req
			WHERE req.entity_type = 'team' AND ts.sport = req.sport AND ts.season = (SELECT season FROM season_pick)
			  AND ts.rating_composite IS NOT NULL
			  AND (req.team_id IS NULL OR ts.team_id = req.team_id)
			  AND (req.league_id IS NULL OR COALESCE(ts.league_id, 0) = req.league_id)
			  AND (req.conference IS NULL OR t.conference = req.conference)
			  AND (req.division IS NULL OR t.division = req.division)
			  AND (req.scope IN ('composite') OR lower(ts.rating_specialty) = req.scope)
		),
		cohort AS (
			SELECT * FROM player_base
			UNION ALL
			SELECT * FROM team_base
		),
		ranked AS (
			SELECT entity_type, id, name, image, position, team_id, team_name, team_code, team_logo, league_id,
			       rating_composite, rating_peak, rating_peak_label, rating_composite_rank, rating_peak_rank,
			       rating_composite_score, rating_peak_score, fantasy_points, fantasy_rank,
			       CASE WHEN sort_metric IS NULL THEN NULL::bigint
			            ELSE row_number() OVER (PARTITION BY (sort_metric IS NULL) ORDER BY sort_metric DESC NULLS LAST, rating_composite DESC NULLS LAST, name) END AS rank
			FROM cohort
			ORDER BY sort_metric IS NULL, sort_metric DESC NULLS LAST, rating_composite DESC NULLS LAST, name
			LIMIT (SELECT CASE WHEN entity_type = 'player' AND team_id IS NOT NULL THEN NULL ELSE lim END FROM req)
		)
		SELECT json_build_object(
			'page', 'leaderboard',
			'sport', lower((SELECT sport FROM req)),
			'entity_type', (SELECT entity_type FROM req),
			'season', (SELECT season FROM season_pick),
			'available_seasons', (SELECT seasons FROM avail_seasons),
			'scope', (SELECT scope FROM req),
			'count', (SELECT count(*) FROM ranked),
			'leaders', COALESCE(
				(SELECT json_agg(row_to_json(ranked) ORDER BY ranked.rank) FROM ranked),
				'[]'::json
			)
		)`,

		// Vibes leaderboard — the sport-wide sentiment board. Each entity's LATEST
		// scored row in the 48h window (DISTINCT ON), ranked by sentiment desc. Joined
		// to players/teams so the row carries name/image/team (one shape across every
		// single-entity board the /leaderboard page renders). The partial index
		// idx_vibe_scores_sport_sentiment covers the inner scan.
		// $1 sport · $2 limit (NULL ⇒ 50) · $3 entity_type (NULL ⇒ both).
		"vibes_leaderboard": `WITH req AS (
			SELECT upper($1::text) AS sport,
			       COALESCE($2::int, 50) AS lim,
			       NULLIF(lower($3::text), '') AS entity_type,
			       $4::int AS league_id,
			       $5::int AS team_id,
			       NULLIF($6::text, '') AS position,
			       NULLIF($7::text, '') AS position_group,
			       NULLIF($8::text, '') AS conference,
			       NULLIF($9::text, '') AS division
		),
		-- Canonical latest-generation rule: take each
		-- entity's latest vibe within the 48h window REGARDLESS of nullability
		-- (latest_raw), then drop it if that latest generation is a no-corpus marker
		-- (sentiment NULL). A newer marker thus clears the entity from the board
		-- instead of leaving an older scored row ranked.
		latest_raw AS (
			SELECT DISTINCT ON (vs.entity_type, vs.entity_id)
			       vs.entity_type, vs.entity_id, vs.sentiment AS score, vs.prompt AS blurb, vs.generated_at
			FROM public.vibe_scores vs, req
			WHERE vs.sport = req.sport
			  AND (req.entity_type IS NULL OR vs.entity_type = req.entity_type)
			  AND vs.generated_at > NOW() - INTERVAL '48 hours'
			ORDER BY vs.entity_type, vs.entity_id, vs.generated_at DESC
		),
		latest AS (
			SELECT * FROM latest_raw WHERE score IS NOT NULL
		),
		ranked AS (
			SELECT u.*, row_number() OVER (ORDER BY u.score DESC, u.generated_at DESC) AS rank
			FROM (
				-- PLAYER
				SELECT 'player'::text AS entity_type, p.id, p.name, p.photo_url AS image,
				       cur.team_id, t.name AS team_name, t.short_code AS team_code, t.logo_url AS team_logo,
				       l.score, l.blurb, l.generated_at
				FROM latest l
				JOIN public.players p ON p.id = l.entity_id AND p.sport = (SELECT sport FROM req)
				LEFT JOIN public.player_current_identity cur ON cur.player_id = p.id AND cur.sport = (SELECT sport FROM req)
				LEFT JOIN public.teams t ON t.id = cur.team_id AND t.sport = (SELECT sport FROM req)
				WHERE l.entity_type = 'player'
				  AND ((SELECT team_id FROM req) IS NULL OR cur.team_id = (SELECT team_id FROM req))
				  AND ((SELECT league_id FROM req) IS NULL OR COALESCE(cur.league_id, t.league_id, 0) = (SELECT league_id FROM req))
				  AND ((SELECT position FROM req) IS NULL OR cur.position = (SELECT position FROM req))
				  AND ((SELECT position_group FROM req) IS NULL OR COALESCE(cur.position_group, public.position_group(p.sport, cur.position)) = (SELECT position_group FROM req))
				UNION ALL
				-- TEAM
				SELECT 'team'::text AS entity_type, t.id, t.name, t.logo_url AS image,
				       t.id AS team_id, t.name AS team_name, t.short_code AS team_code, t.logo_url AS team_logo,
				       l.score, l.blurb, l.generated_at
				FROM latest l
				JOIN public.teams t ON t.id = l.entity_id AND t.sport = (SELECT sport FROM req)
				WHERE l.entity_type = 'team'
				  AND ((SELECT team_id FROM req) IS NULL OR t.id = (SELECT team_id FROM req))
				  AND ((SELECT league_id FROM req) IS NULL OR COALESCE(t.league_id, 0) = (SELECT league_id FROM req))
				  AND ((SELECT conference FROM req) IS NULL OR t.conference = (SELECT conference FROM req))
				  AND ((SELECT division FROM req) IS NULL OR t.division = (SELECT division FROM req))
			) u
			ORDER BY u.score DESC, u.generated_at DESC
			LIMIT (SELECT lim FROM req)
		)
		SELECT json_build_object(
			'page', 'vibes_leaderboard',
			'sport', lower((SELECT sport FROM req)),
			'entity_type', COALESCE((SELECT entity_type FROM req), 'all'),
			'count', (SELECT count(*) FROM ranked),
			'leaders', COALESCE(
				(SELECT json_agg(row_to_json(ranked) ORDER BY ranked.rank) FROM ranked),
				'[]'::json
			)
		)`,

		// Sigil leaderboard (Optimization Ledger O19) — the sport-wide CROWN board: entities
		// ranked by their LATEST Sigil synthesis score (1-100), the holistic Rating+Vibe crown
		// the Product Narrative wants stack-ranked at the front door. Mirrors vibes_leaderboard's
		// shape exactly (DISTINCT ON latest scored row, enriched name/image/team) so the
		// /leaderboard page renders one row shape across every board. Sources sigil_synthesis
		// (append-only, latest-per-entity); the partial index idx_sigil_synthesis_sport_score
		// — (sport, score DESC, generated_at DESC) WHERE score IS NOT NULL AND blurb IS NOT NULL
		// — covers the inner scan. Carries previous_score so the front door can show the crown's
		// delta (sibling boards don't have a native previous; the Sigil synthesis does).
		// $1 sport · $2 limit (NULL ⇒ 50) · $3 entity_type (NULL ⇒ both) · $4 season
		// (NULL ⇒ live/current view).
		"sigil_leaderboard": `WITH req AS (
			SELECT upper($1::text) AS sport,
			       COALESCE($2::int, 50) AS lim,
			       NULLIF(lower($3::text), '') AS entity_type,
			       $4::int AS want_season,
			       (SELECT current_season FROM public.sports WHERE id = upper($1::text)) AS cur_season,
			       $5::int AS league_id,
			       $6::int AS team_id,
			       NULLIF($7::text, '') AS position,
			       NULLIF($8::text, '') AS position_group,
			       NULLIF($9::text, '') AS conference,
			       NULLIF($10::text, '') AS division
		),
		-- Canonical latest-generation rule: take each
		-- entity's latest synthesis REGARDLESS of nullability (latest_raw), then drop
		-- it if that latest generation is a no-pillar marker (score/blurb NULL). A
		-- newer marker therefore removes the entity from the crown board instead of the
		-- old behavior, which filtered markers BEFORE the DISTINCT ON and left a stale
		-- scored row ranked.
		-- Season scope: no ?season ⇒ the LIVE view (current season + legacy
		-- NULL-season rows), so an older season's crown can never rank as current; an
		-- explicit ?season=N ranks that season's board exactly.
		latest_raw AS (
			SELECT DISTINCT ON (ss.entity_type, ss.entity_id)
			       ss.entity_type, ss.entity_id, ss.score, ss.previous_score, ss.blurb, ss.generated_at
			FROM public.sigil_synthesis ss, req
			WHERE ss.sport = req.sport
			  AND (req.entity_type IS NULL OR ss.entity_type = req.entity_type)
			  AND CASE WHEN req.want_season IS NULL
			           THEN (ss.season = req.cur_season OR ss.season IS NULL)
			           ELSE ss.season = req.want_season END
			ORDER BY ss.entity_type, ss.entity_id, ss.generated_at DESC
		),
		latest AS (
			-- F7: mirror the entity_sigil profile's 72h freshness gate (see its vibe_cur
			-- CTE, ~L1133) so the board only crowns entities the profile corroborates —
			-- otherwise an entity whose latest scored synthesis is >72h old ranks here
			-- (with recap) while the profile returns current:null (the recap/score
			-- mismatch bug). SHARED CONSTANT: both windows are INTERVAL '72 hours' — keep
			-- them EQUAL or the mismatch returns. Explicit ?season=N keeps the no-window
			-- final-crown behavior, exactly matching entity_sigil.
			SELECT lr.* FROM latest_raw lr, req
			WHERE lr.score IS NOT NULL AND lr.blurb IS NOT NULL
			  AND (req.want_season IS NOT NULL OR lr.generated_at > NOW() - INTERVAL '72 hours')
		),
		ranked AS (
			SELECT u.*, row_number() OVER (ORDER BY u.score DESC, u.generated_at DESC) AS rank
			FROM (
				-- PLAYER
				SELECT 'player'::text AS entity_type, p.id, p.name, p.photo_url AS image,
				       cur.team_id, t.name AS team_name, t.short_code AS team_code, t.logo_url AS team_logo,
				       l.score, l.previous_score, l.blurb, l.generated_at
				FROM latest l
				JOIN public.players p ON p.id = l.entity_id AND p.sport = (SELECT sport FROM req)
				LEFT JOIN public.player_current_identity cur ON cur.player_id = p.id AND cur.sport = (SELECT sport FROM req)
				LEFT JOIN public.teams t ON t.id = cur.team_id AND t.sport = (SELECT sport FROM req)
				WHERE l.entity_type = 'player'
				  AND ((SELECT team_id FROM req) IS NULL OR cur.team_id = (SELECT team_id FROM req))
				  AND ((SELECT league_id FROM req) IS NULL OR COALESCE(cur.league_id, t.league_id, 0) = (SELECT league_id FROM req))
				  AND ((SELECT position FROM req) IS NULL OR cur.position = (SELECT position FROM req))
				  AND ((SELECT position_group FROM req) IS NULL OR COALESCE(cur.position_group, public.position_group(p.sport, cur.position)) = (SELECT position_group FROM req))
				UNION ALL
				-- TEAM
				SELECT 'team'::text AS entity_type, t.id, t.name, t.logo_url AS image,
				       t.id AS team_id, t.name AS team_name, t.short_code AS team_code, t.logo_url AS team_logo,
				       l.score, l.previous_score, l.blurb, l.generated_at
				FROM latest l
				JOIN public.teams t ON t.id = l.entity_id AND t.sport = (SELECT sport FROM req)
				WHERE l.entity_type = 'team'
				  AND ((SELECT team_id FROM req) IS NULL OR t.id = (SELECT team_id FROM req))
				  AND ((SELECT league_id FROM req) IS NULL OR COALESCE(t.league_id, 0) = (SELECT league_id FROM req))
				  AND ((SELECT conference FROM req) IS NULL OR t.conference = (SELECT conference FROM req))
				  AND ((SELECT division FROM req) IS NULL OR t.division = (SELECT division FROM req))
			) u
			ORDER BY u.score DESC, u.generated_at DESC
			LIMIT (SELECT lim FROM req)
		)
		SELECT json_build_object(
			'page', 'sigil_leaderboard',
			'sport', lower((SELECT sport FROM req)),
			'entity_type', COALESCE((SELECT entity_type FROM req), 'all'),
			'season', COALESCE((SELECT want_season FROM req), (SELECT cur_season FROM req)),
			'count', (SELECT count(*) FROM ranked),
			'leaders', COALESCE(
				(SELECT json_agg(row_to_json(ranked) ORDER BY ranked.rank) FROM ranked),
				'[]'::json
			)
		)`,

		// Momentum leaderboards — the RISERS. Read the maintained current-row
		// projection over durable momentum_scores snapshots; the profile /momentum
		// endpoint still exposes raw trajectory context for one entity.
		"trending_vibe_leaderboard": `WITH req AS (
			SELECT upper($1::text) AS sport,
			       COALESCE($2::int, 30) AS lim,
			       NULLIF(lower($3::text), '') AS entity_type,
			       $4::int AS league_id,
			       $5::int AS team_id,
			       NULLIF($6::text, '') AS position,
			       NULLIF($7::text, '') AS position_group,
			       NULLIF($8::text, '') AS conference,
			       NULLIF($9::text, '') AS division
		),
		-- latest_momentum_scores_per_entity is already the current-row projection,
		-- but keep the old DISTINCT ON boundary so tied top-N output stays
		-- byte-identical while the scan shrinks from full history to current rows.
		latest_raw AS (
			SELECT DISTINCT ON (ms.entity_type, ms.entity_id)
			       ms.entity_type, ms.entity_id, ms.team_id, ms.league_id, ms.position, ms.position_group,
			       ms.conference, ms.division, ms.vibe_slope AS slope, ms.vibe_samples AS samples, ms.generated_at
			FROM public.latest_momentum_scores_per_entity ms, req
			WHERE ms.sport = req.sport
			  AND (req.entity_type IS NULL OR ms.entity_type = req.entity_type)
			ORDER BY ms.entity_type, ms.entity_id, ms.generated_at DESC
		),
		latest AS (
			-- Filter after latest_raw so an entity whose latest slope turned
			-- negative or NULL cannot keep ranking on an older positive snapshot.
			SELECT * FROM latest_raw WHERE slope IS NOT NULL AND slope > 0
		),
		ranked AS (
			SELECT u.*, row_number() OVER (ORDER BY u.slope DESC) AS rank FROM (
				SELECT 'player'::text AS entity_type, p.id, p.name, p.photo_url AS image,
				       l.team_id, t.name AS team_name, t.short_code AS team_code, t.logo_url AS team_logo,
				       round(l.slope::numeric, 1) AS score, round(l.slope::numeric, 3) AS slope
				FROM latest l
				JOIN public.players p ON p.id = l.entity_id AND p.sport = (SELECT sport FROM req)
				LEFT JOIN public.teams t ON t.id = l.team_id AND t.sport = (SELECT sport FROM req)
				WHERE l.entity_type = 'player'
				  AND ((SELECT team_id FROM req) IS NULL OR l.team_id = (SELECT team_id FROM req))
				  AND ((SELECT league_id FROM req) IS NULL OR COALESCE(l.league_id, t.league_id, 0) = (SELECT league_id FROM req))
				  AND ((SELECT position FROM req) IS NULL OR l.position = (SELECT position FROM req))
				  AND ((SELECT position_group FROM req) IS NULL OR COALESCE(l.position_group, public.position_group(p.sport, l.position)) = (SELECT position_group FROM req))
				UNION ALL
				SELECT 'team'::text, t.id, t.name, t.logo_url AS image,
				       t.id AS team_id, t.name AS team_name, t.short_code AS team_code, t.logo_url AS team_logo,
				       round(l.slope::numeric, 1) AS score, round(l.slope::numeric, 3) AS slope
				FROM latest l
				JOIN public.teams t ON t.id = l.entity_id AND t.sport = (SELECT sport FROM req)
				WHERE l.entity_type = 'team'
				  AND ((SELECT team_id FROM req) IS NULL OR t.id = (SELECT team_id FROM req))
				  AND ((SELECT league_id FROM req) IS NULL OR COALESCE(l.league_id, t.league_id, 0) = (SELECT league_id FROM req))
				  AND ((SELECT conference FROM req) IS NULL OR COALESCE(l.conference, t.conference) = (SELECT conference FROM req))
				  AND ((SELECT division FROM req) IS NULL OR COALESCE(l.division, t.division) = (SELECT division FROM req))
			) u ORDER BY u.slope DESC LIMIT (SELECT lim FROM req)
		)
		SELECT json_build_object(
			'page', 'trending_leaderboard', 'metric', 'vibe',
			'sport', lower((SELECT sport FROM req)),
			'entity_type', COALESCE((SELECT entity_type FROM req), 'all'),
			'count', (SELECT count(*) FROM ranked),
			'leaders', COALESCE((SELECT json_agg(row_to_json(ranked) ORDER BY ranked.rank) FROM ranked), '[]'::json)
		)`,

		"trending_rating_leaderboard": `WITH req AS (
			SELECT upper($1::text) AS sport,
			       COALESCE($2::int, 30) AS lim,
			       NULLIF(lower($3::text), '') AS entity_type,
			       $4::int AS league_id,
			       $5::int AS team_id,
			       NULLIF($6::text, '') AS position,
			       NULLIF($7::text, '') AS position_group,
			       NULLIF($8::text, '') AS conference,
			       NULLIF($9::text, '') AS division
		),
		-- Same latest_raw-then-filter shape as the vibe board above; keeping
		-- DISTINCT ON here preserves tied top-N output while sourcing current rows.
		latest_raw AS (
			SELECT DISTINCT ON (ms.entity_type, ms.entity_id)
			       ms.entity_type, ms.entity_id, ms.team_id, ms.league_id, ms.position, ms.position_group,
			       ms.conference, ms.division, ms.rating_slope AS slope, ms.rating_samples AS samples, ms.generated_at
			FROM public.latest_momentum_scores_per_entity ms, req
			WHERE ms.sport = req.sport
			  AND (req.entity_type IS NULL OR ms.entity_type = req.entity_type)
			ORDER BY ms.entity_type, ms.entity_id, ms.generated_at DESC
		),
		latest AS (
			SELECT * FROM latest_raw WHERE slope IS NOT NULL AND slope > 0
		),
		ranked AS (
			SELECT u.*, row_number() OVER (ORDER BY u.slope DESC) AS rank FROM (
				SELECT 'player'::text AS entity_type, p.id, p.name, p.photo_url AS image,
				       l.team_id, t.name AS team_name, t.short_code AS team_code, t.logo_url AS team_logo,
				       round(l.slope::numeric, 1) AS score, round(l.slope::numeric, 3) AS slope
				FROM latest l
				JOIN public.players p ON p.id = l.entity_id AND p.sport = (SELECT sport FROM req)
				LEFT JOIN public.teams t ON t.id = l.team_id AND t.sport = (SELECT sport FROM req)
				WHERE l.entity_type = 'player'
				  AND ((SELECT team_id FROM req) IS NULL OR l.team_id = (SELECT team_id FROM req))
				  AND ((SELECT league_id FROM req) IS NULL OR COALESCE(l.league_id, t.league_id, 0) = (SELECT league_id FROM req))
				  AND ((SELECT position FROM req) IS NULL OR l.position = (SELECT position FROM req))
				  AND ((SELECT position_group FROM req) IS NULL OR COALESCE(l.position_group, public.position_group(p.sport, l.position)) = (SELECT position_group FROM req))
				UNION ALL
				SELECT 'team'::text, t.id, t.name, t.logo_url AS image,
				       t.id AS team_id, t.name AS team_name, t.short_code AS team_code, t.logo_url AS team_logo,
				       round(l.slope::numeric, 1) AS score, round(l.slope::numeric, 3) AS slope
				FROM latest l
				JOIN public.teams t ON t.id = l.entity_id AND t.sport = (SELECT sport FROM req)
				WHERE l.entity_type = 'team'
				  AND ((SELECT team_id FROM req) IS NULL OR t.id = (SELECT team_id FROM req))
				  AND ((SELECT league_id FROM req) IS NULL OR COALESCE(l.league_id, t.league_id, 0) = (SELECT league_id FROM req))
				  AND ((SELECT conference FROM req) IS NULL OR COALESCE(l.conference, t.conference) = (SELECT conference FROM req))
				  AND ((SELECT division FROM req) IS NULL OR COALESCE(l.division, t.division) = (SELECT division FROM req))
			) u ORDER BY u.slope DESC LIMIT (SELECT lim FROM req)
		)
		SELECT json_build_object(
			'page', 'trending_leaderboard', 'metric', 'rating',
			'sport', lower((SELECT sport FROM req)),
			'entity_type', COALESCE((SELECT entity_type FROM req), 'all'),
			'count', (SELECT count(*) FROM ranked),
			'leaders', COALESCE((SELECT json_agg(row_to_json(ranked) ORDER BY ranked.rank) FROM ranked), '[]'::json)
		)`,

		// Narratives leaderboard (two-rail model) — the sport's HOTTEST narratives:
		// each entity's top narrative in the selected scope, ranked by impact.
		// Supersedes the raw mention-count and standalone headlines boards.
		// $1 sport · $2 limit · $3 entity_type · $4 scope
		"narratives_leaderboard": `WITH req AS (
			SELECT upper($1::text) AS sport,
			       COALESCE($2::int, 50) AS lim,
			       NULLIF(lower($3::text), '') AS entity_type,
			       CASE NULLIF(lower($4::text), '')
			         WHEN 'last_week' THEN 'last_week'
			         WHEN 'two_weeks_ago' THEN 'two_weeks_ago'
			         WHEN 'three_weeks_ago' THEN 'three_weeks_ago'
			         WHEN 'last_month' THEN 'last_month'
			         ELSE 'current_week'
			       END AS scope_key,
			       $5::int AS league_id,
			       $6::int AS team_id,
			       NULLIF($7::text, '') AS position,
			       NULLIF($8::text, '') AS position_group,
			       NULLIF($9::text, '') AS conference,
			       NULLIF($10::text, '') AS division
		),
		scope AS (
			SELECT scope_key,
			       CASE scope_key
			         WHEN 'last_week' THEN 'Last week'
			         WHEN 'two_weeks_ago' THEN 'Two weeks ago'
			         WHEN 'three_weeks_ago' THEN 'Three weeks ago'
			         WHEN 'last_month' THEN 'Last month'
			         ELSE 'Current week'
			       END AS label,
			       CASE scope_key
			         WHEN 'last_week' THEN NOW() - INTERVAL '14 days'
			         WHEN 'two_weeks_ago' THEN NOW() - INTERVAL '21 days'
			         WHEN 'three_weeks_ago' THEN NOW() - INTERVAL '28 days'
			         WHEN 'last_month' THEN NOW() - INTERVAL '30 days'
			         ELSE NOW() - INTERVAL '7 days'
			       END AS starts_at,
			       CASE scope_key
			         WHEN 'last_week' THEN NOW() - INTERVAL '7 days'
			         WHEN 'two_weeks_ago' THEN NOW() - INTERVAL '14 days'
			         WHEN 'three_weeks_ago' THEN NOW() - INTERVAL '21 days'
			         ELSE NOW()
			       END AS ends_at
			FROM req
		),
		-- News is an archive-like product: a later no-narratives marker means "no new
		-- story this run", not "erase this week's story". Pick the latest content
		-- generation inside the selected scope; the current-week freshness gate below
		-- still ages out cooling stories.
		latest_gen AS (
			SELECT ns.entity_type, ns.entity_id, max(ns.generated_at) AS gen
			FROM public.news_summaries ns, req, scope
			WHERE ns.sport = req.sport
			  AND (req.entity_type IS NULL OR ns.entity_type = req.entity_type)
			  AND ns.body IS NOT NULL AND ns.impact IS NOT NULL
			  AND ns.generated_at >= scope.starts_at
			  AND ns.generated_at < scope.ends_at
			GROUP BY ns.entity_type, ns.entity_id
		),
		latest AS (
			SELECT DISTINCT ON (ns.entity_type, ns.entity_id)
			       ns.entity_type, ns.entity_id, ns.narrative_title, ns.body, ns.impact,
			       COALESCE(ns.narrative_updated_at, ns.source_latest_at, ns.generated_at) AS updated_at,
			       ns.source_count, ns.source_names, ns.source_latest_at, ns.source_oldest_at,
			       ns.trajectory,
			       CASE ns.trajectory
			         WHEN 'heating_up' THEN 'Heating up'
			         WHEN 'cooling_off' THEN 'Cooling off'
			         ELSE 'Developing story...'
			       END AS trajectory_label,
			       ns.generated_at
			FROM public.news_summaries ns
			JOIN latest_gen lg ON lg.entity_type = ns.entity_type AND lg.entity_id = ns.entity_id
			                  AND ns.generated_at = lg.gen
			CROSS JOIN req
			WHERE ns.sport = req.sport
			  AND ns.body IS NOT NULL AND ns.impact IS NOT NULL
			  AND ((SELECT scope_key FROM scope) <> 'current_week'
			       OR COALESCE(ns.trajectory, 'developing_story') <> 'cooling_off'
			       OR COALESCE(ns.narrative_updated_at, ns.source_latest_at, ns.generated_at) > NOW() - INTERVAL '3 days')
			ORDER BY ns.entity_type, ns.entity_id, ns.impact DESC
		),
		ranked AS (
			SELECT u.*, row_number() OVER (ORDER BY u.score DESC, u.generated_at DESC) AS rank
			FROM (
				SELECT 'player'::text AS entity_type, p.id, p.name, p.photo_url AS image,
				       cur.team_id, t.name AS team_name, t.short_code AS team_code, t.logo_url AS team_logo,
				       l.narrative_title, l.body, l.impact AS score,
				       l.updated_at, l.source_count, l.source_names, l.source_latest_at, l.source_oldest_at,
				       l.trajectory, l.trajectory_label, l.generated_at
				FROM latest l
				JOIN public.players p ON p.id = l.entity_id AND p.sport = (SELECT sport FROM req)
				LEFT JOIN public.player_current_identity cur ON cur.player_id = p.id AND cur.sport = (SELECT sport FROM req)
				LEFT JOIN public.teams t ON t.id = cur.team_id AND t.sport = (SELECT sport FROM req)
				WHERE l.entity_type = 'player'
				  AND ((SELECT team_id FROM req) IS NULL OR cur.team_id = (SELECT team_id FROM req))
				  AND ((SELECT league_id FROM req) IS NULL OR COALESCE(cur.league_id, t.league_id, 0) = (SELECT league_id FROM req))
				  AND ((SELECT position FROM req) IS NULL OR cur.position = (SELECT position FROM req))
				  AND ((SELECT position_group FROM req) IS NULL OR COALESCE(cur.position_group, public.position_group(p.sport, cur.position)) = (SELECT position_group FROM req))
				UNION ALL
				SELECT 'team'::text AS entity_type, t.id, t.name, t.logo_url AS image,
				       t.id AS team_id, t.name AS team_name, t.short_code AS team_code, t.logo_url AS team_logo,
				       l.narrative_title, l.body, l.impact AS score,
				       l.updated_at, l.source_count, l.source_names, l.source_latest_at, l.source_oldest_at,
				       l.trajectory, l.trajectory_label, l.generated_at
				FROM latest l
				JOIN public.teams t ON t.id = l.entity_id AND t.sport = (SELECT sport FROM req)
				WHERE l.entity_type = 'team'
				  AND ((SELECT team_id FROM req) IS NULL OR t.id = (SELECT team_id FROM req))
				  AND ((SELECT league_id FROM req) IS NULL OR COALESCE(t.league_id, 0) = (SELECT league_id FROM req))
				  AND ((SELECT conference FROM req) IS NULL OR t.conference = (SELECT conference FROM req))
				  AND ((SELECT division FROM req) IS NULL OR t.division = (SELECT division FROM req))
			) u
			ORDER BY u.score DESC, u.generated_at DESC
			LIMIT (SELECT lim FROM req)
		)
		SELECT json_build_object(
			'page', 'news_leaderboard',
			'sport', lower((SELECT sport FROM req)),
			'entity_type', COALESCE((SELECT entity_type FROM req), 'all'),
			'scope', (SELECT json_build_object('key', scope_key, 'label', label, 'starts_at', starts_at, 'ends_at', ends_at) FROM scope),
			'count', (SELECT count(*) FROM ranked),
			'leaders', COALESCE((SELECT json_agg(row_to_json(ranked) ORDER BY ranked.rank) FROM ranked), '[]'::json)
		)`,

		// Transfers leaderboard — the sport-wide "hottest rumors" board. The
		// sport-scoped sibling of team_transfers/player_suitors: latest row per
		// (team, player) pair (DISTINCT ON), model-vetted (is_rumor IS TRUE), ranked
		// by heat desc. Each row carries BOTH sides of the pair (player + team).
		// $1 sport · $2 limit (NULL ⇒ 50) · $3 scope · shared cohort filters.
		"transfers_leaderboard": `WITH req AS (
			SELECT upper($1::text) AS sport,
			       COALESCE($2::int, 50) AS lim,
			       CASE NULLIF(lower($3::text), '')
			         WHEN 'last_week' THEN 'last_week'
			         WHEN 'two_weeks_ago' THEN 'two_weeks_ago'
			         WHEN 'three_weeks_ago' THEN 'three_weeks_ago'
			         WHEN 'last_month' THEN 'last_month'
			         ELSE 'current_week'
			       END AS scope_key,
			       NULLIF(lower($4::text), '') AS entity_type,
			       $5::int AS league_id,
			       $6::int AS team_id,
			       NULLIF($7::text, '') AS position,
			       NULLIF($8::text, '') AS position_group,
			       NULLIF($9::text, '') AS conference,
			       NULLIF($10::text, '') AS division
		),
		scope AS (
			SELECT scope_key,
			       CASE scope_key
			         WHEN 'last_week' THEN 'Last week'
			         WHEN 'two_weeks_ago' THEN 'Two weeks ago'
			         WHEN 'three_weeks_ago' THEN 'Three weeks ago'
			         WHEN 'last_month' THEN 'Last month'
			         ELSE 'Current week'
			       END AS label,
			       CASE scope_key
			         WHEN 'last_week' THEN NOW() - INTERVAL '14 days'
			         WHEN 'two_weeks_ago' THEN NOW() - INTERVAL '21 days'
			         WHEN 'three_weeks_ago' THEN NOW() - INTERVAL '28 days'
			         WHEN 'last_month' THEN NOW() - INTERVAL '30 days'
			         ELSE NOW() - INTERVAL '7 days'
			       END AS starts_at,
			       CASE scope_key
			         WHEN 'last_week' THEN NOW() - INTERVAL '7 days'
			         WHEN 'two_weeks_ago' THEN NOW() - INTERVAL '14 days'
			         WHEN 'three_weeks_ago' THEN NOW() - INTERVAL '21 days'
			         ELSE NOW()
			       END AS ends_at
			FROM req
		),
		latest AS (
			-- Newest row per pair regardless of verdict, so a fresh "cleared"
			-- supersedes an older heat-only seed row.
			SELECT DISTINCT ON (tr.team_id, tr.player_id)
			       tr.team_id, tr.player_id, tr.heat, tr.heat_components,
				       tr.direction, tr.stage, tr.model_summary, tr.source_attribution,
			       tr.is_rumor,
			       COALESCE(tr.rumor_updated_at, tr.source_latest_at, tr.generated_at) AS updated_at,
			       tr.source_count, tr.source_names, tr.source_latest_at, tr.source_oldest_at,
			       tr.trajectory,
			       CASE tr.trajectory
			         WHEN 'heating_up' THEN 'Heating up'
			         WHEN 'cooling_off' THEN 'Cooling off'
			         ELSE 'Developing story...'
			       END AS trajectory_label,
			       tr.trajectory_components,
			       tr.generated_at
			FROM public.transfer_rumors tr, req, scope
			WHERE tr.sport = req.sport
			  AND tr.generated_at >= scope.starts_at
			  AND tr.generated_at < scope.ends_at
			ORDER BY tr.team_id, tr.player_id, tr.generated_at DESC
		),
		ranked AS (
			SELECT p.id AS player_id, p.name AS player_name, p.photo_url AS player_image,
			       t.id AS team_id, t.name AS team_name, t.short_code AS team_code, t.logo_url AS team_logo,
			       l.heat, l.heat_components, l.direction, l.stage,
				       l.model_summary AS summary, l.source_attribution,
			       l.updated_at, l.source_count, l.source_names, l.source_latest_at, l.source_oldest_at,
			       l.trajectory, l.trajectory_label, l.trajectory_components, l.generated_at,
			       row_number() OVER (ORDER BY l.heat DESC NULLS LAST, l.generated_at DESC) AS rank
			FROM latest l
				JOIN public.players p ON p.id = l.player_id AND p.sport = (SELECT sport FROM req)
				JOIN public.teams t ON t.id = l.team_id AND t.sport = (SELECT sport FROM req)
				LEFT JOIN public.player_current_identity pci ON pci.player_id = p.id AND pci.sport = p.sport
				LEFT JOIN public.teams current_team ON current_team.id = pci.team_id AND current_team.sport = p.sport
				-- is_rumor IS TRUE = model-vetted; heat > 0 drops zero-signal stragglers.
				WHERE l.is_rumor IS TRUE AND l.heat > 0
				  AND ((SELECT entity_type FROM req) IS NULL
				       OR ((SELECT entity_type FROM req) = 'player' AND pci.player_id IS NOT NULL)
				       OR ((SELECT entity_type FROM req) = 'team'))
				  AND ((SELECT team_id FROM req) IS NULL OR l.team_id = (SELECT team_id FROM req) OR pci.team_id = (SELECT team_id FROM req))
				  AND ((SELECT league_id FROM req) IS NULL OR COALESCE(pci.league_id, current_team.league_id, t.league_id, 0) = (SELECT league_id FROM req))
				  AND ((SELECT position FROM req) IS NULL OR pci.position = (SELECT position FROM req))
				  AND ((SELECT position_group FROM req) IS NULL OR COALESCE(pci.position_group, public.position_group(p.sport, pci.position)) = (SELECT position_group FROM req))
				  AND ((SELECT conference FROM req) IS NULL OR current_team.conference = (SELECT conference FROM req) OR t.conference = (SELECT conference FROM req))
				  AND ((SELECT division FROM req) IS NULL OR current_team.division = (SELECT division FROM req) OR t.division = (SELECT division FROM req))
				  AND ((SELECT scope_key FROM scope) <> 'current_week'
				       OR COALESCE(l.trajectory, 'developing_story') <> 'cooling_off'
				       OR l.updated_at > NOW() - INTERVAL '3 days')
			  AND NOT (
			      (pci.team_id IS NOT NULL AND pci.team_id = l.team_id AND COALESCE(l.direction, '') = 'incoming')
			      OR (pci.team_id IS NOT NULL AND pci.team_id <> l.team_id AND COALESCE(l.direction, '') = 'outgoing')
			  )
		)
		SELECT json_build_object(
			'page', 'transfers_leaderboard',
			'sport', lower((SELECT sport FROM req)),
			'scope', (SELECT json_build_object('key', scope_key, 'label', label, 'starts_at', starts_at, 'ends_at', ends_at) FROM scope),
			'count', (SELECT count(*) FROM ranked),
			'rumors', COALESCE(
				(SELECT json_agg(row_to_json(ranked) ORDER BY ranked.rank) FROM ranked WHERE ranked.rank <= (SELECT lim FROM req)),
				'[]'::json
			)
		)`,

		// Roster (rating engine, per team) — every player on the team's season
		// roster with their season Composite + Specialist (+ ranks + specialty),
		// ranked by the SUM of Composite + Specialist (a single "total impact"
		// order). $1 sport · $2 team_id · $3 season (NULL ⇒ latest rated) · $4 league_id.
		"roster": `WITH req AS (
			SELECT upper($1::text) AS sport, $2::int AS team_id,
			       $3::int AS season, $4::int AS league_id
		),
		season_pick AS (
			SELECT COALESCE(
				(SELECT season FROM req WHERE season IS NOT NULL),
				(SELECT MAX(ps.season) FROM public.player_stats ps, req
				  WHERE ps.sport = req.sport AND ps.team_id = req.team_id
				    AND ps.rating_composite IS NOT NULL)
			) AS season
		),
		ranked AS (
			SELECT p.id, p.name, p.photo_url AS image, ps.position, (ps.stats->>'fantasy_points')::numeric AS fantasy_points,
				ps.rating_composite, ps.rating_specialist AS rating_peak, ps.rating_specialty AS rating_peak_label,
				ps.rating_composite_rank, ps.rating_specialist_rank AS rating_peak_rank,
				ps.rating_composite_score, ps.rating_specialist_score AS rating_peak_score,
				row_number() OVER (
					ORDER BY (COALESCE(ps.rating_composite, 0) + COALESCE(ps.rating_specialist, 0)) DESC
				) AS rank
			FROM public.player_stats ps
			JOIN public.players p ON p.id = ps.player_id AND p.sport = ps.sport
			CROSS JOIN req CROSS JOIN season_pick sp
			WHERE ps.sport = req.sport AND ps.team_id = req.team_id AND ps.season = sp.season
			  AND ps.rating_composite IS NOT NULL
			  AND (req.league_id IS NULL OR COALESCE(ps.league_id, 0) = req.league_id)
		)
		SELECT json_build_object(
			'page', 'roster',
			'sport', lower((SELECT sport FROM req)),
			'team_id', (SELECT team_id FROM req),
			'season', (SELECT season FROM season_pick),
			'count', (SELECT count(*) FROM ranked),
			'players', COALESCE(
				(SELECT json_agg(row_to_json(ranked) ORDER BY ranked.rank) FROM ranked),
				'[]'::json
			)
		)`,

		// --- Per-product news source (split from entity_news_rail) ---
		// One self-contained product per card: /news (narratives), /transfers (the
		// vetted rumor heat list), and /sigil (crown synthesis). Each card fetches
		// its own product. $1 sport · $2 entity_type · $3 entity_id · $4 scope.
		"entity_news": `WITH req AS (
			SELECT upper($1::text) AS sport,
			       lower($2::text) AS entity_type,
			       $3::int AS entity_id,
			       CASE NULLIF(lower($4::text), '')
			         WHEN 'last_week' THEN 'last_week'
			         WHEN 'two_weeks_ago' THEN 'two_weeks_ago'
			         WHEN 'three_weeks_ago' THEN 'three_weeks_ago'
			         WHEN 'last_month' THEN 'last_month'
			         ELSE 'current_week'
			       END AS scope_key
		),
		scope AS (
			SELECT scope_key,
			       CASE scope_key
			         WHEN 'last_week' THEN 'Last week'
			         WHEN 'two_weeks_ago' THEN 'Two weeks ago'
			         WHEN 'three_weeks_ago' THEN 'Three weeks ago'
			         WHEN 'last_month' THEN 'Last month'
			         ELSE 'Current week'
			       END AS label,
			       CASE scope_key
			         WHEN 'last_week' THEN NOW() - INTERVAL '14 days'
			         WHEN 'two_weeks_ago' THEN NOW() - INTERVAL '21 days'
			         WHEN 'three_weeks_ago' THEN NOW() - INTERVAL '28 days'
			         WHEN 'last_month' THEN NOW() - INTERVAL '30 days'
			         ELSE NOW() - INTERVAL '7 days'
			       END AS starts_at,
			       CASE scope_key
			         WHEN 'last_week' THEN NOW() - INTERVAL '7 days'
			         WHEN 'two_weeks_ago' THEN NOW() - INTERVAL '14 days'
			         WHEN 'three_weeks_ago' THEN NOW() - INTERVAL '21 days'
			         ELSE NOW()
			       END AS ends_at
			FROM req
		),
		narr AS (
			-- News is an archive-like product: a later no-narratives marker means "no
			-- new story this run", not "erase this week's story". Return the latest
			-- content generation inside the selected scope; the current-week freshness
			-- gate below still ages out cooling stories.
			SELECT ns.narrative_title, ns.body, ns.impact, ns.impact_components,
			       ns.input_news_ids,
			       COALESCE(ns.narrative_updated_at, ns.source_latest_at, ns.generated_at) AS updated_at,
			       ns.source_count, ns.source_names, ns.source_latest_at, ns.source_oldest_at,
			       ns.trajectory,
			       CASE ns.trajectory
			         WHEN 'heating_up' THEN 'Heating up'
			         WHEN 'cooling_off' THEN 'Cooling off'
			         ELSE 'Developing story...'
			       END AS trajectory_label,
			       ns.trajectory_components,
			       ns.model_version, ns.prompt_version, ns.generated_at
			FROM public.news_summaries ns CROSS JOIN req CROSS JOIN scope
			WHERE ns.entity_type = req.entity_type AND ns.entity_id = req.entity_id AND ns.sport = req.sport
			  AND ns.body IS NOT NULL
			  AND (scope.scope_key <> 'current_week'
			       OR COALESCE(ns.trajectory, 'developing_story') <> 'cooling_off'
			       OR COALESCE(ns.narrative_updated_at, ns.source_latest_at, ns.generated_at) > NOW() - INTERVAL '3 days')
			  AND ns.generated_at = (
			      SELECT max(generated_at) FROM public.news_summaries
			      WHERE entity_type = (SELECT entity_type FROM req) AND entity_id = (SELECT entity_id FROM req)
			        AND sport = (SELECT sport FROM req)
			        AND body IS NOT NULL
			        AND generated_at >= (SELECT starts_at FROM scope)
			        AND generated_at < (SELECT ends_at FROM scope)
			  )
		)
		SELECT json_build_object(
			'page', 'news',
			'sport', lower((SELECT sport FROM req)),
			'entity_type', (SELECT entity_type FROM req),
			'entity_id', (SELECT entity_id FROM req),
			'scope', (SELECT json_build_object('key', scope_key, 'label', label, 'starts_at', starts_at, 'ends_at', ends_at) FROM scope),
			'narratives', COALESCE((SELECT json_agg(row_to_json(n) ORDER BY n.impact DESC NULLS LAST)
			     FROM (SELECT narrative_title, body, impact, impact_components, input_news_ids,
			                  updated_at, source_count, source_names, source_latest_at, source_oldest_at,
			                  trajectory, trajectory_label, trajectory_components,
			                  model_version, prompt_version, generated_at
			           FROM narr) n), '[]'::json)
		)`,
		"entity_transfers": `WITH req AS (
			SELECT upper($1::text) AS sport,
			       lower($2::text) AS entity_type,
			       $3::int AS entity_id,
			       CASE NULLIF(lower($4::text), '')
			         WHEN 'last_week' THEN 'last_week'
			         WHEN 'two_weeks_ago' THEN 'two_weeks_ago'
			         WHEN 'three_weeks_ago' THEN 'three_weeks_ago'
			         WHEN 'last_month' THEN 'last_month'
			         ELSE 'current_week'
			       END AS scope_key
		),
		scope AS (
			SELECT scope_key,
			       CASE scope_key
			         WHEN 'last_week' THEN 'Last week'
			         WHEN 'two_weeks_ago' THEN 'Two weeks ago'
			         WHEN 'three_weeks_ago' THEN 'Three weeks ago'
			         WHEN 'last_month' THEN 'Last month'
			         ELSE 'Current week'
			       END AS label,
			       CASE scope_key
			         WHEN 'last_week' THEN NOW() - INTERVAL '14 days'
			         WHEN 'two_weeks_ago' THEN NOW() - INTERVAL '21 days'
			         WHEN 'three_weeks_ago' THEN NOW() - INTERVAL '28 days'
			         WHEN 'last_month' THEN NOW() - INTERVAL '30 days'
			         ELSE NOW() - INTERVAL '7 days'
			       END AS starts_at,
			       CASE scope_key
			         WHEN 'last_week' THEN NOW() - INTERVAL '7 days'
			         WHEN 'two_weeks_ago' THEN NOW() - INTERVAL '14 days'
			         WHEN 'three_weeks_ago' THEN NOW() - INTERVAL '21 days'
			         ELSE NOW()
			       END AS ends_at
			FROM req
		),
		tr_latest AS (
			SELECT DISTINCT ON (tr.team_id, tr.player_id)
			       tr.team_id, tr.player_id, tr.heat, tr.heat_components, tr.direction, tr.stage,
				       tr.model_summary, tr.source_attribution, tr.is_rumor,
			       COALESCE(tr.rumor_updated_at, tr.source_latest_at, tr.generated_at) AS updated_at,
			       tr.source_count, tr.source_names, tr.source_latest_at, tr.source_oldest_at,
			       tr.trajectory,
			       CASE tr.trajectory
			         WHEN 'heating_up' THEN 'Heating up'
			         WHEN 'cooling_off' THEN 'Cooling off'
			         ELSE 'Developing story...'
			       END AS trajectory_label,
			       tr.trajectory_components,
			       tr.generated_at
			FROM public.transfer_rumors tr CROSS JOIN req CROSS JOIN scope
			WHERE tr.sport = req.sport
			  AND ( (req.entity_type = 'team'   AND tr.team_id   = req.entity_id)
			     OR (req.entity_type = 'player' AND tr.player_id = req.entity_id) )
			  AND tr.generated_at >= scope.starts_at
			  AND tr.generated_at < scope.ends_at
			ORDER BY tr.team_id, tr.player_id, tr.generated_at DESC
		),
		tr_ranked AS (
			SELECT
			    CASE WHEN (SELECT entity_type FROM req) = 'team' THEN p.id        ELSE t.id       END AS id,
			    CASE WHEN (SELECT entity_type FROM req) = 'team' THEN p.name      ELSE t.name     END AS name,
			    CASE WHEN (SELECT entity_type FROM req) = 'team' THEN p.photo_url ELSE t.logo_url END AS image,
				    l.heat, l.heat_components, l.direction, l.stage, l.model_summary AS summary, l.source_attribution,
			    l.updated_at, l.source_count, l.source_names, l.source_latest_at, l.source_oldest_at,
			    l.trajectory, l.trajectory_label, l.trajectory_components,
			    row_number() OVER (ORDER BY l.heat DESC NULLS LAST) AS rank
			FROM tr_latest l
			LEFT JOIN public.players p ON (SELECT entity_type FROM req) = 'team'   AND p.id = l.player_id AND p.sport = (SELECT sport FROM req)
			LEFT JOIN public.teams   t ON (SELECT entity_type FROM req) = 'player' AND t.id = l.team_id   AND t.sport = (SELECT sport FROM req)
			LEFT JOIN public.player_current_identity pci ON pci.player_id = l.player_id AND pci.sport = (SELECT sport FROM req)
			WHERE l.is_rumor IS TRUE AND l.heat > 0
			  AND ((SELECT scope_key FROM scope) <> 'current_week'
			       OR COALESCE(l.trajectory, 'developing_story') <> 'cooling_off'
			       OR l.updated_at > NOW() - INTERVAL '3 days')
			  AND NOT (
			      (pci.team_id IS NOT NULL AND pci.team_id = l.team_id AND COALESCE(l.direction, '') = 'incoming')
			      OR (pci.team_id IS NOT NULL AND pci.team_id <> l.team_id AND COALESCE(l.direction, '') = 'outgoing')
			  )
		)
		SELECT json_build_object(
			'page', 'transfers',
			'sport', lower((SELECT sport FROM req)),
			'entity_type', (SELECT entity_type FROM req),
			'entity_id', (SELECT entity_id FROM req),
			'scope', (SELECT json_build_object('key', scope_key, 'label', label, 'starts_at', starts_at, 'ends_at', ends_at) FROM scope),
			'transfers', COALESCE((SELECT json_agg(row_to_json(x) ORDER BY x.rank)
			     FROM (SELECT id, name, image, heat, heat_components, direction, stage, summary, source_attribution,
			                  updated_at, source_count, source_names, source_latest_at, source_oldest_at,
			                  trajectory, trajectory_label, trajectory_components, rank
			           FROM tr_ranked WHERE rank <= 25) x), '[]'::json)
		)`,
		// $1 sport · $2 entity_type · $3 entity_id · $4 season (NULL ⇒ live/current view).
		"entity_sigil": `WITH req AS (
			SELECT upper($1::text) AS sport, lower($2::text) AS entity_type, $3::int AS entity_id,
			       $4::int AS want_season,
			       (SELECT current_season FROM public.sports WHERE id = upper($1::text)) AS cur_season
		),
		-- Season scope: no ?season ⇒ the LIVE view — the
		-- current season plus legacy NULL-season rows (the pre-S12 event-driven default) —
		-- so synthesizing an OLDER season can never become the current crown. An explicit
		-- ?season=N selects that season exactly (its final crown, no freshness window).
		latest_gen AS (
			-- Canonical latest-generation rule: the latest synthesis WITHIN
			-- the season scope, REGARDLESS of nullability.
			SELECT max(vs.generated_at) AS g
			FROM public.sigil_synthesis vs, req
			WHERE vs.entity_type = req.entity_type AND vs.entity_id = req.entity_id AND vs.sport = req.sport
			  AND CASE WHEN req.want_season IS NULL
			           THEN (vs.season = req.cur_season OR vs.season IS NULL)
			           ELSE vs.season = req.want_season END
		),
		vibe_cur AS (
			-- Surface the latest generation only when it is a real scored row AND (for the
			-- live view) fresh. A newer no-pillar marker (score NULL) is the latest
			-- generation → the score IS NOT NULL guard yields zero rows → current null,
			-- clearing the stale crown. (History below keeps only scored points — a marker
			-- changes the current projection, never the sparkline.)
			SELECT vs.score, vs.blurb, vs.convergence, vs.disagreement, vs.why_now,
			       vs.previous_score, vs.model_version, vs.prompt_version, vs.generated_at
			FROM public.sigil_synthesis vs, req, latest_gen
			WHERE vs.entity_type = req.entity_type AND vs.entity_id = req.entity_id AND vs.sport = req.sport
			  AND vs.generated_at = latest_gen.g
			  AND vs.score IS NOT NULL
			  -- SHARED CONSTANT with sigil_leaderboard's 'latest' CTE (F7): both gate the
			  -- live crown behind INTERVAL '72 hours'. Keep them equal — a drift reopens the
			  -- recap/score mismatch (board shows a crown the profile has cleared).
			  AND (req.want_season IS NOT NULL OR vs.generated_at > NOW() - INTERVAL '72 hours')
		),
		vibe_hist AS (
			SELECT vs.score, vs.generated_at
			FROM public.sigil_synthesis vs, req
			WHERE vs.entity_type = req.entity_type AND vs.entity_id = req.entity_id AND vs.sport = req.sport
			  AND vs.score IS NOT NULL
			  AND CASE WHEN req.want_season IS NULL
			           THEN (vs.season = req.cur_season OR vs.season IS NULL)
			           ELSE vs.season = req.want_season END
			ORDER BY vs.generated_at DESC LIMIT 14
		)
		SELECT json_build_object(
			'page', 'sigil',
			'sport', lower((SELECT sport FROM req)),
			'entity_type', (SELECT entity_type FROM req),
			'entity_id', (SELECT entity_id FROM req),
			'season', COALESCE((SELECT want_season FROM req), (SELECT cur_season FROM req)),
			'current', (SELECT row_to_json(v) FROM (SELECT score, blurb, convergence, disagreement, why_now, previous_score, model_version, prompt_version, generated_at FROM vibe_cur) v),
			'history', COALESCE((SELECT json_agg(json_build_object('score', score, 'generated_at', generated_at) ORDER BY generated_at DESC) FROM vibe_hist), '[]'::json)
		)`,
		// Entity meta (two-rail model) — per-entity IDENTITY for the page header: name,
		// image, physicals, current team/club and position (player_current_identity),
		// tier. UNION-gated on entity_type so a missing entity returns 0
		// rows (404). This hydrates the page-header island directly; the only local
		// frontend search DB should be the universal /api/v1/entities directory.
		// $1 sport · $2 entity_type · $3 entity_id.
		"entity_meta": `WITH req AS (
			SELECT upper($1::text) AS sport, lower($2::text) AS entity_type, $3::int AS entity_id
		)
		SELECT json_build_object(
			'entity_type', 'player', 'id', p.id, 'sport', lower(p.sport), 'name', p.name,
			'first_name', p.first_name, 'last_name', p.last_name, 'image', p.photo_url,
			'nationality', p.nationality, 'date_of_birth', p.date_of_birth, 'height', p.height, 'weight', p.weight,
			'position', NULLIF(pci.position, 'Unknown'), 'tier', p.tier,
			'team', CASE WHEN t.id IS NOT NULL THEN json_build_object('id', t.id, 'name', t.name, 'short_code', t.short_code, 'image', t.logo_url) ELSE NULL END
		) AS meta
		FROM public.players p
		LEFT JOIN public.player_current_identity pci ON pci.player_id = p.id AND pci.sport = p.sport
		LEFT JOIN public.teams t ON t.id = pci.team_id AND t.sport = p.sport
		WHERE (SELECT entity_type FROM req) = 'player' AND p.id = (SELECT entity_id FROM req) AND p.sport = (SELECT sport FROM req)
		UNION ALL
		SELECT json_build_object(
			'entity_type', 'team', 'id', t.id, 'sport', lower(t.sport), 'name', t.name, 'image', t.logo_url,
			'short_code', t.short_code, 'country', t.country, 'city', t.city, 'venue', t.venue_name,
			'conference', t.conference, 'division', t.division, 'tier', t.tier
		)
		FROM public.teams t
		WHERE (SELECT entity_type FROM req) = 'team' AND t.id = (SELECT entity_id FROM req) AND t.sport = (SELECT sport FROM req)`,

		// --- Per-product stats source (split from sparkline) ---
		// Live routing:
		//   /stats   = the full season rating (Composite card + ContentShell controls) — THIS statement;
		//   /rating  = the PEAK scouting-report projection + stat commentary ("entity_rating" statement);
		//   /momentum absorbs the per-event series (built in trendsStatement, GetTrendsPage).
		// The heavy fantasy/template/datapoints blocks live only in /stats. $1 sport · $2 type ·
		// $3 id · $4 season (NULL ⇒ latest rated) · $5 league_id.
		"entity_stats": `WITH req AS (
			SELECT upper($1::text) AS sport, lower($2::text) AS etype,
			       $3::int AS eid, $4::int AS season, $5::int AS league_id
		),
		season_pick AS (
			SELECT COALESCE(
				(SELECT season FROM req WHERE season IS NOT NULL),
				(SELECT MAX(s) FROM (
					SELECT ps.season AS s FROM public.player_stats ps, req
					 WHERE req.etype = 'player' AND ps.sport = req.sport AND ps.player_id = req.eid
					   AND (ps.rating_composite IS NOT NULL OR ps.rating_breakdown IS NOT NULL)
					   AND (req.league_id IS NULL OR COALESCE(ps.league_id, 0) = req.league_id)
					UNION ALL
					SELECT ts.season FROM public.team_stats ts, req
					 WHERE req.etype = 'team' AND ts.sport = req.sport AND ts.team_id = req.eid
					   AND ts.rating_composite IS NOT NULL
					   AND (req.league_id IS NULL OR COALESCE(ts.league_id, 0) = req.league_id)
				) ss)
			) AS season
		),
		season_rating AS (
			SELECT season, league_id, position, rating_composite, rating_composite_rank, rating_composite_score,
			       rating_peak, rating_peak_rank, rating_peak_score, rating_peak_label, rating_breakdown,
			       rating_categories, rating_scoped_ranks, rating_scoped_scores, rating_modes, conference, division, team, fantasy, template, datapoints FROM (
				SELECT ps.season, NULLIF(ps.league_id, 0) AS league_id, ps.position,
				       ps.rating_composite, ps.rating_composite_rank, ps.rating_composite_score,
				       ps.rating_specialist AS rating_peak, ps.rating_specialist_rank AS rating_peak_rank, ps.rating_specialist_score AS rating_peak_score, ps.rating_specialty AS rating_peak_label, ps.rating_breakdown,
				       NULL::jsonb AS rating_categories, ps.rating_scoped_ranks, ps.rating_scoped_scores, CASE WHEN ps.rating_modes IS NULL THEN NULL ELSE COALESCE((SELECT jsonb_object_agg(rm_k, (rm_v - 'specialist' - 'specialist_rank' - 'specialist_score' - 'specialty') || jsonb_strip_nulls(jsonb_build_object('peak', COALESCE(rm_v->'peak', rm_v->'specialist'), 'peak_rank', COALESCE(rm_v->'peak_rank', rm_v->'specialist_rank'), 'peak_score', COALESCE(rm_v->'peak_score', rm_v->'specialist_score'), 'peak_label', COALESCE(rm_v->'peak_label', rm_v->'specialty')))) FROM jsonb_each(ps.rating_modes) AS rm(rm_k, rm_v)), ps.rating_modes) END AS rating_modes,
				       NULL::text AS conference, NULL::text AS division,
				       CASE WHEN pt.id IS NULL THEN NULL::json
				            ELSE json_build_object('id', pt.id, 'name', pt.name, 'short_code', pt.short_code, 'logo_url', pt.logo_url) END AS team,
				       public.fantasy_block(ps.stats, ps.percentiles, ps.scoped_percentiles) AS fantasy,
				       public.template_block(ps.sport, ps.position, ps.stats, ps.percentiles, ps.scoped_percentiles) AS template,
				       public.datapoints_block(ps.sport, ps.stats, ps.percentiles, ps.scoped_percentiles) AS datapoints
				FROM public.player_stats ps CROSS JOIN req CROSS JOIN season_pick sp
				LEFT JOIN public.teams pt ON pt.id = NULLIF(ps.team_id, 0) AND pt.sport = ps.sport
				WHERE req.etype = 'player' AND ps.sport = req.sport
				  AND ps.player_id = req.eid AND ps.season = sp.season
				  AND (req.league_id IS NULL OR COALESCE(ps.league_id, 0) = req.league_id)
				UNION ALL
				SELECT ts.season, NULLIF(ts.league_id, 0), NULL::text,
				       ts.rating_composite, ts.rating_composite_rank, ts.rating_composite_score,
				       ts.rating_specialist AS rating_peak, ts.rating_specialist_rank AS rating_peak_rank, ts.rating_specialist_score AS rating_peak_score, ts.rating_specialty AS rating_peak_label, ts.rating_breakdown,
				       ts.rating_categories, ts.rating_scoped_ranks, ts.rating_scoped_scores, NULL::jsonb AS rating_modes,
				       tmc.conference, tmc.division,
				       json_build_object('id', tmc.id, 'name', tmc.name, 'short_code', tmc.short_code, 'logo_url', tmc.logo_url),
				       NULL::jsonb AS fantasy,
				       public.team_template_block(ts.sport, ts.stats, ts.percentiles, ts.scoped_percentiles) AS template,
				       public.team_datapoints_block(ts.sport, ts.stats, ts.percentiles, ts.scoped_percentiles) AS datapoints
				FROM public.team_stats ts
				JOIN public.teams tmc ON tmc.id = ts.team_id AND tmc.sport = ts.sport
				CROSS JOIN req CROSS JOIN season_pick sp
				WHERE req.etype = 'team' AND ts.sport = req.sport
				  AND ts.team_id = req.eid AND ts.season = sp.season
				  AND (req.league_id IS NULL OR COALESCE(ts.league_id, 0) = req.league_id)
			) u ORDER BY rating_composite DESC NULLS LAST, jsonb_array_length(rating_breakdown) DESC NULLS LAST LIMIT 1
		),
		event_series AS (
			SELECT fixture_id, start_time, rating_composite, rating_peak, rating_peak_label, rating_composite_pct, rating_peak_pct FROM (
				SELECT e.fixture_id, f.start_time,
				       e.rating_composite, e.rating_specialist AS rating_peak, e.rating_specialty AS rating_peak_label, e.rating_composite_pct, e.rating_specialist_pct AS rating_peak_pct
				FROM public.event_box_scores e
				JOIN public.fixtures f ON f.id = e.fixture_id
				CROSS JOIN req CROSS JOIN season_pick sp
				WHERE req.etype = 'player' AND e.sport = req.sport
				  AND e.player_id = req.eid AND e.season = sp.season
				  AND (req.league_id IS NULL OR COALESCE(e.league_id, 0) = req.league_id)
				  AND e.rating_composite IS NOT NULL
				UNION ALL
				SELECT e.fixture_id, f.start_time,
				       e.rating_composite, e.rating_specialist AS rating_peak, e.rating_specialty AS rating_peak_label, e.rating_composite_pct, e.rating_specialist_pct AS rating_peak_pct
				FROM public.event_team_stats e
				JOIN public.fixtures f ON f.id = e.fixture_id
				CROSS JOIN req CROSS JOIN season_pick sp
				WHERE req.etype = 'team' AND e.sport = req.sport
				  AND e.team_id = req.eid AND e.season = sp.season
				  AND (req.league_id IS NULL OR COALESCE(e.league_id, 0) = req.league_id)
				  AND e.rating_composite IS NOT NULL
			) u ORDER BY start_time
		)
		SELECT json_build_object(
			'page', 'stats',
			'sport', lower((SELECT sport FROM req)),
			'entity_type', (SELECT etype FROM req),
			'entity_id', (SELECT eid FROM req),
			'season', (SELECT season FROM season_pick),
			'available_seasons', COALESCE((
				SELECT array_agg(DISTINCT s ORDER BY s DESC) FROM (
					SELECT ps.season AS s FROM public.player_stats ps, req
					 WHERE req.etype = 'player' AND ps.sport = req.sport AND ps.player_id = req.eid
					   AND (ps.rating_composite IS NOT NULL OR ps.rating_breakdown IS NOT NULL)
					   AND (req.league_id IS NULL OR COALESCE(ps.league_id, 0) = req.league_id)
					UNION
					SELECT ts.season FROM public.team_stats ts, req
					 WHERE req.etype = 'team' AND ts.sport = req.sport AND ts.team_id = req.eid
					   AND ts.rating_composite IS NOT NULL
					   AND (req.league_id IS NULL OR COALESCE(ts.league_id, 0) = req.league_id)
				) seasons
			), '{}'::int[]),
			'rating', (SELECT row_to_json(season_rating) FROM season_rating),
			'events', COALESCE(
				(SELECT json_agg(row_to_json(es) ORDER BY es.start_time) FROM event_series es),
				'[]'::json
			)
		)`,
		"entity_rating": `WITH req AS (
			SELECT upper($1::text) AS sport, lower($2::text) AS etype,
			       $3::int AS eid, $4::int AS season, $5::int AS league_id
		),
		season_pick AS (
			SELECT COALESCE(
				(SELECT season FROM req WHERE season IS NOT NULL),
				(SELECT MAX(s) FROM (
					SELECT ps.season AS s FROM public.player_stats ps, req
					 WHERE req.etype = 'player' AND ps.sport = req.sport AND ps.player_id = req.eid
					   AND (ps.rating_composite IS NOT NULL OR ps.rating_breakdown IS NOT NULL)
					   AND (req.league_id IS NULL OR COALESCE(ps.league_id, 0) = req.league_id)
					UNION ALL
					SELECT ts.season FROM public.team_stats ts, req
					 WHERE req.etype = 'team' AND ts.sport = req.sport AND ts.team_id = req.eid
					   AND ts.rating_composite IS NOT NULL
					   AND (req.league_id IS NULL OR COALESCE(ts.league_id, 0) = req.league_id)
				) ss)
			) AS season
		),
		spec_rating AS (
			SELECT season, position, rating_peak, rating_peak_rank, rating_peak_score,
			       rating_peak_label, rating_composite_score, rating_composite_rank, rating_breakdown, rating_modes FROM (
				SELECT ps.season, ps.position, ps.rating_specialist AS rating_peak, ps.rating_specialist_rank AS rating_peak_rank, ps.rating_specialist_score AS rating_peak_score,
				       ps.rating_composite_score, ps.rating_composite_rank, ps.rating_specialty AS rating_peak_label, ps.rating_breakdown, CASE WHEN ps.rating_modes IS NULL THEN NULL ELSE COALESCE((SELECT jsonb_object_agg(rm_k, (rm_v - 'specialist' - 'specialist_rank' - 'specialist_score' - 'specialty') || jsonb_strip_nulls(jsonb_build_object('peak', COALESCE(rm_v->'peak', rm_v->'specialist'), 'peak_rank', COALESCE(rm_v->'peak_rank', rm_v->'specialist_rank'), 'peak_score', COALESCE(rm_v->'peak_score', rm_v->'specialist_score'), 'peak_label', COALESCE(rm_v->'peak_label', rm_v->'specialty')))) FROM jsonb_each(ps.rating_modes) AS rm(rm_k, rm_v)), ps.rating_modes) END AS rating_modes, ps.rating_composite
				FROM public.player_stats ps CROSS JOIN req CROSS JOIN season_pick sp
				WHERE req.etype = 'player' AND ps.sport = req.sport
				  AND ps.player_id = req.eid AND ps.season = sp.season
				  AND (req.league_id IS NULL OR COALESCE(ps.league_id, 0) = req.league_id)
				UNION ALL
				SELECT ts.season, NULL::text, ts.rating_specialist AS rating_peak, ts.rating_specialist_rank AS rating_peak_rank, ts.rating_specialist_score AS rating_peak_score,
				       ts.rating_composite_score, ts.rating_composite_rank, ts.rating_specialty AS rating_peak_label, ts.rating_breakdown, NULL::jsonb AS rating_modes, ts.rating_composite
				FROM public.team_stats ts CROSS JOIN req CROSS JOIN season_pick sp
				WHERE req.etype = 'team' AND ts.sport = req.sport
				  AND ts.team_id = req.eid AND ts.season = sp.season
				  AND (req.league_id IS NULL OR COALESCE(ts.league_id, 0) = req.league_id)
			) u ORDER BY rating_composite DESC NULLS LAST, jsonb_array_length(rating_breakdown) DESC NULLS LAST LIMIT 1
		)
		SELECT json_build_object(
			'page', 'rating',
			'sport', lower((SELECT sport FROM req)),
			'entity_type', (SELECT etype FROM req),
			'entity_id', (SELECT eid FROM req),
			'season', (SELECT season FROM season_pick),
			'rating', (SELECT row_to_json(spec_rating) FROM spec_rating),
			'commentary', (
				-- Canonical latest-generation rule: pick the
				-- latest commentary generation for this entity-season REGARDLESS of
				-- nullability (unfiltered max), then return it only when it carries a body.
				-- A newer no-stats marker (body NULL) becomes the latest generation and
				-- the body IS NOT NULL guard yields zero rows → null commentary, clearing
				-- stale prose. Season-scoped so a new season's content is independent.
				SELECT row_to_json(c) FROM (
					SELECT s.body, s.notability, s.notability_components, s.season, s.prompt_version, s.generated_at, s.divined_peak,
					       COALESCE(s.peak_trajectory, 'steady') AS peak_trajectory,
					       s.peak_trajectory_label,
					       s.peak_trajectory_components
					FROM public.stat_summaries s
					WHERE s.entity_type = (SELECT etype FROM req) AND s.entity_id = (SELECT eid FROM req)
					  AND s.sport = (SELECT sport FROM req) AND s.season = (SELECT season FROM season_pick)
					  AND s.body IS NOT NULL
					  AND s.generated_at = (
					      SELECT max(generated_at) FROM public.stat_summaries
					      WHERE entity_type = (SELECT etype FROM req) AND entity_id = (SELECT eid FROM req)
					        AND sport = (SELECT sport FROM req) AND season = (SELECT season FROM season_pick)
					  )
					ORDER BY s.generated_at DESC LIMIT 1
				) c
			)
		)`,
		"nba_meta_page": `WITH meta_info AS (
			SELECT
				COALESCE((SELECT version FROM public.sport_autofill_versions WHERE sport = 'NBA'), 1) AS version,
				COALESCE((SELECT generated_at FROM public.sport_autofill_versions WHERE sport = 'NBA'), '1970-01-01'::timestamptz) AS generated_at,
				(SELECT current_season FROM public.sports WHERE id = 'NBA') AS current_season,
				COALESCE((SELECT total_entities FROM public.sport_autofill_versions WHERE sport = 'NBA'), (SELECT COUNT(*)::int FROM nba.autofill_entities)) AS total_entities,
				COALESCE((SELECT status FROM public.sport_autofill_versions WHERE sport = 'NBA'), 'ready') AS status
		)
		SELECT json_build_object(
			'page', 'meta',
			'sport', 'nba',
			'scope', json_build_object('league_id', $1::int),
			'meta_version', (SELECT version::text FROM meta_info),
			'generated_at', (SELECT generated_at FROM meta_info),
			'autofill_status', (SELECT status FROM meta_info),
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
				COALESCE((SELECT version FROM public.sport_autofill_versions WHERE sport = 'NFL'), 1) AS version,
				COALESCE((SELECT generated_at FROM public.sport_autofill_versions WHERE sport = 'NFL'), '1970-01-01'::timestamptz) AS generated_at,
				(SELECT current_season FROM public.sports WHERE id = 'NFL') AS current_season,
				COALESCE((SELECT total_entities FROM public.sport_autofill_versions WHERE sport = 'NFL'), (SELECT COUNT(*)::int FROM nfl.autofill_entities)) AS total_entities,
				COALESCE((SELECT status FROM public.sport_autofill_versions WHERE sport = 'NFL'), 'ready') AS status
		)
		SELECT json_build_object(
			'page', 'meta',
			'sport', 'nfl',
			'scope', json_build_object('league_id', $1::int),
			'meta_version', (SELECT version::text FROM meta_info),
			'generated_at', (SELECT generated_at FROM meta_info),
			'autofill_status', (SELECT status FROM meta_info),
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
				COALESCE((SELECT version FROM public.sport_autofill_versions WHERE sport = 'FOOTBALL'), 1) AS version,
				COALESCE((SELECT generated_at FROM public.sport_autofill_versions WHERE sport = 'FOOTBALL'), '1970-01-01'::timestamptz) AS generated_at,
				(SELECT current_season FROM public.sports WHERE id = 'FOOTBALL') AS current_season,
				(SELECT COUNT(*)::int FROM football.autofill_entities 
				 WHERE ($1::int IS NULL OR COALESCE(league_id, 0) = $1::int)) AS total_entities,
				COALESCE((SELECT status FROM public.sport_autofill_versions WHERE sport = 'FOOTBALL'), 'ready') AS status
		)
		SELECT json_build_object(
			'page', 'meta',
			'sport', 'football',
			'scope', json_build_object('league_id', $1::int),
			'meta_version', (SELECT version::text FROM meta_info),
			'generated_at', (SELECT generated_at FROM meta_info),
			'autofill_status', (SELECT status FROM meta_info),
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
		"team_name_lookup": "SELECT name FROM teams WHERE id = $1 AND sport = $2",
		// O12: player_news_lookup + team_news_lookup removed with the live-RSS serving
		// handler (handler/news.go). team_name_lookup stays — notifications/store.go uses it.

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
	cohort_lookup AS (
		-- O1: the precomputed full-cohort season aggregate (peer_cohort_aggregate,
		-- refreshed nightly) for THIS entity's cohort — replacing the live per-read
		-- 248-member jsonb_each + AVG scan (~17.6 ms). Cohort key: the requesting
		-- entity's resolved position (player) or '' (team); league = COALESCE(
		-- effective_league, 0) — football resolves to the entity's own league,
		-- NBA/NFL collapse to 0 (their league_id is uniformly 0). A NULL player
		-- position matches no cohort (same as the prior ps.position = pp.position).
		SELECT pca.key_sums, pca.key_cnts, pca.score_sum, pca.score_cnt, pca.member_count
		FROM public.peer_cohort_aggregate pca, req, resolved_season rs, effective_league el
		WHERE pca.sport = '` + sportID + `'
		  AND pca.season = rs.season
		  AND pca.league_id = COALESCE(el.league_id, 0)
		  AND pca.entity_type = req.entity_type
		  AND pca.position = CASE WHEN req.entity_type = 'player'
		                          THEN (SELECT position FROM player_position)
		                          ELSE '' END
	),
	peer_aggregate AS (
		-- O1: reconstruct the EXACT leave-one-out cohort averages from the precompute
		-- by subtracting the entity's own normalized season values
		-- (entity_season_aggregate — identical normalization formula) and decrementing
		-- the per-key count. Bit-identical to the prior live cohort scan (validated
		-- across all sports × entity types incl. the highest/lowest-rated outliers).
		-- For keys the entity itself lacks, the full-cohort average already IS the
		-- leave-one-out average (it never contributed to that key). cohort_size and the
		-- peer season-score avg are likewise reconstructed leave-one-out (self is a
		-- cohort member iff it has a season stats row → entity_self_row exists).
		SELECT
			COALESCE((
				SELECT jsonb_object_agg(k, v) FROM (
					SELECT k,
						CASE WHEN esa.avgs ? k
							 THEN ((cl.key_sums->>k)::numeric - (esa.avgs->>k)::numeric)
							      / NULLIF((cl.key_cnts->>k)::numeric - 1, 0)
							 ELSE (cl.key_sums->>k)::numeric
							      / NULLIF((cl.key_cnts->>k)::numeric, 0)
						END AS v
					FROM cohort_lookup cl
					CROSS JOIN entity_season_aggregate esa
					CROSS JOIN LATERAL jsonb_object_keys(cl.key_sums) k
				) s WHERE v IS NOT NULL
			), '{}'::jsonb) AS avgs,
			COALESCE((
				SELECT cl.member_count - (CASE WHEN EXISTS (SELECT 1 FROM entity_self_row) THEN 1 ELSE 0 END)
				FROM cohort_lookup cl
			), 0) AS cohort_size,
			(
				SELECT ROUND(
					(cl.score_sum - COALESCE((SELECT season_composite_score FROM entity_self_row), 0))
					/ NULLIF(cl.score_cnt - (CASE WHEN (SELECT season_composite_score FROM entity_self_row) IS NOT NULL THEN 1 ELSE 0 END), 0),
					1)
				FROM cohort_lookup cl
			) AS score_avg
	),
	vibe_window AS (
		-- Last 7 days of sentiment scores (1-100) for this entity.
		-- vibe_scores is append-only (BIGSERIAL PK + INSERT-only writes).
		-- Legacy blurb-only rows have sentiment IS NULL — exclude them.
		SELECT vs.sentiment, vs.generated_at, vs.trigger_type
		FROM vibe_scores vs, req
		WHERE vs.entity_type = req.entity_type
		  AND vs.entity_id = req.entity_id
		  AND vs.sport = '` + sportID + `'
		  AND vs.sentiment IS NOT NULL
		  AND vs.generated_at >= NOW() - INTERVAL '7 days'
	),
	-- Season sentiment series: daily-bucketed sentiment averages from the start
	-- of the current sport+league season through NOW(). Drives the frontend's
	-- season-length sentiment sparkline on TrendsCard. Sibling to vibe_window
	-- (recent 7-day raw snapshot list); the two answer different questions and both stay.
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
		'page', 'momentum',
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
		'peer_season_score_avg',              (SELECT score_avg FROM peer_aggregate),
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
		'entity_season_sentiment_series', COALESCE((
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
