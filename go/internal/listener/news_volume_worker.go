// Package listener — news-volume vibe trigger.
//
// When an entity (player or team) accumulates 5 distinct news articles
// inside a 60-minute rolling window, the SQL trigger
// notify_vibe_trigger() fires pg_notify('vibe_trigger', ...) once per
// crossing. This consumer receives the event and runs Gemma to refresh
// the entity's vibe score so a breaking-news cycle gets a fresh sentiment
// score within minutes instead of waiting for the next twice-daily cron.
//
// Per-entity debounce (30 min) guards against bursty spikes that re-trip
// the threshold mid-storm — one Gemma call per entity per 30 min is plenty.
package listener

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/ml"
)

const (
	newsVolumeChannel  = "vibe_trigger"
	newsVolumeDebounce = 30 * time.Minute
	// narrativesDebounce gates the narratives refresh on a spike. Narratives are
	// the heavier (~100s) stage; one refresh per entity per window is plenty.
	narrativesDebounce = 30 * time.Minute
)

// NewsVolumeEvent is the JSON payload from pg_notify('vibe_trigger', ...).
type NewsVolumeEvent struct {
	EntityType   string `json:"entity_type"`
	EntityID     int    `json:"entity_id"`
	Sport        string `json:"sport"`
	ArticleCount int    `json:"article_count"`
	Timestamp    int64  `json:"ts"`
}

// StartNewsVolume opens a dedicated connection LISTEN'ing on vibe_trigger
// and runs the staged Gemma reveal against entities whose news volume crossed
// the SQL trigger's threshold: refresh the NARRATIVES first, then the VIBE
// (which reads those narratives + transfer heat). Reconnects automatically on
// connection loss. Blocks until ctx is cancelled. Intended to be called with `go`.
//
// narrator/gen may be nil — in that case events are logged but no Gemma calls
// fire. That's the degraded mode when Ollama is unreachable at startup.
func StartNewsVolume(ctx context.Context, dbURL string, pool *pgxpool.Pool, narrator *ml.NewsNarrator, gen *ml.Generator, logger *slog.Logger) {
	backoff := reconnectBackoff

	for {
		err := newsVolumeLoop(ctx, dbURL, pool, narrator, gen, logger)
		if ctx.Err() != nil {
			logger.Info("News-volume listener stopped (context cancelled)")
			return
		}

		logger.Error("News-volume listener disconnected, reconnecting...",
			"error", err, "backoff", backoff)

		select {
		case <-time.After(backoff):
			backoff = min(backoff*2, maxReconnect)
		case <-ctx.Done():
			return
		}
	}
}

func newsVolumeLoop(ctx context.Context, dbURL string, pool *pgxpool.Pool, narrator *ml.NewsNarrator, gen *ml.Generator, logger *slog.Logger) error {
	conn, err := pgx.Connect(ctx, dbURL)
	if err != nil {
		return fmt.Errorf("connect: %w", err)
	}
	defer conn.Close(context.Background())

	if _, err := conn.Exec(ctx, "LISTEN "+newsVolumeChannel); err != nil {
		return fmt.Errorf("LISTEN %s: %w", newsVolumeChannel, err)
	}
	logger.Info("News-volume listener connected", "channel", newsVolumeChannel)

	for {
		notification, err := conn.WaitForNotification(ctx)
		if err != nil {
			return fmt.Errorf("wait for notification: %w", err)
		}

		var event NewsVolumeEvent
		if err := json.Unmarshal([]byte(notification.Payload), &event); err != nil {
			logger.Warn("Failed to parse news-volume event",
				"payload", notification.Payload, "error", err)
			continue
		}

		logger.Info("News-volume spike",
			"entity_type", event.EntityType,
			"entity_id", event.EntityID,
			"sport", event.Sport,
			"articles_60m", event.ArticleCount)

		if gen == nil {
			continue
		}
		go dispatchNewsVolume(ctx, pool, narrator, gen, event, logger)
	}
}

// dispatchNewsVolume runs the staged reveal for one spike: refresh the entity's
// NARRATIVES first (the derived layer the vibe reads — and the spike's primary
// product), then determine the VIBE from those narratives + transfer heat. Each
// stage has its own debounce, so a vibe still inside its window doesn't block a
// fresh narrative, and vice-versa.
func dispatchNewsVolume(ctx context.Context, pool *pgxpool.Pool, narrator *ml.NewsNarrator, gen *ml.Generator, event NewsVolumeEvent, logger *slog.Logger) {
	name, err := lookupEntityName(ctx, pool, event.EntityType, event.EntityID, event.Sport)
	if err != nil || name == "" {
		logger.Warn("news-volume: entity lookup failed",
			"entity_type", event.EntityType, "entity_id", event.EntityID,
			"sport", event.Sport, "error", err)
		return
	}
	trigger := map[string]any{"article_count_60m": event.ArticleCount}

	// Stage 2 — narratives. Skip only if we refreshed them very recently.
	if narrator != nil && !recentlyNarrated(ctx, pool, event) {
		nres, err := narrator.Generate(ctx, ml.NarrativesRequest{
			EntityType:  event.EntityType,
			EntityID:    event.EntityID,
			EntityName:  name,
			Sport:       event.Sport,
			TriggerType: "news_spike",
			Trigger:     trigger,
		})
		switch {
		case err != nil:
			logger.Warn("news-volume: narratives failed",
				"entity_type", event.EntityType, "entity_id", event.EntityID, "sport", event.Sport, "error", err)
		case nres.SkippedNoCorpus:
			logger.Info("news-volume: narratives skipped (no corpus)",
				"entity_type", event.EntityType, "entity_id", event.EntityID, "sport", event.Sport)
		default:
			logger.Info("news-volume: narratives generated",
				"entity_type", event.EntityType, "entity_id", event.EntityID, "sport", event.Sport,
				"narratives", len(nres.Narratives), "duration", nres.Duration)
		}
	}

	// Stage 3 — vibe from the fresh narratives + heat.
	if recentlyVibed(ctx, pool, event) {
		return
	}
	result, err := gen.Generate(ctx, ml.VibeRequest{
		EntityType:  event.EntityType,
		EntityID:    event.EntityID,
		EntityName:  name,
		Sport:       event.Sport,
		TriggerType: "news_spike",
		Trigger:     trigger,
	})
	if err != nil {
		logger.Warn("news-volume: generate failed",
			"entity_type", event.EntityType, "entity_id", event.EntityID,
			"sport", event.Sport, "error", err)
		return
	}
	if result.SkippedNoCorpus {
		logger.Info("news-volume: skipped (no corpus inside lookback)",
			"entity_type", event.EntityType, "entity_id", event.EntityID,
			"sport", event.Sport)
		return
	}

	logger.Info("news-volume: vibe generated",
		"entity_type", event.EntityType, "entity_id", event.EntityID,
		"sport", event.Sport, "sentiment", result.Sentiment,
		"news", len(result.InputNewsIDs), "tweets", len(result.InputTweetIDs),
		"duration", result.Duration)
}

// recentlyNarrated returns true if narratives were generated for this entity
// inside the narratives debounce window. DB-backed so it survives restarts.
func recentlyNarrated(ctx context.Context, pool *pgxpool.Pool, event NewsVolumeEvent) bool {
	var exists bool
	err := pool.QueryRow(ctx, `
		SELECT EXISTS (
			SELECT 1 FROM news_summaries
			WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
			  AND generated_at > NOW() - $4::interval
		)
	`, event.EntityType, event.EntityID, event.Sport, narrativesDebounce.String()).Scan(&exists)
	if err != nil {
		// Fail open: better to over-generate than silently drop a spike.
		return false
	}
	return exists
}

// recentlyVibed returns true if a vibe was generated for this entity inside
// the debounce window. DB-backed so it survives API restarts and works
// across replicas sharing one Postgres.
func recentlyVibed(ctx context.Context, pool *pgxpool.Pool, event NewsVolumeEvent) bool {
	var exists bool
	err := pool.QueryRow(ctx, `
		SELECT EXISTS (
			SELECT 1 FROM vibe_scores
			WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
			  AND generated_at > NOW() - $4::interval
		)
	`, event.EntityType, event.EntityID, event.Sport, newsVolumeDebounce.String()).Scan(&exists)
	if err != nil {
		// Fail open: better to over-generate than silently drop a spike.
		return false
	}
	return exists
}

// lookupEntityName resolves the display name for the Gemma prompt.
func lookupEntityName(ctx context.Context, pool *pgxpool.Pool, entityType string, id int, sport string) (string, error) {
	var query string
	if entityType == "player" {
		query = `SELECT name FROM players WHERE id = $1 AND sport = $2`
	} else {
		query = `SELECT name FROM teams WHERE id = $1 AND sport = $2`
	}
	var name string
	err := pool.QueryRow(ctx, query, id, sport).Scan(&name)
	return name, err
}
