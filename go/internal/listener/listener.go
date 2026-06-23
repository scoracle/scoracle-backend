// Package listener provides the Postgres LISTEN/NOTIFY consumer for FCM push
// notifications. It holds a dedicated pgx connection (not from the pool) and
// listens on the percentile_changed channel: significant stat-line percentile
// crossings (90/95/99 or delta >= 10) become push notifications, and a large
// composite shift also triggers Sigil synthesis (one of its three inputs).
//
// The old news-rail consumers (vibe_trigger / transfer_trigger, which ran Gemma
// directly off a transient NOTIFY) were retired in FIRST-GPT-AUDIT Session 9 in
// favour of the durable pipeline_work queue drained by internal/derive.
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

	"github.com/albapepper/scoracle-data/internal/ml"
	"github.com/albapepper/scoracle-data/internal/notifications"
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
// is cancelled. Intended to be called with `go`.
// Start opens a dedicated connection and listens on the percentile_changed
// channel. It reconnects automatically on connection loss. Blocks until ctx
// is cancelled. synthGen is optional — nil disables composite-shift synthesis.
func Start(ctx context.Context, dbURL string, pool *pgxpool.Pool, sender *notifications.FCMSender, synthGen *ml.SigilGenerator, logger *slog.Logger) {
	backoff := reconnectBackoff

	for {
		err := listenLoop(ctx, dbURL, pool, sender, synthGen, logger)
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
func listenLoop(ctx context.Context, dbURL string, pool *pgxpool.Pool, sender *notifications.FCMSender, synthGen *ml.SigilGenerator, logger *slog.Logger) error {
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
		go handlePercentileChange(ctx, pool, sender, synthGen, event, logger)
	}
}

// handlePercentileChange resolves followers for the entity, dispatches FCM
// push notifications, and optionally triggers vibe synthesis on composite shifts.
func handlePercentileChange(ctx context.Context, pool *pgxpool.Pool, sender *notifications.FCMSender, synthGen *ml.SigilGenerator, event PercentileChangeEvent, logger *slog.Logger) {
	// Find followers for this entity
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

	// Composite-shift synthesis trigger: a significant rating change is one of
	// the three synthesis inputs. Spawn best-effort; 24h debounce prevents storms.
	if synthGen != nil && math.Abs(event.NewPercentile-event.OldPercentile) >= 10 &&
		!ml.RecentlySynthesized(ctx, pool, event.EntityType, event.EntityID, event.Sport, 24*time.Hour) {
		go func() {
			_, _ = synthGen.Generate(ctx, ml.SigilRequest{
				EntityType:  event.EntityType,
				EntityID:    event.EntityID,
				EntityName:  entityName,
				Sport:       event.Sport,
				TriggerType: "composite_shift",
				Trigger:     map[string]any{"stat_key": event.StatKey, "delta": event.NewPercentile - event.OldPercentile},
			})
		}()
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
