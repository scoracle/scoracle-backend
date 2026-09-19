// Package analytics is the analytical boundary between Scoracle's application
// logic and its analytical engines. PostgreSQL remains the canonical system of
// record; engines behind this interface are either the existing Postgres path
// or the experimental DuckDB engine reading the same data. DuckDB owns
// nothing here: every implementation produces derived read models only.
//
// Selection is controlled by ANALYTICS_ENGINE ("postgres" | "duckdb") via
// config; the DuckDB path is experimental and must never become a default
// serving path while the POC is in progress.
package analytics

import (
	"context"
	"fmt"

	"github.com/albapepper/scoracle-data/internal/analytics/duckdb"
	"github.com/albapepper/scoracle-data/internal/analytics/model"
	"github.com/albapepper/scoracle-data/internal/analytics/postgres"
	"github.com/albapepper/scoracle-data/internal/config"
	"github.com/albapepper/scoracle-data/internal/db"
)

// Engine identifiers for config.AnalyticsEngine.
const (
	EnginePostgres = "postgres"
	EngineDuckDB   = "duckdb"
)

// Analytics is the boundary application code asks for context through.
// Implementations must not leak engine-specific SQL or types upward.
type Analytics interface {
	RatingTrajectory(ctx context.Context, entityType string, entityID int32, sport string, season int32) (model.Trajectory, error)
	RatingBundle(ctx context.Context, sport string, season int32, rateMode string) ([]model.BundleRow, error)
	Close(ctx context.Context) error
}

// Open returns the analytics engine selected by cfg.AnalyticsEngine. The
// Postgres implementation wraps the existing shared pool; the DuckDB
// implementation attaches the same database read-only.
func Open(ctx context.Context, cfg *config.Config, pool *db.Pool) (Analytics, error) {
	switch cfg.AnalyticsEngine {
	case EngineDuckDB:
		return duckdb.Open(ctx, duckdb.Options{
			DatabaseURL: cfg.DatabaseURL,
			Path:        cfg.AnalyticsDuckDBPath,
			MemoryLimit: cfg.AnalyticsDuckDBMemoryLimit,
		})
	case EnginePostgres:
		return postgres.New(pool), nil
	default:
		return nil, fmt.Errorf("unsupported analytics engine %q", cfg.AnalyticsEngine)
	}
}

// NewPostgres returns the canonical-path implementation over the shared pool.
func NewPostgres(pool *db.Pool) *postgres.Analytics {
	return postgres.New(pool)
}
