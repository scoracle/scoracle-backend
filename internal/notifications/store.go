package notifications

import (
	"context"
	"fmt"
	"time"

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

// GetMatchTime returns the start time of a fixture.
func GetMatchTime(ctx context.Context, pool *pgxpool.Pool, fixtureID int) (time.Time, error) {
	var t time.Time
	if err := pool.QueryRow(ctx, "fixture_start_time", fixtureID).Scan(&t); err != nil {
		return time.Time{}, fmt.Errorf("get match time: %w", err)
	}
	return t, nil
}

// InsertPending persists a batch of scheduled notifications.
func InsertPending(ctx context.Context, pool *pgxpool.Pool, pending []Pending) (int, error) {
	inserted := 0
	for _, n := range pending {
		_, err := pool.Exec(ctx, `
			INSERT INTO notifications (
				user_id, entity_type, entity_id, sport, fixture_id,
				stat_key, percentile, message, status, scheduled_for
			) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'scheduled',$9)`,
			n.UserID, n.EntityType, n.EntityID, n.Sport, n.FixtureID,
			n.StatKey, n.Percentile, n.Message, n.ScheduleFor,
		)
		if err != nil {
			return inserted, fmt.Errorf("insert notification: %w", err)
		}
		inserted++
	}
	return inserted, nil
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
