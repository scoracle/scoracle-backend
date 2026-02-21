package notifications

import (
	"context"
	"fmt"
	"math"

	"github.com/jackc/pgx/v5/pgxpool"
)

// DetectChanges finds percentile movements that cross milestones or exceed
// the delta threshold for entities involved in a fixture.
func DetectChanges(ctx context.Context, pool *pgxpool.Pool, fixtureID int) ([]Change, error) {
	rows, err := pool.Query(ctx, "detect_percentile_changes", fixtureID)
	if err != nil {
		return nil, fmt.Errorf("detect percentile changes: %w", err)
	}
	defer rows.Close()

	var changes []Change
	for rows.Next() {
		var c Change
		if err := rows.Scan(
			&c.EntityType, &c.EntityID, &c.Sport, &c.Season, &c.LeagueID,
			&c.StatKey, &c.OldPctile, &c.NewPctile, &c.SampleSize,
		); err != nil {
			return nil, fmt.Errorf("scan percentile change: %w", err)
		}
		c.FixtureID = fixtureID
		if isSignificant(c) {
			changes = append(changes, c)
		}
	}
	return changes, rows.Err()
}

// isSignificant returns true if a change crosses a milestone or has a large delta.
func isSignificant(c Change) bool {
	// Milestone crossings (90th, 95th, 99th)
	for _, m := range milestones {
		if (c.OldPctile < m && c.NewPctile >= m) || (c.OldPctile >= m && c.NewPctile < m) {
			return true
		}
	}
	// Large delta
	return math.Abs(c.NewPctile-c.OldPctile) >= deltaThreshold
}
