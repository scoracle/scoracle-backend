// Package listener provides the Postgres LISTEN/NOTIFY consumer for FCM push
// notifications. It holds a dedicated pgx connection (not from the pool) and
// listens on the percentile_changed channel: significant stat-line percentile
// crossings (90/95/99 or delta >= 10) become push notifications, and a large
// composite shift ENQUEUES durable Sigil convergence work (one of its three inputs).
//
// The old news-rail consumers (vibe_trigger / transfer_trigger, which ran Gemma
// directly off a transient NOTIFY) were retired in FIRST-GPT-AUDIT Session 9 in
// favour of the durable pipeline_work queue. Session 12
// (F-017) brings the STATS-rail composite_shift trigger into that same model: instead
// of running Gemma inline off the transient NOTIFY, it enqueues a sigil pipeline_work
// item the Rust cognition daemon drains — and does so independently of followers/FCM, so an
// entity with zero followers still reconverges its crown (simplification A).
package listener

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"math"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/notifications"
	"github.com/albapepper/scoracle-data/internal/work"
)

const (
	channel          = "percentile_changed"
	reconnectBackoff = 5 * time.Second
	maxReconnect     = 30 * time.Second
)

// PercentileChangeEvent is the JSON payload from pg_notify('percentile_changed', ...).
type PercentileChangeEvent struct {
	EntityType    string  `json:"entity_type"`
	EntityID      int     `json:"entity_id"`
	Sport         string  `json:"sport"`
	Season        int     `json:"season"`
	StatKey       string  `json:"stat_key"`
	OldPercentile float64 `json:"old_percentile"`
	NewPercentile float64 `json:"new_percentile"`
	Timestamp     int64   `json:"ts"`
}

// Start opens a dedicated connection and listens on the percentile_changed
// channel. It reconnects automatically on connection loss. Blocks until ctx
// is cancelled. Intended to be called with `go`. Composite-shift Sigil convergence
// is enqueued as durable pipeline_work (no inline generator dependency).
func Start(ctx context.Context, dbURL string, pool *pgxpool.Pool, sender *notifications.FCMSender, logger *slog.Logger) {
	backoff := reconnectBackoff

	for {
		err := listenLoop(ctx, dbURL, pool, sender, logger)
		if ctx.Err() != nil {
			logger.Info("Percentile listener stopped (context cancelled)")
			return
		}

		logger.Error("Percentile listener disconnected, reconnecting...",
			"error", err, "backoff", backoff)

		select {
		case <-time.After(backoff):
			backoff = min(backoff*2, maxReconnect)
		case <-ctx.Done():
			return
		}
	}
}

// listenLoop runs a single listen session. Returns when the connection drops
// or the context is cancelled.
func listenLoop(ctx context.Context, dbURL string, pool *pgxpool.Pool, sender *notifications.FCMSender, logger *slog.Logger) error {
	conn, err := pgx.Connect(ctx, dbURL)
	if err != nil {
		return fmt.Errorf("connect: %w", err)
	}
	defer conn.Close(context.Background())

	_, err = conn.Exec(ctx, "LISTEN "+channel)
	if err != nil {
		return fmt.Errorf("LISTEN %s: %w", channel, err)
	}
	logger.Info("Percentile listener connected", "channel", channel)

	for {
		notification, err := conn.WaitForNotification(ctx)
		if err != nil {
			return fmt.Errorf("wait for notification: %w", err)
		}

		var event PercentileChangeEvent
		if err := json.Unmarshal([]byte(notification.Payload), &event); err != nil {
			logger.Warn("Failed to parse percentile change event",
				"payload", notification.Payload, "error", err)
			continue
		}

		logger.Info("Percentile change event received",
			"entity_type", event.EntityType,
			"entity_id", event.EntityID,
			"sport", event.Sport,
			"stat", event.StatKey,
			"old_pctile", event.OldPercentile,
			"new_pctile", event.NewPercentile)

		// Process asynchronously to avoid blocking the listener.
		go handlePercentileChange(ctx, pool, sender, event, logger)
	}
}

// handlePercentileChange enqueues stats-rail Sigil convergence on a large composite
// shift (a durable concern, independent of followers), then dispatches FCM push
// notifications to any followers (a separate delivery concern).
func handlePercentileChange(ctx context.Context, pool *pgxpool.Pool, sender *notifications.FCMSender, event PercentileChangeEvent, logger *slog.Logger) {
	// Stats-rail Sigil convergence (FIRST-GPT-AUDIT Session 12, F-017 + simplification A):
	// a significant composite move is one of the three Sigil inputs. Enqueue a DURABLE
	// sigil pipeline_work item so the derive worker drains it like every other stage —
	// no inline Gemma off the transient NOTIFY, no follower gate. The drain resolves the
	// sport's current_season and the SigilGenerator's input-hash gate skips a Gemma call
	// when nothing material changed, so a same-percentile re-fire is a cheap no-op.
	if math.Abs(event.NewPercentile-event.OldPercentile) >= 10 {
		iv := fmt.Sprintf("composite:%d:%.0f", event.Season, event.NewPercentile)
		if err := work.Enqueue(ctx, pool, work.Item{
			Stage:        work.StageSigil,
			EntityType:   event.EntityType,
			EntityID:     event.EntityID,
			Sport:        event.Sport,
			InputVersion: iv,
		}); err != nil {
			logger.Warn("listener: enqueue sigil (composite_shift) failed",
				"entity_type", event.EntityType, "entity_id", event.EntityID, "sport", event.Sport, "error", err)
		}
	}

	// Find followers for this entity (FCM push delivery — a separate concern)
	followers, err := notifications.GetFollowers(ctx, pool, event.EntityType, event.EntityID, event.Sport)
	if err != nil {
		logger.Warn("Failed to get followers for percentile change",
			"entity_type", event.EntityType, "entity_id", event.EntityID, "error", err)
		return
	}
	if len(followers) == 0 {
		return
	}

	// Resolve entity name and stat display name for the message
	entityName, _ := notifications.GetEntityName(ctx, pool, event.EntityType, event.EntityID, event.Sport)
	statDisplay, _ := notifications.GetStatDisplayName(ctx, pool, event.Sport, event.StatKey, event.EntityType)

	pctile := int(event.NewPercentile)
	suffix := ordinalSuffix(pctile)
	message := fmt.Sprintf("%s is now %d%s percentile in %s", entityName, pctile, suffix, statDisplay)

	data := map[string]string{
		"entity_type":    event.EntityType,
		"entity_id":      fmt.Sprintf("%d", event.EntityID),
		"sport":          event.Sport,
		"stat_key":       event.StatKey,
		"old_percentile": fmt.Sprintf("%.1f", event.OldPercentile),
		"new_percentile": fmt.Sprintf("%.1f", event.NewPercentile),
	}

	if sender == nil {
		logger.Info("Percentile notification (FCM disabled)",
			"message", message, "followers", len(followers))
		return
	}

	// Dispatch to each follower's devices
	sent, failed := 0, 0
	for _, f := range followers {
		tokens, err := notifications.GetDeviceTokens(ctx, pool, f.UserID)
		if err != nil || len(tokens) == 0 {
			continue
		}

		if err := sender.SendMulti(ctx, tokens, "Scoracle", message, data); err != nil {
			logger.Warn("FCM send failed", "user_id", f.UserID, "error", err)
			failed++
		} else {
			sent++
		}
	}

	if sent+failed > 0 {
		logger.Info("Percentile notifications dispatched",
			"message", message, "sent", sent, "failed", failed)
	}
}

func ordinalSuffix(n int) string {
	if n%100 >= 11 && n%100 <= 13 {
		return "th"
	}
	switch n % 10 {
	case 1:
		return "st"
	case 2:
		return "nd"
	case 3:
		return "rd"
	default:
		return "th"
	}
}
