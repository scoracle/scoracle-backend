package seed

import (
	"context"
	"log/slog"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/provider"
	"github.com/albapepper/scoracle-data/internal/provider/bdl"
)

const sportNBA = "NBA"

// SeedNBA runs the full NBA seed flow: teams -> player stats -> team stats.
// Player profiles are upserted automatically during the player stats phase.
func SeedNBA(ctx context.Context, pool *pgxpool.Pool, handler *bdl.NBAHandler, season int, logger *slog.Logger) SeedResult {
	var result SeedResult

	// 1. Teams
	logger.Info("Seeding NBA teams...")
	teams, err := handler.GetTeams(ctx)
	if err != nil {
		result.AddErrorf("fetch NBA teams: %v", err)
		return result
	}
	for _, team := range teams {
		if err := UpsertTeam(ctx, pool, sportNBA, team); err != nil {
			result.AddErrorf("upsert team %d: %v", team.ID, err)
		} else {
			result.TeamsUpserted++
		}
	}
	logger.Info("NBA teams done", "count", result.TeamsUpserted)

	// 2. Player stats (profiles are auto-upserted)
	logger.Info("Seeding NBA player stats...", "season", season)
	count := 0
	err = handler.GetPlayerStats(ctx, season, "regular", func(ps provider.PlayerStats) error {
		if ps.Player != nil {
			if err := UpsertPlayer(ctx, pool, sportNBA, *ps.Player); err != nil {
				result.AddErrorf("upsert player %d: %v", ps.PlayerID, err)
			} else {
				result.PlayersUpserted++
			}
		}
		if err := UpsertPlayerStats(ctx, pool, sportNBA, season, 0, ps); err != nil {
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
	}
	logger.Info("NBA player stats done", "count", result.PlayerStatsUpserted)

	// 3. Team stats
	logger.Info("Seeding NBA team stats...", "season", season)
	teamStats, err := handler.GetTeamStats(ctx, season, "regular")
	if err != nil {
		result.AddErrorf("fetch NBA team stats: %v", err)
		return result
	}
	for _, ts := range teamStats {
		if err := UpsertTeamStats(ctx, pool, sportNBA, season, 0, ts); err != nil {
			result.AddErrorf("upsert team stats %d: %v", ts.TeamID, err)
		} else {
			result.TeamStatsUpserted++
		}
	}
	logger.Info("NBA team stats done", "count", result.TeamStatsUpserted)

	logger.Info("NBA seed complete", "summary", result.Summary())
	return result
}
