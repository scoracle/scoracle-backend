package fixture

import (
	"context"
	"fmt"

	"github.com/jackc/pgx/v5/pgxpool"
)

// GetPending returns fixtures ready for seeding.
// Delegates to the get_pending_fixtures() Postgres function which checks
// status, seed delay, and retry limits.
func GetPending(ctx context.Context, pool *pgxpool.Pool, sport string, limit, maxRetries int) ([]Row, error) {
	if limit == 0 {
		limit = defaultMaxFixtures
	}
	if maxRetries == 0 {
		maxRetries = defaultMaxRetries
	}

	var sportParam interface{} = sport
	if sport == "" {
		sportParam = nil
	}

	rows, err := pool.Query(ctx, "get_pending_fixtures", sportParam, limit, maxRetries)
	if err != nil {
		return nil, fmt.Errorf("get pending fixtures: %w", err)
	}
	defer rows.Close()

	var fixtures []Row
	for rows.Next() {
		var f Row
		if err := rows.Scan(
			&f.ID, &f.Sport, &f.LeagueID, &f.Season,
			&f.HomeTeamID, &f.AwayTeamID, &f.StartTime,
			&f.SeedDelayHours, &f.SeedAttempts, &f.ExternalID,
		); err != nil {
			return nil, fmt.Errorf("scan fixture: %w", err)
		}
		fixtures = append(fixtures, f)
	}
	return fixtures, rows.Err()
}

// GetByID returns a single fixture row.
func GetByID(ctx context.Context, pool *pgxpool.Pool, id int) (*Row, error) {
	var f Row
	err := pool.QueryRow(ctx, "fixture_by_id", id).Scan(
		&f.ID, &f.Sport, &f.LeagueID, &f.Season,
		&f.HomeTeamID, &f.AwayTeamID, &f.StartTime,
		&f.SeedDelayHours, &f.SeedAttempts, &f.ExternalID,
	)
	if err != nil {
		return nil, fmt.Errorf("get fixture %d: %w", id, err)
	}
	return &f, nil
}

// MarkSeeded marks a fixture as successfully seeded.
func MarkSeeded(ctx context.Context, pool *pgxpool.Pool, id int) error {
	_, err := pool.Exec(ctx, "SELECT mark_fixture_seeded($1)", id)
	return err
}

// RecordFailure increments seed_attempts and records the error.
func RecordFailure(ctx context.Context, pool *pgxpool.Pool, id int, errMsg string) error {
	_, err := pool.Exec(ctx, `
		UPDATE fixtures
		SET seed_attempts = seed_attempts + 1,
			last_seed_error = $2,
			updated_at = NOW()
		WHERE id = $1`, id, errMsg)
	return err
}
