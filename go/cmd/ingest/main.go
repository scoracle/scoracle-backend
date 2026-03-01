// Command ingest is the Scoracle data ingestion CLI.
//
// Usage:
//
//	scoracle-ingest seed nba --season 2025
//	scoracle-ingest seed nfl --season 2025
//	scoracle-ingest seed football --season 2025 --league 8
//	scoracle-ingest percentiles --sport NBA --season 2025
//	scoracle-ingest fixtures process --sport NBA --max 10 --workers 2
//	scoracle-ingest fixtures seed --id 42
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

	"github.com/albapepper/scoracle-data/internal/config"
	"github.com/albapepper/scoracle-data/internal/db"
	"github.com/albapepper/scoracle-data/internal/fixture"
	"github.com/albapepper/scoracle-data/internal/provider/bdl"
	"github.com/albapepper/scoracle-data/internal/provider/sportmonks"
	"github.com/albapepper/scoracle-data/internal/seed"
)

var logger = slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo}))

func main() {
	// Load .env if present
	_ = godotenv.Load(".env")

	root := &cobra.Command{
		Use:   "scoracle-ingest",
		Short: "Scoracle data ingestion CLI",
	}

	root.AddCommand(seedCmd())
	root.AddCommand(percentilesCmd())
	root.AddCommand(fixturesCmd())

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
// fixtures command
// --------------------------------------------------------------------------

func fixturesCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "fixtures",
		Short: "Process post-match fixtures (seed stats after games finish)",
	}
	cmd.AddCommand(fixturesProcessCmd())
	cmd.AddCommand(fixturesSeedCmd())
	return cmd
}

func fixturesProcessCmd() *cobra.Command {
	var (
		sport           string
		maxFixtures     int
		workers         int
		maxRetries      int
		skipPercentiles bool
	)
	cmd := &cobra.Command{
		Use:   "process",
		Short: "Find and seed all pending fixtures",
		RunE: func(cmd *cobra.Command, args []string) error {
			return runSeed(func(ctx context.Context, cfg *config.Config, pool *db.Pool) error {
				deps := buildFixtureDeps(cfg)
				start := time.Now()
				result := fixture.ProcessPending(
					ctx, pool.Pool, deps, sport,
					maxFixtures, maxRetries, workers,
					!skipPercentiles, logger,
				)
				logger.Info("Fixtures process finished",
					"duration", time.Since(start).Round(time.Second),
					"summary", result.Summary())
				if len(result.Errors) > 0 {
					for _, e := range result.Errors {
						logger.Error("fixture error", "error", e)
					}
				}
				return nil
			})
		},
	}
	cmd.Flags().StringVar(&sport, "sport", "", "Filter by sport (NBA, NFL, FOOTBALL); empty = all")
	cmd.Flags().IntVar(&maxFixtures, "max", 50, "Maximum fixtures to process")
	cmd.Flags().IntVar(&workers, "workers", 2, "Concurrent worker count")
	cmd.Flags().IntVar(&maxRetries, "max-retries", 3, "Skip fixtures with this many failed attempts")
	cmd.Flags().BoolVar(&skipPercentiles, "skip-percentiles", false, "Skip percentile recalculation after seeding")
	return cmd
}

func fixturesSeedCmd() *cobra.Command {
	var (
		fixtureID       int
		skipPercentiles bool
	)
	cmd := &cobra.Command{
		Use:   "seed",
		Short: "Seed a single fixture by ID",
		RunE: func(cmd *cobra.Command, args []string) error {
			if fixtureID == 0 {
				return fmt.Errorf("--id is required")
			}
			return runSeed(func(ctx context.Context, cfg *config.Config, pool *db.Pool) error {
				deps := buildFixtureDeps(cfg)
				start := time.Now()
				result := fixture.SeedFixture(
					ctx, pool.Pool, deps,
					fixtureID, !skipPercentiles, logger,
				)
				logger.Info("Fixture seed finished",
					"duration", time.Since(start).Round(time.Second),
					"summary", result.Summary())
				if !result.Success {
					return fmt.Errorf("fixture %d failed: %s", fixtureID, result.Error)
				}
				return nil
			})
		},
	}
	cmd.Flags().IntVar(&fixtureID, "id", 0, "Fixture ID to seed")
	cmd.Flags().BoolVar(&skipPercentiles, "skip-percentiles", false, "Skip percentile recalculation after seeding")
	return cmd
}

// buildFixtureDeps creates handler dependencies based on configured API keys.
func buildFixtureDeps(cfg *config.Config) *fixture.Deps {
	deps := &fixture.Deps{}
	if cfg.BDLAPIKey != "" {
		deps.NBAHandler = bdl.NewNBAHandler(cfg.BDLAPIKey, logger)
		deps.NFLHandler = bdl.NewNFLHandler(cfg.BDLAPIKey, logger)
	}
	if cfg.SportMonksAPIToken != "" {
		deps.FootballHandler = sportmonks.NewFootballHandler(cfg.SportMonksAPIToken, logger)
	}
	return deps
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
