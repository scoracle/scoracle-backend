package maintenance

import (
	"context"
	"fmt"
	"log/slog"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// RefreshMaterializedViews refreshes all materialized views after ingestion.
// Uses CONCURRENTLY so reads are not blocked during refresh.
// Call this after a successful seed or fixture processing cycle.
func RefreshMaterializedViews(ctx context.Context, pool *pgxpool.Pool, logger *slog.Logger) error {
	views := []string{
		"mv_autofill_entities",
	}

	for _, v := range views {
		start := time.Now()
		_, err := pool.Exec(ctx, fmt.Sprintf("REFRESH MATERIALIZED VIEW CONCURRENTLY %s", v))
		dur := time.Since(start).Round(time.Millisecond)

		if err != nil {
			logger.Warn("Failed to refresh materialized view",
				"view", v, "duration", dur, "error", err)
			return fmt.Errorf("refresh %s: %w", v, err)
		}
		logger.Info("Refreshed materialized view", "view", v, "duration", dur)
	}
	return nil
}
