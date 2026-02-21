package notifications

import (
	"context"
	"fmt"
	"log/slog"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// StartWorker runs a background loop that sends due notifications.
// Blocks until ctx is cancelled. Intended to be called with `go`.
func StartWorker(ctx context.Context, pool *pgxpool.Pool, sender *FCMSender, logger *slog.Logger) {
	logger.Info("Notification dispatch worker started", "interval", dispatchInterval)
	ticker := time.NewTicker(dispatchInterval)
	defer ticker.Stop()

	for {
		select {
		case <-ticker.C:
			sent, failed, err := dispatchBatch(ctx, pool, sender, logger)
			if err != nil {
				logger.Error("dispatch error", "error", err)
			} else if sent+failed > 0 {
				logger.Info("dispatch batch", "sent", sent, "failed", failed)
			}
		case <-ctx.Done():
			logger.Info("Notification dispatch worker stopped")
			return
		}
	}
}

func dispatchBatch(ctx context.Context, pool *pgxpool.Pool, sender *FCMSender, logger *slog.Logger) (sent, failed int, err error) {
	claimed, err := ClaimDue(ctx, pool)
	if err != nil {
		return 0, 0, err
	}

	for _, row := range claimed {
		tokens, tokErr := getDeviceTokens(ctx, pool, row.UserID)
		if tokErr != nil {
			logger.Warn("no device tokens", "user_id", row.UserID, "error", tokErr)
			_ = MarkFailed(ctx, pool, row.ID, "no device tokens")
			failed++
			continue
		}

		data := map[string]string{
			"entity_type": row.EntityType,
			"entity_id":   fmt.Sprintf("%d", row.EntityID),
			"sport":       row.Sport,
		}

		sendErr := sender.SendMulti(ctx, tokens, "Scoracle", row.Message, data)
		if sendErr != nil {
			logger.Warn("send failed", "notification_id", row.ID, "error", sendErr)
			_ = MarkFailed(ctx, pool, row.ID, sendErr.Error())
			failed++
		} else {
			_ = MarkSent(ctx, pool, row.ID)
			sent++
		}
	}
	return sent, failed, nil
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
	if len(tokens) == 0 {
		return nil, fmt.Errorf("no active tokens for user %s", userID)
	}
	return tokens, rows.Err()
}
