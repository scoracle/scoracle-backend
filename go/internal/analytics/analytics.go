// Package analytics retains the rating-bundle/trajectory parity interface.
// The production cohort producer lives in snapshot.Maintain and always uses
// bounded DuckDB computation; this probe selector does not change that producer.
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
