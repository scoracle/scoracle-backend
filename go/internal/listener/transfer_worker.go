// Package listener — transfer-rumor news-spike worker.
//
// When a TEAM crosses the 5-articles-in-60-min news threshold, the SQL trigger
// notify_vibe_trigger() (migration 034) fires pg_notify('transfer_trigger', ...).
// This consumer re-vets that team's transfer/trade candidates with Gemma so a
// breaking transfer cycle surfaces within minutes instead of waiting for the next
// corpus cron.
//
// GPU contention is the central risk: one Gemma/Archbox GPU is shared with the
// vibe worker, and a deadline-day flood fans out to many pair-calls per team. Two
// governors keep it bounded:
//   - a GLOBAL concurrency cap (buffered-channel semaphore) so at most N team
//     analyses ever call Ollama at once — waiters block, they don't pile onto the
//     GPU;
//   - a PER-TEAM in-flight guard so a same-team burst can't launch the team twice
//     concurrently (the DB debounce only updates AFTER a run finishes, so it can't
//     dedupe mid-run).
// Plus the DB-backed 60-min debounce (survives restarts) and the cron carrying the
// comprehensive batch. Heat-only rows already render, so saturation degrades to
// numbers-without-fresh-summaries, never a broken card.
package listener

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"strconv"
	"sync"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/ml"
)

const transferChannel = "transfer_trigger"

// TransferConfig tunes the worker's governors (from config.go).
type TransferConfig struct {
	MaxConcurrent int           // global cap on concurrent team analyses (Ollama)
	Debounce      time.Duration // min gap between runs for one team
	MinArticles   int           // candidate pre-filter passed to GenerateForTeam
}

// TransferEvent is the JSON payload from pg_notify('transfer_trigger', ...).
type TransferEvent struct {
	EntityType   string `json:"entity_type"`
	EntityID     int    `json:"entity_id"`
	Sport        string `json:"sport"`
	ArticleCount int    `json:"article_count"`
	Timestamp    int64  `json:"ts"`
}

// transferDispatcher holds the two governors shared across dispatch goroutines.
type transferDispatcher struct {
	sem      chan struct{} // global concurrency semaphore
	inflight sync.Map      // teamKey -> struct{}; per-team mutual exclusion
}

// StartTransfer opens a dedicated connection LISTEN'ing on transfer_trigger and
// re-vets a team's rumors when its news volume spikes. Reconnects automatically on
// connection loss. Blocks until ctx is cancelled. Intended to be called with `go`.
//
// gen may be nil — events are then logged but no Gemma calls fire (the degraded
// mode when Ollama is unreachable at startup).
func StartTransfer(ctx context.Context, dbURL string, pool *pgxpool.Pool, gen *ml.TransferGenerator, cfg TransferConfig, logger *slog.Logger) {
	if cfg.MaxConcurrent < 1 {
		cfg.MaxConcurrent = 1
	}
	d := &transferDispatcher{sem: make(chan struct{}, cfg.MaxConcurrent)}
	backoff := reconnectBackoff

	for {
		err := transferLoop(ctx, dbURL, pool, gen, cfg, d, logger)
		if ctx.Err() != nil {
			logger.Info("Transfer listener stopped (context cancelled)")
			return
		}

		logger.Error("Transfer listener disconnected, reconnecting...",
			"error", err, "backoff", backoff)

		select {
		case <-time.After(backoff):
			backoff = min(backoff*2, maxReconnect)
		case <-ctx.Done():
			return
		}
	}
}

func transferLoop(ctx context.Context, dbURL string, pool *pgxpool.Pool, gen *ml.TransferGenerator, cfg TransferConfig, d *transferDispatcher, logger *slog.Logger) error {
	conn, err := pgx.Connect(ctx, dbURL)
	if err != nil {
		return fmt.Errorf("connect: %w", err)
	}
	defer conn.Close(context.Background())

	if _, err := conn.Exec(ctx, "LISTEN "+transferChannel); err != nil {
		return fmt.Errorf("LISTEN %s: %w", transferChannel, err)
	}
	logger.Info("Transfer listener connected", "channel", transferChannel, "max_concurrent", cfg.MaxConcurrent)

	for {
		notification, err := conn.WaitForNotification(ctx)
		if err != nil {
			return fmt.Errorf("wait for notification: %w", err)
		}

		var event TransferEvent
		if err := json.Unmarshal([]byte(notification.Payload), &event); err != nil {
			logger.Warn("Failed to parse transfer event", "payload", notification.Payload, "error", err)
			continue
		}

		// v1 acts on teams only — the SQL guard already enforces this; defend anyway.
		if event.EntityType != "team" {
			continue
		}

		logger.Info("Transfer news spike",
			"team_id", event.EntityID, "sport", event.Sport, "articles_60m", event.ArticleCount)

		if gen == nil {
			continue
		}
		go d.dispatch(ctx, pool, gen, cfg, event, logger)
	}
}

func (d *transferDispatcher) dispatch(ctx context.Context, pool *pgxpool.Pool, gen *ml.TransferGenerator, cfg TransferConfig, event TransferEvent, logger *slog.Logger) {
	// Cheap exits before touching the GPU.
	if recentlyTransferred(ctx, pool, event.EntityID, event.Sport, cfg.Debounce) {
		return
	}
	key := event.Sport + ":" + strconv.Itoa(event.EntityID)
	if _, busy := d.inflight.LoadOrStore(key, struct{}{}); busy {
		logger.Info("transfer: team already in flight, skipping", "team_id", event.EntityID, "sport", event.Sport)
		return
	}
	defer d.inflight.Delete(key)

	// Block for a global slot — bounds concurrent Ollama load, never the GPU.
	select {
	case d.sem <- struct{}{}:
	case <-ctx.Done():
		return
	}
	defer func() { <-d.sem }()

	// Re-check debounce: a cron run (or another path) may have refreshed this team
	// while we waited for a slot.
	if recentlyTransferred(ctx, pool, event.EntityID, event.Sport, cfg.Debounce) {
		return
	}

	name, err := lookupEntityName(ctx, pool, "team", event.EntityID, event.Sport)
	if err != nil || name == "" {
		logger.Warn("transfer: team lookup failed", "team_id", event.EntityID, "sport", event.Sport, "error", err)
		return
	}

	res, err := gen.GenerateForTeam(ctx, ml.TransferRequest{
		TeamID:      event.EntityID,
		TeamName:    name,
		Sport:       event.Sport,
		TriggerType: "news_spike",
		MinArticles: cfg.MinArticles,
	})
	if err != nil {
		logger.Warn("transfer: generate failed", "team", name, "sport", event.Sport, "error", err)
		return
	}
	logger.Info("transfer: rumors refreshed",
		"team", name, "sport", event.Sport,
		"candidates", res.Candidates, "rumors", res.Rumors, "cleared", res.Cleared,
		"duration", res.Duration)
}

// recentlyTransferred reports whether this team got a transfer run inside the
// debounce window. DB-backed so it survives API restarts and works across replicas
// sharing one Postgres. Fails open (better to over-generate than drop a spike).
func recentlyTransferred(ctx context.Context, pool *pgxpool.Pool, teamID int, sport string, debounce time.Duration) bool {
	var exists bool
	err := pool.QueryRow(ctx, `
		SELECT EXISTS (
			SELECT 1 FROM transfer_rumors
			WHERE team_id = $1 AND sport = $2
			  AND generated_at > NOW() - $3::interval
		)
	`, teamID, sport, debounce.String()).Scan(&exists)
	if err != nil {
		return false
	}
	return exists
}
