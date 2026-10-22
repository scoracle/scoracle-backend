package main

import (
	"context"
	"log/slog"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

type coverageMonitor struct {
	nextSweep time.Time
	wasOpen   bool
}

func (m *coverageMonitor) poll(open bool, now time.Time) bool {
	due := open && (!m.wasOpen || !now.Before(m.nextSweep))
	m.wasOpen = open
	if due {
		m.nextSweep = now.Add(time.Hour)
	}
	return due
}

// Poll the shared clock even when no RSS is due. Reopening starts a catch-up sweep.
// The existing per-sweep jobrun guard prevents overlap with manual ingestion.
func runMonitor(ctx context.Context, pool *pgxpool.Pool, dbURL, sport string, limit, pause int, logger *slog.Logger) int {
	timer := time.NewTicker(15 * time.Second)
	defer timer.Stop()
	var monitor coverageMonitor
	for {
		qctx, cancel := context.WithTimeout(ctx, 10*time.Second)
		var open bool
		err := pool.QueryRow(qctx, "SELECT enabled AND public.coverage_collection_open() FROM public.coverage_schedule").Scan(&open)
		cancel()
		if err != nil {
			logger.Error("coverage monitor: clock unavailable; discovery paused", "error", err)
			monitor.poll(false, time.Now())
		} else if monitor.poll(open, time.Now()) {
			runIngestOnly(ctx, pool, dbURL, sport, limit, pause, logger)
		}

		select {
		case <-ctx.Done():
			return 0
		case <-timer.C:
		}
	}
}
