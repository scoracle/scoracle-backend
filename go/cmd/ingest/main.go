// Command ingest is the Scoracle data ingestion CLI.
//
// Usage:
//
//	scoracle-ingest seed nba --season 2025
//	scoracle-ingest seed nfl --season 2025
//	scoracle-ingest seed football --season 2025 --league 8
//	scoracle-ingest percentiles --sport NBA --season 2025
package main

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"os/signal"
	"time"

	"github.com/joho/godotenv"
	"github.com/spf13/cobra"

	"github.com/albapepper/scoracle-data/go/internal/config"
	"github.com/albapepper/scoracle-data/go/internal/db"
	"github.com/albapepper/scoracle-data/go/internal/provider/bdl"
	"github.com/albapepper/scoracle-data/go/internal/provider/sportmonks"
	"github.com/albapepper/scoracle-data/go/internal/seed"
)

var logger = slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo}))

func main() {
	// Load .env from repo root if present
	_ = godotenv.Load("../.env")
	_ = godotenv.Load(".env")

	root := &cobra.Command{
		Use:   "scoracle-ingest",
		Short: "Scoracle data ingestion CLI",
	}

	root.AddCommand(seedCmd())
	root.AddCommand(percentilesCmd())

	if err := root.Execute(); err != nil {
		os.Exit(1)
	}
}

// --------------------------------------------------------------------------
// seed command
// --------------------------------------------------------------------------

func seedCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "seed",
		Short: "Seed data from external providers",
	}
	cmd.AddCommand(seedNBACmd())
	cmd.AddCommand(seedNFLCmd())
	cmd.AddCommand(seedFootballCmd())
	return cmd
}

func seedNBACmd() *cobra.Command {
	var season int
	cmd := &cobra.Command{
		Use:   "nba",
		Short: "Seed NBA data from BallDontLie",
		RunE: func(cmd *cobra.Command, args []string) error {
			return runSeed(func(ctx context.Context, cfg *config.Config, pool *db.Pool) error {
				if cfg.BDLAPIKey == "" {
					return fmt.Errorf("BALLDONTLIE_API_KEY is required")
				}
				handler := bdl.NewNBAHandler(cfg.BDLAPIKey, logger)
				start := time.Now()
				result := seed.SeedNBA(ctx, pool.Pool, handler, season, logger)
				logger.Info("NBA seed finished", "duration", time.Since(start).Round(time.Second), "summary", result.Summary())
				if len(result.Errors) > 0 {
					for _, e := range result.Errors {
						logger.Error("seed error", "error", e)
					}
				}
				return nil
			})
		},
	}
	cmd.Flags().IntVar(&season, "season", config.SportRegistry["NBA"].CurrentSeason, "Season year")
	return cmd
}

func seedNFLCmd() *cobra.Command {
	var season int
	cmd := &cobra.Command{
		Use:   "nfl",
		Short: "Seed NFL data from BallDontLie",
		RunE: func(cmd *cobra.Command, args []string) error {
			return runSeed(func(ctx context.Context, cfg *config.Config, pool *db.Pool) error {
				if cfg.BDLAPIKey == "" {
					return fmt.Errorf("BALLDONTLIE_API_KEY is required")
				}
				handler := bdl.NewNFLHandler(cfg.BDLAPIKey, logger)
				start := time.Now()
				result := seed.SeedNFL(ctx, pool.Pool, handler, season, logger)
				logger.Info("NFL seed finished", "duration", time.Since(start).Round(time.Second), "summary", result.Summary())
				if len(result.Errors) > 0 {
					for _, e := range result.Errors {
						logger.Error("seed error", "error", e)
					}
				}
				return nil
			})
		},
	}
	cmd.Flags().IntVar(&season, "season", config.SportRegistry["NFL"].CurrentSeason, "Season year")
	return cmd
}

func seedFootballCmd() *cobra.Command {
	var season, leagueID int
	cmd := &cobra.Command{
		Use:   "football",
		Short: "Seed Football data from SportMonks",
		RunE: func(cmd *cobra.Command, args []string) error {
			return runSeed(func(ctx context.Context, cfg *config.Config, pool *db.Pool) error {
				if cfg.SportMonksAPIToken == "" {
					return fmt.Errorf("SPORTMONKS_API_TOKEN is required")
				}
				handler := sportmonks.NewFootballHandler(cfg.SportMonksAPIToken, logger)

				// Resolve SportMonks season ID
				smSeasonID, err := seed.ResolveProviderSeasonID(ctx, pool.Pool, leagueID, season)
				if err != nil {
					return fmt.Errorf("resolve season: %w", err)
				}
				logger.Info("Resolved provider season", "league_id", leagueID, "season", season, "sm_season_id", smSeasonID)

				start := time.Now()
				result := seed.SeedFootballSeason(ctx, pool.Pool, handler, smSeasonID, leagueID, season, leagueID, logger)
				logger.Info("Football seed finished",
					"league_id", leagueID, "duration", time.Since(start).Round(time.Second),
					"summary", result.Summary())
				if len(result.Errors) > 0 {
					for _, e := range result.Errors {
						logger.Error("seed error", "error", e)
					}
				}
				return nil
			})
		},
	}
	cmd.Flags().IntVar(&season, "season", config.SportRegistry["FOOTBALL"].CurrentSeason, "Season year")
	cmd.Flags().IntVar(&leagueID, "league", 8, "League ID (8=PL, 82=BL, 301=L1, 384=SA, 564=LL)")
	return cmd
}

// --------------------------------------------------------------------------
// percentiles command
// --------------------------------------------------------------------------

func percentilesCmd() *cobra.Command {
	var sport string
	var season int
	cmd := &cobra.Command{
		Use:   "percentiles",
		Short: "Recalculate percentile rankings",
		RunE: func(cmd *cobra.Command, args []string) error {
			return runSeed(func(ctx context.Context, cfg *config.Config, pool *db.Pool) error {
				logger.Info("Recalculating percentiles", "sport", sport, "season", season)
				start := time.Now()
				players, teams, err := seed.RecalculatePercentiles(ctx, pool.Pool, sport, season)
				if err != nil {
					return err
				}
				logger.Info("Percentiles complete",
					"sport", sport, "season", season,
					"players_updated", players, "teams_updated", teams,
					"duration", time.Since(start).Round(time.Second))
				return nil
			})
		},
	}
	cmd.Flags().StringVar(&sport, "sport", "NBA", "Sport (NBA, NFL, FOOTBALL)")
	cmd.Flags().IntVar(&season, "season", 2025, "Season year")
	return cmd
}

// --------------------------------------------------------------------------
// Shared setup
// --------------------------------------------------------------------------

// runSeed handles config loading, DB connection, and context cancellation.
func runSeed(fn func(ctx context.Context, cfg *config.Config, pool *db.Pool) error) error {
	ctx, cancel := signal.NotifyContext(context.Background(), os.Interrupt)
	defer cancel()

	cfg, err := config.Load()
	if err != nil {
		return fmt.Errorf("load config: %w", err)
	}

	pool, err := db.New(ctx, cfg)
	if err != nil {
		return fmt.Errorf("connect to database: %w", err)
	}
	defer pool.Close()

	return fn(ctx, cfg, pool)
}
