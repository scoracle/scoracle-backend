package fixture

import (
	"context"
	"fmt"
	"log/slog"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/maintenance"
	"github.com/albapepper/scoracle-data/internal/notifications"
	"github.com/albapepper/scoracle-data/internal/provider"
	"github.com/albapepper/scoracle-data/internal/provider/bdl"
	"github.com/albapepper/scoracle-data/internal/provider/sportmonks"
	"github.com/albapepper/scoracle-data/internal/seed"
)

// Deps holds the handler dependencies needed for seeding.
// Each sport needs its own handler; only the relevant one is used per fixture.
type Deps struct {
	NBAHandler      *bdl.NBAHandler
	NFLHandler      *bdl.NFLHandler
	FootballHandler *sportmonks.FootballHandler
}

// SeedFixture seeds stats for a single fixture by calling provider handlers
// directly, then upserting via seed.Upsert* functions.
//
// Phase 1: Uses full-season fetch through existing provider handlers.
// Phase 2 (future): Add team-filtered provider methods for targeted fetching
// using fixture.HomeTeamID / AwayTeamID.
//
// On success, marks the fixture as seeded and runs the notification pipeline.
// On failure, records the error and increments seed_attempts.
func SeedFixture(
	ctx context.Context,
	pool *pgxpool.Pool,
	deps *Deps,
	fixtureID int,
	recalcPercentiles bool,
	logger *slog.Logger,
) Result {
	start := time.Now()

	f, err := GetByID(ctx, pool, fixtureID)
	if err != nil {
		return Result{FixtureID: fixtureID, Error: err.Error()}
	}

	result := Result{
		FixtureID:  f.ID,
		Sport:      f.Sport,
		HomeTeamID: f.HomeTeamID,
		AwayTeamID: f.AwayTeamID,
	}

	logger.Info("Seeding fixture",
		"fixture_id", f.ID, "sport", f.Sport, "season", f.Season,
		"home_team", f.HomeTeamID, "away_team", f.AwayTeamID)

	var seedResult seed.SeedResult

	switch f.Sport {
	case "NBA":
		if deps.NBAHandler == nil {
			result.Error = "NBA handler not configured"
			_ = RecordFailure(ctx, pool, f.ID, result.Error)
			result.Duration = time.Since(start)
			return result
		}
		seedResult = seedNBAFixture(ctx, pool, deps.NBAHandler, f, logger)

	case "NFL":
		if deps.NFLHandler == nil {
			result.Error = "NFL handler not configured"
			_ = RecordFailure(ctx, pool, f.ID, result.Error)
			result.Duration = time.Since(start)
			return result
		}
		seedResult = seedNFLFixture(ctx, pool, deps.NFLHandler, f, logger)

	case "FOOTBALL":
		if deps.FootballHandler == nil {
			result.Error = "Football handler not configured"
			_ = RecordFailure(ctx, pool, f.ID, result.Error)
			result.Duration = time.Since(start)
			return result
		}
		seedResult = seedFootballFixture(ctx, pool, deps.FootballHandler, f, logger)

	default:
		result.Error = fmt.Sprintf("unknown sport: %s", f.Sport)
		_ = RecordFailure(ctx, pool, f.ID, result.Error)
		result.Duration = time.Since(start)
		return result
	}

	result.PlayersUpdated = seedResult.PlayerStatsUpserted
	result.TeamsUpdated = seedResult.TeamStatsUpserted

	// Check for seed errors
	if len(seedResult.Errors) > 0 {
		result.Error = seedResult.Errors[0]
		_ = RecordFailure(ctx, pool, f.ID, result.Error)
		result.Duration = time.Since(start)
		return result
	}

	// Percentile recalculation
	if recalcPercentiles && seedResult.PlayerStatsUpserted > 0 {
		// Archive current percentiles before recalculating (for notification diffing)
		if err := seed.ArchivePercentiles(ctx, pool, f.Sport, f.Season); err != nil {
			logger.Warn("Percentile archiving failed", "error", err)
		}

		_, _, err := seed.RecalculatePercentiles(ctx, pool, f.Sport, f.Season)
		if err != nil {
			logger.Warn("Percentile recalculation failed", "error", err)
		} else {
			result.PercentilesRecalculated = true
		}
	}

	// Mark seeded
	if err := MarkSeeded(ctx, pool, f.ID); err != nil {
		logger.Warn("Failed to mark fixture seeded", "fixture_id", f.ID, "error", err)
	}

	result.Success = true
	result.Duration = time.Since(start)

	// Post-ingestion hook: refresh materialized views
	if err := maintenance.RefreshMaterializedViews(ctx, pool, logger); err != nil {
		logger.Warn("Post-ingestion view refresh failed", "error", err)
	}

	// Run notification pipeline (fire-and-warn, never blocks seeding)
	if result.PercentilesRecalculated {
		if err := notifications.Run(ctx, pool, f.ID, logger); err != nil {
			logger.Warn("Notification pipeline failed", "fixture_id", f.ID, "error", err)
		}
	}

	logger.Info("Fixture seeded", "summary", result.Summary())
	return result
}

// --------------------------------------------------------------------------
// Per-sport seeders — call provider handlers + seed.Upsert* directly
// --------------------------------------------------------------------------

func seedNBAFixture(ctx context.Context, pool *pgxpool.Pool, handler *bdl.NBAHandler, f *Row, logger *slog.Logger) seed.SeedResult {
	var result seed.SeedResult

	// Player stats (profiles auto-upserted)
	logger.Info("Seeding NBA player stats...", "season", f.Season)
	count := 0
	err := handler.GetPlayerStats(ctx, f.Season, "regular", func(ps provider.PlayerStats) error {
		if ps.Player != nil {
			if err := seed.UpsertPlayer(ctx, pool, "NBA", *ps.Player); err != nil {
				result.AddErrorf("upsert player %d: %v", ps.PlayerID, err)
			} else {
				result.PlayersUpserted++
			}
		}
		if err := seed.UpsertPlayerStats(ctx, pool, "NBA", f.Season, 0, ps); err != nil {
			result.AddErrorf("upsert player stats %d: %v", ps.PlayerID, err)
		} else {
			result.PlayerStatsUpserted++
		}
		count++
		if count%50 == 0 {
			logger.Info("NBA player stats progress", "processed", count)
		}
		return nil
	})
	if err != nil {
		result.AddErrorf("fetch NBA player stats: %v", err)
		return result
	}
	logger.Info("NBA player stats done", "count", result.PlayerStatsUpserted)

	// Team stats
	logger.Info("Seeding NBA team stats...", "season", f.Season)
	teamStats, err := handler.GetTeamStats(ctx, f.Season, "regular")
	if err != nil {
		result.AddErrorf("fetch NBA team stats: %v", err)
		return result
	}
	for _, ts := range teamStats {
		if err := seed.UpsertTeamStats(ctx, pool, "NBA", f.Season, 0, ts); err != nil {
			result.AddErrorf("upsert team stats %d: %v", ts.TeamID, err)
		} else {
			result.TeamStatsUpserted++
		}
	}
	logger.Info("NBA team stats done", "count", result.TeamStatsUpserted)

	return result
}

func seedNFLFixture(ctx context.Context, pool *pgxpool.Pool, handler *bdl.NFLHandler, f *Row, logger *slog.Logger) seed.SeedResult {
	var result seed.SeedResult

	// Player stats
	logger.Info("Seeding NFL player stats...", "season", f.Season)
	count := 0
	err := handler.GetPlayerStats(ctx, f.Season, false, func(ps provider.PlayerStats) error {
		if ps.Player != nil {
			if err := seed.UpsertPlayer(ctx, pool, "NFL", *ps.Player); err != nil {
				result.AddErrorf("upsert player %d: %v", ps.PlayerID, err)
			} else {
				result.PlayersUpserted++
			}
		}
		if err := seed.UpsertPlayerStats(ctx, pool, "NFL", f.Season, 0, ps); err != nil {
			result.AddErrorf("upsert player stats %d: %v", ps.PlayerID, err)
		} else {
			result.PlayerStatsUpserted++
		}
		count++
		if count%50 == 0 {
			logger.Info("NFL player stats progress", "processed", count)
		}
		return nil
	})
	if err != nil {
		result.AddErrorf("fetch NFL player stats: %v", err)
		return result
	}
	logger.Info("NFL player stats done", "count", result.PlayerStatsUpserted)

	// Team stats
	logger.Info("Seeding NFL team stats...", "season", f.Season)
	teamStats, err := handler.GetTeamStats(ctx, f.Season, "regular")
	if err != nil {
		result.AddErrorf("fetch NFL team stats: %v", err)
		return result
	}
	for _, ts := range teamStats {
		if err := seed.UpsertTeamStats(ctx, pool, "NFL", f.Season, 0, ts); err != nil {
			result.AddErrorf("upsert team stats %d: %v", ts.TeamID, err)
		} else {
			result.TeamStatsUpserted++
		}
	}
	logger.Info("NFL team stats done", "count", result.TeamStatsUpserted)

	return result
}

func seedFootballFixture(ctx context.Context, pool *pgxpool.Pool, handler *sportmonks.FootballHandler, f *Row, logger *slog.Logger) seed.SeedResult {
	var result seed.SeedResult

	leagueID := 0
	if f.LeagueID != nil {
		leagueID = *f.LeagueID
	}

	smSeasonID, err := seed.ResolveProviderSeasonID(ctx, pool, leagueID, f.Season)
	if err != nil {
		result.AddErrorf("resolve season: %v", err)
		return result
	}

	// Teams
	logger.Info("Seeding football teams...", "sm_season_id", smSeasonID)
	teams, err := handler.GetTeams(ctx, smSeasonID)
	if err != nil {
		result.AddErrorf("fetch teams: %v", err)
	} else {
		for _, team := range teams {
			if err := seed.UpsertTeam(ctx, pool, "FOOTBALL", team); err != nil {
				result.AddErrorf("upsert team %d: %v", team.ID, err)
			} else {
				result.TeamsUpserted++
			}
		}
	}

	// Resolve SportMonks league ID
	var smLeagueID int
	var dbSmID *int
	var leagueName string
	err = pool.QueryRow(ctx, "league_lookup", leagueID).Scan(&dbSmID, &leagueName)
	if err != nil || dbSmID == nil {
		result.AddErrorf("no sportmonks_id for league %d: %v", leagueID, err)
		return result
	}
	smLeagueID = *dbSmID

	// Player stats via squad iteration
	logger.Info("Seeding football player stats...")
	teamIDs := make([]int, len(teams))
	for i, t := range teams {
		teamIDs[i] = t.ID
	}

	count := 0
	err = handler.GetPlayersWithStats(ctx, smSeasonID, teamIDs, smLeagueID,
		func(ps provider.PlayerStats) error {
			if ps.Player != nil {
				if err := seed.UpsertPlayer(ctx, pool, "FOOTBALL", *ps.Player); err != nil {
					result.AddErrorf("upsert player %d: %v", ps.PlayerID, err)
				} else {
					result.PlayersUpserted++
				}
			}
			if len(ps.Stats) > 0 {
				if err := seed.UpsertPlayerStats(ctx, pool, "FOOTBALL", f.Season, leagueID, ps); err != nil {
					result.AddErrorf("upsert player stats %d: %v", ps.PlayerID, err)
				} else {
					result.PlayerStatsUpserted++
				}
			}
			count++
			if count%50 == 0 {
				logger.Info("Football player stats progress", "count", count)
			}
			return nil
		})
	if err != nil {
		result.AddErrorf("fetch players/stats: %v", err)
	}
	logger.Info("Football player stats done", "count", result.PlayerStatsUpserted)

	// Team stats (standings)
	logger.Info("Seeding football standings...")
	teamStats, err := handler.GetTeamStats(ctx, smSeasonID)
	if err != nil {
		result.AddErrorf("fetch standings: %v", err)
	} else {
		for _, ts := range teamStats {
			if ts.Team != nil {
				_ = seed.UpsertTeam(ctx, pool, "FOOTBALL", *ts.Team)
			}
			if err := seed.UpsertTeamStats(ctx, pool, "FOOTBALL", f.Season, leagueID, ts); err != nil {
				result.AddErrorf("upsert team stats %d: %v", ts.TeamID, err)
			} else {
				result.TeamStatsUpserted++
			}
		}
	}
	logger.Info("Football standings done", "count", result.TeamStatsUpserted)

	return result
}
