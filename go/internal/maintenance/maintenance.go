// Package maintenance runs periodic background tasks as Go tickers.
// Replaces pg_cron — all scheduled work is driven from Go since it is
// already a persistent, long-running service (required for LISTEN/NOTIFY).
package maintenance

import (
	"context"
	"log/slog"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// Config controls maintenance task intervals. Zero duration disables a task.
type Config struct {
	CleanupInterval time.Duration // Expired notifications + stale cache rows
	DigestInterval  time.Duration // Batch digest generation
	CatchUpInterval time.Duration // Sweep for missed NOTIFY events
}

// DefaultConfig returns sensible production defaults.
func DefaultConfig() Config {
	return Config{
		CleanupInterval: 30 * time.Minute,
		DigestInterval:  1 * time.Hour,
		CatchUpInterval: 15 * time.Minute,
	}
}

// Start launches all configured maintenance tickers. Blocks until ctx is
// cancelled. Intended to be called with `go`.
func Start(ctx context.Context, pool *pgxpool.Pool, cfg Config, logger *slog.Logger) {
	logger.Info("Maintenance tickers started",
		"cleanup", cfg.CleanupInterval,
		"digest", cfg.DigestInterval,
		"catchup", cfg.CatchUpInterval)

	tickers := make([]*time.Ticker, 0, 3)
	defer func() {
		for _, t := range tickers {
			t.Stop()
		}
	}()

	// Cleanup: remove old sent/failed notifications and expired cache rows
	if cfg.CleanupInterval > 0 {
		t := time.NewTicker(cfg.CleanupInterval)
		tickers = append(tickers, t)
		go runLoop(ctx, t.C, "cleanup", func() { cleanup(ctx, pool, logger) })
	}

	// Digest: generate batch notification records for digest delivery
	if cfg.DigestInterval > 0 {
		t := time.NewTicker(cfg.DigestInterval)
		tickers = append(tickers, t)
		go runLoop(ctx, t.C, "digest", func() { generateDigests(ctx, pool, logger) })
	}

	// Catch-up: sweep for NOTIFY events missed during downtime
	if cfg.CatchUpInterval > 0 {
		t := time.NewTicker(cfg.CatchUpInterval)
		tickers = append(tickers, t)
		go runLoop(ctx, t.C, "catchup", func() { catchUpSweep(ctx, pool, logger) })
	}

	<-ctx.Done()
	logger.Info("Maintenance tickers stopped")
}

func runLoop(ctx context.Context, ch <-chan time.Time, name string, fn func()) {
	for {
		select {
		case <-ch:
			fn()
		case <-ctx.Done():
			return
		}
	}
}

// --------------------------------------------------------------------------
// Task implementations
// --------------------------------------------------------------------------

// cleanup removes notifications older than 30 days that have been sent or
// failed, and expired percentile_archive rows marked as final.
func cleanup(ctx context.Context, pool *pgxpool.Pool, logger *slog.Logger) {
	// Purge old sent/failed notifications
	tag, err := pool.Exec(ctx, `
		DELETE FROM notifications
		WHERE status IN ('sent', 'failed')
		  AND updated_at < NOW() - INTERVAL '30 days'`)
	if err != nil {
		logger.Warn("Cleanup: failed to purge old notifications", "error", err)
	} else if tag.RowsAffected() > 0 {
		logger.Info("Cleanup: purged old notifications", "count", tag.RowsAffected())
	}

	// Purge old non-final percentile archive rows (keep final snapshots)
	tag, err = pool.Exec(ctx, `
		DELETE FROM percentile_archive
		WHERE is_final = false
		  AND archived_at < NOW() - INTERVAL '7 days'`)
	if err != nil {
		logger.Warn("Cleanup: failed to purge old archive rows", "error", err)
	} else if tag.RowsAffected() > 0 {
		logger.Info("Cleanup: purged old archive rows", "count", tag.RowsAffected())
	}
}

// generateDigests creates batch notification summaries for users who prefer
// digest-style delivery instead of real-time pushes.
// Currently a placeholder — will be implemented when user preference tables
// include a delivery_mode column.
func generateDigests(ctx context.Context, pool *pgxpool.Pool, logger *slog.Logger) {
	// TODO: When user preferences support digest mode:
	// 1. Query users with delivery_mode = 'digest'
	// 2. Aggregate pending changes since last digest
	// 3. Build summary notification and insert into notifications table
	_ = ctx
	_ = pool
	_ = logger
}

// catchUpSweep checks for entities with high percentiles that may not have
// had their NOTIFY events processed (e.g., during listener downtime).
// Compares current percentiles against the last archived snapshot and
// re-triggers notification processing for any gaps.
func catchUpSweep(ctx context.Context, pool *pgxpool.Pool, logger *slog.Logger) {
	// Find player_stats with percentiles >= 90 that were updated recently
	// but don't have a corresponding notification scheduled
	tag, err := pool.Exec(ctx, `
		INSERT INTO notifications (user_id, entity_type, entity_id, sport, stat_key, percentile, message, status, scheduled_for)
		SELECT
			uf.user_id,
			'player',
			ps.player_id,
			ps.sport,
			kv.key,
			(kv.value::text)::numeric,
			p.name || ' reached ' || round((kv.value::text)::numeric) || 'th percentile in ' || COALESCE(sd.display_name, kv.key),
			'scheduled',
			NOW()
		FROM player_stats ps
		CROSS JOIN LATERAL jsonb_each(ps.percentiles) AS kv(key, value)
		JOIN players p ON p.id = ps.player_id AND p.sport = ps.sport
		JOIN user_follows uf ON uf.entity_type = 'player' AND uf.entity_id = ps.player_id AND uf.sport = ps.sport
		LEFT JOIN stat_definitions sd ON sd.sport = ps.sport AND sd.key_name = kv.key AND sd.entity_type = 'player'
		WHERE kv.key NOT LIKE '\_%'
		  AND jsonb_typeof(kv.value) = 'number'
		  AND (kv.value::text)::numeric >= 90
		  AND ps.updated_at > NOW() - INTERVAL '1 hour'
		  AND NOT EXISTS (
			SELECT 1 FROM notifications n
			WHERE n.entity_type = 'player'
			  AND n.entity_id = ps.player_id
			  AND n.sport = ps.sport
			  AND n.stat_key = kv.key
			  AND n.created_at > NOW() - INTERVAL '2 hours'
		  )
		ON CONFLICT DO NOTHING`)
	if err != nil {
		logger.Warn("Catch-up sweep: failed", "error", err)
	} else if tag.RowsAffected() > 0 {
		logger.Info("Catch-up sweep: created missed notifications", "count", tag.RowsAffected())
	}
}
