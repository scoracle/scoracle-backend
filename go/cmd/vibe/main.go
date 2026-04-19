// vibe — CLI for generating a single vibe blurb on demand.
//
// Useful for prompt iteration without waiting for a milestone to fire.
// Reads from the same tables the listener-driven worker will use.
//
// Usage:
//   go run ./cmd/vibe -entity-type player -entity-id 237 -sport NBA
//   go run ./cmd/vibe -entity-type team -entity-id 14 -sport NBA -trigger manual
//
// Env: DATABASE_PRIVATE_URL (or fallbacks) + OLLAMA_* (see config.go).
package main

import (
	"context"
	"flag"
	"fmt"
	"log/slog"
	"os"
	"strings"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/joho/godotenv"

	"github.com/albapepper/scoracle-data/internal/config"
	"github.com/albapepper/scoracle-data/internal/ml"
)

func main() {
	entityType := flag.String("entity-type", "player", "player | team")
	entityID := flag.Int("entity-id", 0, "canonical entity id (required)")
	sport := flag.String("sport", "", "NBA | NFL | FOOTBALL (required)")
	trigger := flag.String("trigger", "manual", "milestone | manual | periodic")
	flag.Parse()

	if *entityID <= 0 || *sport == "" {
		fmt.Fprintln(os.Stderr, "entity-id and sport are required")
		flag.Usage()
		os.Exit(2)
	}

	logger := slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo}))

	_ = godotenv.Load(".env.local", ".env")

	cfg, err := config.Load()
	if err != nil {
		logger.Error("config load failed", "error", err)
		os.Exit(1)
	}

	ctx, cancel := context.WithTimeout(context.Background(), cfg.OllamaTimeout+10*time.Second)
	defer cancel()

	pool, err := pgxpool.New(ctx, cfg.DatabaseURL)
	if err != nil {
		logger.Error("db connect failed", "error", err)
		os.Exit(1)
	}
	defer pool.Close()

	// Look up entity name so the prompt has a human-readable subject.
	sportUpper := strings.ToUpper(*sport)
	var entityName string
	var query string
	if *entityType == "player" {
		query = `SELECT name FROM players WHERE id = $1 AND sport = $2`
	} else {
		query = `SELECT name FROM teams WHERE id = $1 AND sport = $2`
	}
	if err := pool.QueryRow(ctx, query, *entityID, sportUpper).Scan(&entityName); err != nil {
		logger.Error("entity lookup failed", "error", err, "type", *entityType, "id", *entityID, "sport", sportUpper)
		os.Exit(1)
	}

	ollama := ml.NewOllamaClient(cfg.OllamaBaseURL, cfg.OllamaModel, cfg.OllamaTimeout)
	if err := ollama.Ping(ctx); err != nil {
		logger.Error("ollama unreachable", "error", err, "base_url", cfg.OllamaBaseURL)
		os.Exit(1)
	}

	gen := ml.NewGenerator(pool, ollama)
	result, err := gen.Generate(ctx, ml.VibeRequest{
		EntityType:  *entityType,
		EntityID:    *entityID,
		EntityName:  entityName,
		Sport:       sportUpper,
		TriggerType: *trigger,
	})
	if err != nil {
		logger.Error("vibe generate failed", "error", err)
		os.Exit(1)
	}

	fmt.Printf("\n--- Vibe for %s (%s %d, %s) ---\n", entityName, *entityType, *entityID, sportUpper)
	fmt.Println(result.Blurb)
	fmt.Printf("\n(model=%s prompt=%s duration=%s news=%d tweets=%d)\n",
		result.Model, result.PromptVersion, result.Duration.Round(10*time.Millisecond),
		len(result.InputNewsIDs), len(result.InputTweetIDs))
}
