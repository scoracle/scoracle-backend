package fixture

import (
	"context"
	"fmt"
	"log/slog"
	"sync"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
)

// ProcessPending finds pending fixtures and seeds them.
// Groups by (sport, season, league_id) to deduplicate API calls — one seed
// per group instead of per fixture. Uses a worker pool for concurrency
// across groups.
func ProcessPending(
	ctx context.Context,
	pool *pgxpool.Pool,
	deps *Deps,
	sport string,
	maxFixtures int,
	maxRetries int,
	workers int,
	recalcPercentiles bool,
	logger *slog.Logger,
) SchedulerResult {
	start := time.Now()
	var result SchedulerResult

	pending, err := GetPending(ctx, pool, sport, maxFixtures, maxRetries)
	if err != nil {
		result.Errors = append(result.Errors, err.Error())
		result.Duration = time.Since(start)
		return result
	}

	result.FixturesFound = len(pending)
	if len(pending) == 0 {
		logger.Info("No pending fixtures to seed")
		result.Duration = time.Since(start)
		return result
	}

	logger.Info("Found pending fixtures", "count", len(pending))

	// Group by (sport, season, league_id) — one API fetch per group
	type groupKey struct {
		Sport    string
		Season   int
		LeagueID int
	}
	groups := make(map[groupKey][]Row)
	for _, f := range pending {
		lid := 0
		if f.LeagueID != nil {
			lid = *f.LeagueID
		}
		key := groupKey{f.Sport, f.Season, lid}
		groups[key] = append(groups[key], f)
	}

	// Worker pool: one channel of groups, N workers
	if workers < 1 {
		workers = 1
	}
	if workers > len(groups) {
		workers = len(groups)
	}

	type groupWork struct {
		key      groupKey
		fixtures []Row
	}

	ch := make(chan groupWork, len(groups))
	for key, fixtures := range groups {
		ch <- groupWork{key, fixtures}
	}
	close(ch)

	var mu sync.Mutex
	var wg sync.WaitGroup

	for i := 0; i < workers; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for work := range ch {
				// Seed using the first fixture as representative
				representative := work.fixtures[0]
				seedResult := SeedFixture(ctx, pool, deps, representative.ID, recalcPercentiles, logger)

				mu.Lock()
				for _, f := range work.fixtures {
					r := Result{
						FixtureID:               f.ID,
						Sport:                   f.Sport,
						HomeTeamID:              f.HomeTeamID,
						AwayTeamID:              f.AwayTeamID,
						PlayersUpdated:          seedResult.PlayersUpdated,
						TeamsUpdated:            seedResult.TeamsUpdated,
						PercentilesRecalculated: seedResult.PercentilesRecalculated,
						Success:                 seedResult.Success,
						Error:                   seedResult.Error,
						Duration:                seedResult.Duration,
					}

					result.Results = append(result.Results, r)
					result.FixturesProcessed++

					if r.Success {
						// Mark all group fixtures as seeded (representative already marked)
						if f.ID != representative.ID {
							_ = MarkSeeded(ctx, pool, f.ID)
						}
						result.FixturesSucceeded++
						result.PlayersUpdated += r.PlayersUpdated
						result.TeamsUpdated += r.TeamsUpdated
					} else {
						if f.ID != representative.ID {
							_ = RecordFailure(ctx, pool, f.ID, r.Error)
						}
						result.FixturesFailed++
						result.Errors = append(result.Errors, fmt.Sprintf("fixture %d: %s", f.ID, r.Error))
					}
				}
				mu.Unlock()
			}
		}()
	}

	wg.Wait()
	result.Duration = time.Since(start)

	logger.Info("Scheduler run complete", "summary", result.Summary())
	return result
}
