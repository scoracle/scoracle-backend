package seed

import (
	"context"
	"fmt"
	"log/slog"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/provider"
	"github.com/albapepper/scoracle-data/internal/provider/sportmonks"
)

const sportFootball = "FOOTBALL"

// SeedFootballSeason seeds all data for a single Football league-season.
//
// Args:
//   - smSeasonID: SportMonks season ID (from provider_seasons table)
//   - leagueID: Our internal league ID (8, 82, 301, 384, 564)
//   - seasonYear: Year (e.g. 2024 for 2024-25 season)
//   - smLeagueID: SportMonks league ID (same as our internal ID for football)
func SeedFootballSeason(
	ctx context.Context,
	pool *pgxpool.Pool,
	handler *sportmonks.FootballHandler,
	smSeasonID int,
	leagueID int,
	seasonYear int,
	smLeagueID int,
	logger *slog.Logger,
) SeedResult {
	var result SeedResult

	// Resolve SportMonks league ID from DB if not provided
	if smLeagueID == 0 {
		var dbSmID *int
		var leagueName string
		err := pool.QueryRow(ctx, "league_lookup", leagueID).Scan(&dbSmID, &leagueName)
		if err != nil || dbSmID == nil {
			result.AddErrorf("no sportmonks_id found for league %d: %v", leagueID, err)
			return result
		}
		smLeagueID = *dbSmID
		logger.Info("Resolved SportMonks league", "league_id", leagueID, "name", leagueName, "sm_id", smLeagueID)
	}

	logger.Info("Seeding football season",
		"sm_season_id", smSeasonID, "league_id", leagueID,
		"season_year", seasonYear, "sm_league_id", smLeagueID)

	// 1. Teams
	logger.Info("Phase 1/3: Seeding teams...")
	teams, err := handler.GetTeams(ctx, smSeasonID)
	if err != nil {
		result.AddErrorf("fetch teams: %v", err)
	} else {
		for _, team := range teams {
			if err := UpsertTeam(ctx, pool, sportFootball, team); err != nil {
				result.AddErrorf("upsert team %d: %v", team.ID, err)
			} else {
				result.TeamsUpserted++
			}
		}
	}
	logger.Info("Teams done", "count", result.TeamsUpserted)

	// 2. Players + Player Stats (via squad iteration)
	logger.Info("Phase 2/3: Seeding players + stats...")
	teamIDs := make([]int, len(teams))
	for i, t := range teams {
		teamIDs[i] = t.ID
	}

	count := 0
	err = handler.GetPlayersWithStats(ctx, smSeasonID, teamIDs, smLeagueID,
		func(ps provider.PlayerStats) error {
			if ps.Player != nil {
				if err := UpsertPlayer(ctx, pool, sportFootball, *ps.Player); err != nil {
					result.AddErrorf("upsert player %d: %v", ps.PlayerID, err)
				} else {
					result.PlayersUpserted++
				}
			}
			if len(ps.Stats) > 0 {
				if err := UpsertPlayerStats(ctx, pool, sportFootball, seasonYear, leagueID, ps); err != nil {
					result.AddErrorf("upsert player stats %d: %v", ps.PlayerID, err)
				} else {
					result.PlayerStatsUpserted++
				}
			}
			count++
			if count%50 == 0 {
				logger.Info("Player progress", "count", count)
			}
			return nil
		})
	if err != nil {
		result.AddErrorf("fetch players/stats: %v", err)
	}
	logger.Info("Players + stats done",
		"players", result.PlayersUpserted, "stats", result.PlayerStatsUpserted)

	// 3. Team Stats (Standings)
	logger.Info("Phase 3/3: Seeding standings...")
	teamStats, err := handler.GetTeamStats(ctx, smSeasonID)
	if err != nil {
		result.AddErrorf("fetch standings: %v", err)
	} else {
		for _, ts := range teamStats {
			if ts.Team != nil {
				if err := UpsertTeam(ctx, pool, sportFootball, *ts.Team); err != nil {
					result.AddErrorf("upsert team from standings %d: %v", ts.TeamID, err)
				}
			}
			if err := UpsertTeamStats(ctx, pool, sportFootball, seasonYear, leagueID, ts); err != nil {
				result.AddErrorf("upsert team stats %d: %v", ts.TeamID, err)
			} else {
				result.TeamStatsUpserted++
			}
		}
	}
	logger.Info("Standings done", "count", result.TeamStatsUpserted)

	logger.Info("Football season seed complete",
		"league_id", leagueID, "season", seasonYear, "summary", result.Summary())
	return result
}

// ResolveProviderSeasonID looks up the SportMonks season ID from the provider_seasons table.
func ResolveProviderSeasonID(ctx context.Context, pool *pgxpool.Pool, leagueID, seasonYear int) (int, error) {
	var smSeasonID *int
	err := pool.QueryRow(ctx, "resolve_provider_season", leagueID, seasonYear).Scan(&smSeasonID)
	if err != nil {
		return 0, fmt.Errorf("resolve provider season: %w", err)
	}
	if smSeasonID == nil {
		return 0, fmt.Errorf("no provider season found for league %d season %d", leagueID, seasonYear)
	}
	return *smSeasonID, nil
}
