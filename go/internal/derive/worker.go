package derive

// worker.go — the real-time, in-API drain of the durable pipeline_work queue
// (FIRST-GPT-AUDIT Session 9). It replaces the old news_volume / transfer LISTEN
// workers that ran Gemma directly off a transient NOTIFY and held the affected
// entity set in process memory.
//
// The migration-103 trigger ENQUEUES durable work on the vetted=TRUE transition and
// fires pg_notify('pipeline_work_ready', ...) purely as a low-latency wake-up. This
// worker drains that durable queue:
//
//   - on startup (before the first wait), so work enqueued while we were down — or a
//     NOTIFY missed during a restart — is always picked up; the queue, not the
//     notification, is the source of truth;
//   - on every wake-up NOTIFY (low latency);
//   - on a safety-net timeout if no NOTIFY arrives (so correctness never depends on a
//     notification being delivered).
//
// A single goroutine runs the drain, so at most one DrainAll is ever in flight —
// bounded GPU concurrency (=1). Per-entity in-flight protection and cross-replica
// safety come for free from the queue itself: Claim uses FOR UPDATE SKIP LOCKED, and
// a claimed row is 'running' until completed, so two API replicas (or this worker and
// the nightly cron) get disjoint rows and never double-process an entity.

import (
	"context"
	"fmt"
	"log/slog"
	"time"

	"github.com/jackc/pgx/v5"

	"github.com/albapepper/scoracle-data/internal/work"
)

const (
	workReadyChannel = "pipeline_work_ready"
	reconnectBackoff = 5 * time.Second
	maxReconnect     = 30 * time.Second
)

// StartWorker opens a dedicated connection LISTEN'ing on pipeline_work_ready and
// drains the durable derive queue (drainer) on wake-up, on startup, and on the
// safety-net interval. Reconnects automatically on connection loss. Blocks until ctx
// is cancelled. Intended to be called with `go`. The caller should only start this
// when the drainer's generators are available (Ollama reachable) — otherwise every
// claimed item would fail and burn its retries.
func StartWorker(ctx context.Context, dbURL string, drainer *Drainer, interval time.Duration, logger *slog.Logger) {
	if interval <= 0 {
		interval = 30 * time.Second
	}
	backoff := reconnectBackoff
	for {
		err := workerLoop(ctx, dbURL, drainer, interval, logger)
		if ctx.Err() != nil {
			logger.Info("Derive drain worker stopped (context cancelled)")
			return
		}
		logger.Error("Derive drain worker disconnected, reconnecting...", "error", err, "backoff", backoff)
		select {
		case <-time.After(backoff):
			backoff = min(backoff*2, maxReconnect)
		case <-ctx.Done():
			return
		}
	}
}

func workerLoop(ctx context.Context, dbURL string, drainer *Drainer, interval time.Duration, logger *slog.Logger) error {
	conn, err := pgx.Connect(ctx, dbURL)
	if err != nil {
		return fmt.Errorf("connect: %w", err)
	}
	defer conn.Close(context.Background())

	if _, err := conn.Exec(ctx, "LISTEN "+workReadyChannel); err != nil {
		return fmt.Errorf("LISTEN %s: %w", workReadyChannel, err)
	}
	logger.Info("Derive drain worker connected", "channel", workReadyChannel, "safety_net", interval)

	for {
		// Recover any crashed-drainer leases, then drain everything pending. Runs on
		// startup before the first wait, so a NOTIFY missed during a restart is still
		// picked up.
		if n, rerr := work.RequeueStale(ctx, drainer.Pool, StaleLease); rerr != nil {
			logger.Warn("derive worker: requeue stale failed", "error", rerr)
		} else if n > 0 {
			logger.Info("derive worker: recovered stale work", "rows", n)
		}
		drainer.DrainAll(ctx)

		// Block until a wake-up NOTIFY arrives OR the safety-net interval elapses.
		// pgx v5 leaves the LISTEN connection usable after a context-deadline
		// interrupt (it resets the socket deadline on unwatch and the read parks at a
		// message boundary), so this per-call timeout is safe to loop on.
		wctx, cancel := context.WithTimeout(ctx, interval)
		_, nerr := conn.WaitForNotification(wctx)
		timedOut := wctx.Err() == context.DeadlineExceeded // capture before cancel()
		cancel()
		if nerr != nil {
			if ctx.Err() != nil {
				return nerr // parent cancelled → unwind to StartWorker, which stops
			}
			if timedOut {
				continue // our own safety-net tick → drain again
			}
			return fmt.Errorf("wait for notification: %w", nerr) // real conn error → reconnect
		}
		// A NOTIFY arrived → loop drains it (any others that landed coalesce into the
		// same next pass).
	}
}
