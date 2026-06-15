// statcommentary — CLI to generate (or dry-run) the Gemma stats-rail commentary
// for one entity: the on-field IDENTITY analysis derived from its rating-engine
// datapoints (composite = how well, special = how).
//
//	# dry-run (default): read rating profile + narrate via Gemma, print, DO NOT persist
//	go run ./cmd/statcommentary -entity-type player -entity-id 237 -sport NBA
//	# persist to stat_summaries (requires migration 086 applied)
//	go run ./cmd/statcommentary -entity-type team -entity-id 18 -sport FOOTBALL -persist
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
	"github.com/albapepper/scoracle-data/internal/corpus"
	"github.com/albapepper/scoracle-data/internal/ml"
)

func main() {
	entityType := flag.String("entity-type", "player", "player | team")
	entityID := flag.Int("entity-id", 0, "canonical entity id")
	sport := flag.String("sport", "", "NBA | NFL | FOOTBALL")
	trigger := flag.String("trigger", "manual", "manual | periodic | stat_change")
	persist := flag.Bool("persist", false, "persist to stat_summaries (default: dry-run, no write)")
	flag.Parse()

	logger := slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelWarn}))
	_ = godotenv.Load(".env.local", ".env")
	cfg, err := config.Load()
	if err != nil {
		logger.Error("config load failed", "error", err)
		os.Exit(1)
	}
	if *entityID <= 0 || *sport == "" {
		fmt.Fprintln(os.Stderr, "-entity-id and -sport are required")
		os.Exit(2)
	}

	pool, err := pgxpool.New(context.Background(), cfg.DatabaseURL)
	if err != nil {
		logger.Error("db connect failed", "error", err)
		os.Exit(1)
	}
	defer pool.Close()

	ollama := ml.NewOllamaClient(cfg.OllamaBaseURL, cfg.OllamaModel, cfg.OllamaTimeout)
	if err := ollama.Ping(context.Background()); err != nil {
		logger.Error("ollama unreachable", "error", err, "base_url", cfg.OllamaBaseURL)
		os.Exit(1)
	}
	commentator := ml.NewStatCommentator(pool, ollama)

	ctx, cancel := context.WithTimeout(context.Background(), cfg.OllamaTimeout+10*time.Second)
	defer cancel()

	sportUpper := strings.ToUpper(*sport)
	name, err := corpus.LookupEntityName(ctx, pool, *entityType, *entityID, sportUpper)
	if err != nil || name == "" {
		logger.Error("entity lookup failed", "error", err)
		os.Exit(1)
	}

	res, err := commentator.Generate(ctx, ml.StatCommentaryRequest{
		EntityType:  *entityType,
		EntityID:    *entityID,
		EntityName:  name,
		Sport:       sportUpper,
		TriggerType: *trigger,
		DryRun:      !*persist,
	})
	if err != nil {
		logger.Error("stat commentary generation failed", "error", err)
		os.Exit(1)
	}

	mode := "DRY-RUN (not persisted)"
	if *persist {
		mode = "PERSISTED to stat_summaries"
	}
	fmt.Printf("\n=== Stat commentary: %s (%s %d, %s) — %s ===\n", name, *entityType, *entityID, sportUpper, mode)
	if res.SkippedNoStats {
		fmt.Println("(no usable rating profile — null marker)")
	} else {
		fmt.Printf("\n[notability %d/100] %v\n\n%s\n", res.Notability, res.NotabilityComponents, res.Body)
	}
	fmt.Printf("\n(model=%s prompt=%s duration=%s)\n",
		res.Model, res.PromptVersion, res.Duration.Round(10*time.Millisecond))
}
