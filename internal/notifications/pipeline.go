package notifications

import (
	"context"
	"fmt"
	"log/slog"

	"github.com/jackc/pgx/v5/pgxpool"
)

// Run detects percentile changes from a seeded fixture, fans out to followers,
// schedules delivery times, and persists the notifications.
// Called after seed + percentile recalculation.
func Run(ctx context.Context, pool *pgxpool.Pool, fixtureID int, logger *slog.Logger) error {
	// 1. Detect significant percentile changes
	changes, err := DetectChanges(ctx, pool, fixtureID)
	if err != nil {
		return fmt.Errorf("detect changes: %w", err)
	}
	if len(changes) == 0 {
		logger.Info("No significant percentile changes", "fixture_id", fixtureID)
		return nil
	}
	logger.Info("Detected percentile changes", "fixture_id", fixtureID, "count", len(changes))

	// 2. Get match time for scheduling window
	matchTime, err := GetMatchTime(ctx, pool, fixtureID)
	if err != nil {
		return fmt.Errorf("get match time: %w", err)
	}

	// 3. Fan out: for each change, find followers and build notifications
	var pending []Pending
	for _, change := range changes {
		followers, err := GetFollowers(ctx, pool, change.EntityType, change.EntityID, change.Sport)
		if err != nil {
			logger.Warn("get followers failed", "entity", change.EntityID, "error", err)
			continue
		}
		if len(followers) == 0 {
			continue
		}

		entityName, _ := GetEntityName(ctx, pool, change.EntityType, change.EntityID, change.Sport)
		statDisplay, _ := GetStatDisplayName(ctx, pool, change.Sport, change.StatKey, change.EntityType)
		msg := buildMessage(entityName, statDisplay, change)

		for _, f := range followers {
			pending = append(pending, Pending{
				UserID:      f.UserID,
				EntityType:  change.EntityType,
				EntityID:    change.EntityID,
				Sport:       change.Sport,
				FixtureID:   fixtureID,
				StatKey:     change.StatKey,
				Percentile:  change.NewPctile,
				Message:     msg,
				ScheduleFor: ScheduleDelivery(matchTime, f.Timezone),
			})
		}
	}

	if len(pending) == 0 {
		logger.Info("No followers to notify", "fixture_id", fixtureID)
		return nil
	}

	// 4. Persist
	inserted, err := InsertPending(ctx, pool, pending)
	if err != nil {
		return fmt.Errorf("insert pending: %w", err)
	}
	logger.Info("Notifications scheduled",
		"fixture_id", fixtureID, "count", inserted)
	return nil
}

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

func buildMessage(entityName, statDisplay string, c Change) string {
	pctile := int(c.NewPctile)
	suffix := ordinalSuffix(pctile)
	return fmt.Sprintf("%s is now %d%s percentile in %s", entityName, pctile, suffix, statDisplay)
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
