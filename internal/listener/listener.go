// Package listener provides a Postgres LISTEN/NOTIFY consumer for real-time
// milestone event processing. It holds a dedicated pgx connection (not from
// the pool) listening on the `milestone_reached` channel.
//
// When a percentile milestone is reached (>= 90th), the Postgres trigger
// fires pg_notify and this consumer receives the event, resolves followers,
// and dispatches FCM push notifications.
package listener

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/notifications"
)

const (
	channel          = "milestone_reached"
	reconnectBackoff = 5 * time.Second
	maxReconnect     = 30 * time.Second
)

// MilestoneEvent is the JSON payload from pg_notify('milestone_reached', ...).
type MilestoneEvent struct {
	EntityType string  `json:"entity_type"`
	EntityID   int     `json:"entity_id"`
	Sport      string  `json:"sport"`
	Season     int     `json:"season"`
	StatKey    string  `json:"stat_key"`
	Percentile float64 `json:"percentile"`
	Timestamp  int64   `json:"ts"`
}

// Start opens a dedicated connection and listens on the milestone_reached
// channel. It reconnects automatically on connection loss. Blocks until ctx
// is cancelled. Intended to be called with `go`.
func Start(ctx context.Context, dbURL string, pool *pgxpool.Pool, sender *notifications.FCMSender, logger *slog.Logger) {
	backoff := reconnectBackoff

	for {
		err := listenLoop(ctx, dbURL, pool, sender, logger)
		if ctx.Err() != nil {
			logger.Info("Milestone listener stopped (context cancelled)")
			return
		}

		logger.Error("Milestone listener disconnected, reconnecting...",
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
	logger.Info("Milestone listener connected", "channel", channel)

	for {
		notification, err := conn.WaitForNotification(ctx)
		if err != nil {
			return fmt.Errorf("wait for notification: %w", err)
		}

		var event MilestoneEvent
		if err := json.Unmarshal([]byte(notification.Payload), &event); err != nil {
			logger.Warn("Failed to parse milestone event",
				"payload", notification.Payload, "error", err)
			continue
		}

		logger.Info("Milestone event received",
			"entity_type", event.EntityType,
			"entity_id", event.EntityID,
			"sport", event.Sport,
			"stat", event.StatKey,
			"percentile", event.Percentile)

		// Process asynchronously to avoid blocking the listener
		go handleMilestone(ctx, pool, sender, event, logger)
	}
}

// handleMilestone resolves followers for the entity and dispatches FCM
// push notifications for the milestone crossing.
func handleMilestone(ctx context.Context, pool *pgxpool.Pool, sender *notifications.FCMSender, event MilestoneEvent, logger *slog.Logger) {
	// Find followers for this entity
	followers, err := notifications.GetFollowers(ctx, pool, event.EntityType, event.EntityID, event.Sport)
	if err != nil {
		logger.Warn("Failed to get followers for milestone",
			"entity_type", event.EntityType, "entity_id", event.EntityID, "error", err)
		return
	}
	if len(followers) == 0 {
		return
	}

	// Resolve entity name and stat display name for the message
	entityName, _ := notifications.GetEntityName(ctx, pool, event.EntityType, event.EntityID, event.Sport)
	statDisplay, _ := notifications.GetStatDisplayName(ctx, pool, event.Sport, event.StatKey, event.EntityType)

	pctile := int(event.Percentile)
	suffix := ordinalSuffix(pctile)
	message := fmt.Sprintf("%s is now %d%s percentile in %s", entityName, pctile, suffix, statDisplay)

	data := map[string]string{
		"entity_type": event.EntityType,
		"entity_id":   fmt.Sprintf("%d", event.EntityID),
		"sport":       event.Sport,
		"stat_key":    event.StatKey,
		"percentile":  fmt.Sprintf("%.1f", event.Percentile),
	}

	if sender == nil {
		logger.Info("Milestone notification (FCM disabled)",
			"message", message, "followers", len(followers))
		return
	}

	// Dispatch to each follower's devices
	sent, failed := 0, 0
	for _, f := range followers {
		tokens, err := getDeviceTokens(ctx, pool, f.UserID)
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
		logger.Info("Milestone notifications dispatched",
			"message", message, "sent", sent, "failed", failed)
	}
}

func getDeviceTokens(ctx context.Context, pool *pgxpool.Pool, userID string) ([]string, error) {
	rows, err := pool.Query(ctx, "get_user_device_tokens", userID)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var tokens []string
	for rows.Next() {
		var t string
		if err := rows.Scan(&t); err != nil {
			return nil, err
		}
		tokens = append(tokens, t)
	}
	return tokens, rows.Err()
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
