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

// HealthCheck runs a trivial query to verify the database is reachable.
func (p *Pool) HealthCheck(ctx context.Context) error {
	var n int
	return p.QueryRow(ctx, "health_check").Scan(&n)
}

// registerPreparedStatements registers all statements the API and ingestion
// layers use. Prepared statements eliminate parse overhead on every request.
func registerPreparedStatements(ctx context.Context, conn *pgx.Conn) error {
	stmts := map[string]string{
		// Health
		"health_check": "SELECT 1",

		// API: profiles (Postgres returns complete JSON)
		"api_player_profile": "SELECT api_player_profile($1, $2)",
		"api_team_profile":   "SELECT api_team_profile($1, $2)",

		// API: stats
		"api_entity_stats":      "SELECT api_entity_stats($1, $2, $3, $4, $5)",
		"api_available_seasons": "SELECT api_available_seasons($1, $2, $3)",

		// API: stat definitions
		"stat_definitions": "SELECT json_agg(row_to_json(sd) ORDER BY sd.sort_order) FROM stat_definitions sd WHERE sd.sport = $1",

		// API: stat leaders & standings (existing Postgres functions)
		"fn_stat_leaders": "SELECT * FROM fn_stat_leaders($1, $2, $3, $4, $5, $6)",
		"fn_standings":    "SELECT * FROM fn_standings($1, $2, $3, $4)",

		// API: bootstrap (materialized view)
		"autofill_entities": "SELECT json_agg(row_to_json(e)) FROM mv_autofill_entities e WHERE e.sport = $1",

		// API: news entity lookup
		"player_name_lookup": "SELECT name, first_name, last_name, team_id FROM players WHERE id = $1 AND sport = $2",
		"team_name_lookup":   "SELECT name FROM teams WHERE id = $1 AND sport = $2",
		"team_name_by_id":    "SELECT name FROM teams WHERE id = $1 AND sport = $2",

		// Ingestion: season validation
		"check_player_stats_season": "SELECT 1 FROM player_stats WHERE sport = $1 AND season = $2 LIMIT 1",
		"check_team_stats_season":   "SELECT 1 FROM team_stats WHERE sport = $1 AND season = $2 LIMIT 1",

		// Ingestion: percentile recalculation
		"recalculate_percentiles": "SELECT * FROM recalculate_percentiles($1, $2)",

		// Ingestion: provider season resolution
		"resolve_provider_season": "SELECT resolve_provider_season_id($1, $2)",

		// Ingestion: league lookup
		"league_lookup": "SELECT sportmonks_id, name FROM leagues WHERE id = $1",

		// Fixtures
		"get_pending_fixtures": "SELECT * FROM get_pending_fixtures($1, $2, $3)",
		"fixture_by_id":        "SELECT id, sport, league_id, season, home_team_id, away_team_id, start_time, seed_delay_hours, seed_attempts, external_id FROM fixtures WHERE id = $1",
		"fixture_start_time":   "SELECT start_time FROM fixtures WHERE id = $1",

		// Percentile archiving
		"archive_current_percentiles": "SELECT archive_current_percentiles($1, $2)",

		// Notifications
		"detect_percentile_changes": "SELECT * FROM detect_percentile_changes($1)",
		"get_entity_followers":      "SELECT uf.user_id, u.timezone FROM user_follows uf JOIN users u ON u.id = uf.user_id WHERE uf.entity_type = $1 AND uf.entity_id = $2 AND uf.sport = $3",
		"notification_player_name":  "SELECT name FROM players WHERE id = $1 AND sport = $2",
		"stat_display_name":         "SELECT display_name FROM stat_definitions WHERE sport = $1 AND key_name = $2 AND entity_type = $3",
		"get_user_device_tokens":    "SELECT token FROM user_devices WHERE user_id = $1 AND is_active = true",
	}

	for name, sql := range stmts {
		if _, err := conn.Prepare(ctx, name, sql); err != nil {
			return fmt.Errorf("prepare %q: %w", name, err)
		}
	}
	return nil
}
