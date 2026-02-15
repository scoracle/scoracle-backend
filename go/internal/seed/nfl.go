package seed

import (
	"context"
	"log/slog"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/go/internal/provider"
	"github.com/albapepper/scoracle-data/go/internal/provider/bdl"
)

const sportNFL = "NFL"

// SeedNFL runs the full NFL seed flow: teams -> player stats -> team stats.
func SeedNFL(ctx context.Context, pool *pgxpool.Pool, handler *bdl.NFLHandler, season int, logger *slog.Logger) SeedResult {
	var result SeedResult

	// 1. Teams
	logger.Info("Seeding NFL teams...")
	teams, err := handler.GetTeams(ctx)
	if err != nil {
		result.AddErrorf("fetch NFL teams: %v", err)
		return result
	}
	for _, team := range teams {
		if err := UpsertTeam(ctx, pool, sportNFL, team); err != nil {
			result.AddErrorf("upsert team %d: %v", team.ID, err)
		} else {
			result.TeamsUpserted++
		}
	}
	logger.Info("NFL teams done", "count", result.TeamsUpserted)

	// 2. Player stats (profiles are auto-upserted)
	logger.Info("Seeding NFL player stats...", "season", season)
	count := 0
	err = handler.GetPlayerStats(ctx, season, false, func(ps provider.PlayerStats) error {
		if ps.Player != nil {
			if err := UpsertPlayer(ctx, pool, sportNFL, *ps.Player); err != nil {
				result.AddErrorf("upsert player %d: %v", ps.PlayerID, err)
			} else {
				result.PlayersUpserted++
			}
		}
		if err := UpsertPlayerStats(ctx, pool, sportNFL, season, 0, ps); err != nil {
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
	}
	logger.Info("NFL player stats done", "count", result.PlayerStatsUpserted)

	// 3. Team stats
	logger.Info("Seeding NFL team stats...", "season", season)
	teamStats, err := handler.GetTeamStats(ctx, season, "regular")
	if err != nil {
		result.AddErrorf("fetch NFL team stats: %v", err)
		return result
	}
	for _, ts := range teamStats {
		if err := UpsertTeamStats(ctx, pool, sportNFL, season, 0, ts); err != nil {
			result.AddErrorf("upsert team stats %d: %v", ts.TeamID, err)
		} else {
			result.TeamStatsUpserted++
		}
	}
	logger.Info("NFL team stats done", "count", result.TeamStatsUpserted)

	logger.Info("NFL seed complete", "summary", result.Summary())
	return result
}
