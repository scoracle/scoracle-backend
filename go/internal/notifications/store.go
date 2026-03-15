package notifications

import (
	"context"
	"fmt"

	"github.com/jackc/pgx/v5/pgxpool"
)

// GetFollowers returns users following an entity.
func GetFollowers(ctx context.Context, pool *pgxpool.Pool, entityType string, entityID int, sport string) ([]Follower, error) {
	rows, err := pool.Query(ctx, "get_entity_followers", entityType, entityID, sport)
	if err != nil {
		return nil, fmt.Errorf("get followers: %w", err)
	}
	defer rows.Close()

	var followers []Follower
	for rows.Next() {
		var f Follower
		if err := rows.Scan(&f.UserID, &f.Timezone); err != nil {
			return nil, fmt.Errorf("scan follower: %w", err)
		}
		followers = append(followers, f)
	}
	return followers, rows.Err()
}

// GetEntityName returns the display name for an entity.
func GetEntityName(ctx context.Context, pool *pgxpool.Pool, entityType string, entityID int, sport string) (string, error) {
	var name string
	stmt := "team_name_lookup"
	if entityType == "player" {
		stmt = "notification_player_name"
	}
	if err := pool.QueryRow(ctx, stmt, entityID, sport).Scan(&name); err != nil {
		return "", fmt.Errorf("get entity name: %w", err)
	}
	return name, nil
}

// GetStatDisplayName returns the human-readable name for a stat key.
func GetStatDisplayName(ctx context.Context, pool *pgxpool.Pool, sport, statKey, entityType string) (string, error) {
	var displayName string
	err := pool.QueryRow(ctx, "stat_display_name", sport, statKey, entityType).Scan(&displayName)
	if err != nil {
		return statKey, nil // fallback to raw key
	}
	return displayName, nil
}

// GetDeviceTokens returns active FCM device tokens for a user.
func GetDeviceTokens(ctx context.Context, pool *pgxpool.Pool, userID string) ([]string, error) {
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

// claimedRow is an internal type for claimed notification rows.
type claimedRow struct {
	ID         int
	UserID     string
	Message    string
	EntityType string
	EntityID   int
	Sport      string
}

// ClaimDue atomically claims a batch of due notifications for sending.
// Uses FOR UPDATE SKIP LOCKED for safe concurrent dispatch.
func ClaimDue(ctx context.Context, pool *pgxpool.Pool) ([]claimedRow, error) {
	rows, err := pool.Query(ctx, `
		UPDATE notifications
		SET status = 'sending', updated_at = NOW()
		WHERE id IN (
			SELECT id FROM notifications
			WHERE status = 'scheduled' AND scheduled_for <= NOW()
			ORDER BY scheduled_for
			LIMIT $1
			FOR UPDATE SKIP LOCKED
		)
		RETURNING id, user_id, message, entity_type, entity_id, sport`,
		dispatchBatchSize,
	)
	if err != nil {
		return nil, fmt.Errorf("claim due notifications: %w", err)
	}
	defer rows.Close()

	var claimed []claimedRow
	for rows.Next() {
		var r claimedRow
		if err := rows.Scan(&r.ID, &r.UserID, &r.Message, &r.EntityType, &r.EntityID, &r.Sport); err != nil {
			return nil, fmt.Errorf("scan claimed: %w", err)
		}
		claimed = append(claimed, r)
	}
	return claimed, rows.Err()
}

// MarkSent marks a notification as successfully sent.
func MarkSent(ctx context.Context, pool *pgxpool.Pool, id int) error {
	_, err := pool.Exec(ctx, `
		UPDATE notifications SET status = 'sent', sent_at = NOW(), updated_at = NOW()
		WHERE id = $1`, id)
	return err
}

// MarkFailed marks a notification as failed.
func MarkFailed(ctx context.Context, pool *pgxpool.Pool, id int, reason string) error {
	_, err := pool.Exec(ctx, `
		UPDATE notifications SET status = 'failed', last_error = $2, updated_at = NOW()
		WHERE id = $1`, id, reason)
	return err
}
