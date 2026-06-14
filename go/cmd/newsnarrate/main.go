// newsnarrate — CLI to generate (or dry-run) the Gemma narratives stage for one
// entity: the distinct storylines in its recent vetted news, each a write-up with
// a per-narrative impact.
//
//	# dry-run (default): read vetted corpus + group via Gemma, print, DO NOT persist
//	go run ./cmd/newsnarrate -entity-type team -entity-id 18 -sport FOOTBALL
//	# persist to news_summaries (requires migration 085 applied)
//	go run ./cmd/newsnarrate -entity-type team -entity-id 18 -sport FOOTBALL -persist
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
	entityID := flag.Int("entity-id", 0, "canonical entity id")
	sport := flag.String("sport", "", "NBA | NFL | FOOTBALL")
	trigger := flag.String("trigger", "manual", "manual | periodic | news_spike")
	persist := flag.Bool("persist", false, "persist to news_summaries (default: dry-run, no write)")
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
	narrator := ml.NewNewsNarrator(pool, ollama)

	ctx, cancel := context.WithTimeout(context.Background(), cfg.OllamaTimeout+10*time.Second)
	defer cancel()

	sportUpper := strings.ToUpper(*sport)
	name, err := lookupEntityName(ctx, pool, *entityType, *entityID, sportUpper)
	if err != nil || name == "" {
		logger.Error("entity lookup failed", "error", err)
		os.Exit(1)
	}

	res, err := narrator.Generate(ctx, ml.NarrativesRequest{
		EntityType:  *entityType,
		EntityID:    *entityID,
		EntityName:  name,
		Sport:       sportUpper,
		TriggerType: *trigger,
		DryRun:      !*persist,
	})
	if err != nil {
		logger.Error("narratives generation failed", "error", err)
		os.Exit(1)
	}

	mode := "DRY-RUN (not persisted)"
	if *persist {
		mode = "PERSISTED to news_summaries"
	}
	fmt.Printf("\n=== Narratives: %s (%s %d, %s) — %s ===\n", name, *entityType, *entityID, sportUpper, mode)
	if res.SkippedNoCorpus {
		fmt.Println("(no usable narratives — null marker)")
	} else {
		for i, n := range res.Narratives {
			fmt.Printf("\n[%d] (impact %d, %d articles) %s\n", i+1, n.Impact, len(n.InputNewsIDs), n.Title)
			fmt.Printf("    %s\n", n.Body)
		}
	}
	fmt.Printf("\n(model=%s prompt=%s duration=%s narratives=%d)\n",
		res.Model, res.PromptVersion, res.Duration.Round(10*time.Millisecond), len(res.Narratives))
}

func lookupEntityName(ctx context.Context, pool *pgxpool.Pool, entityType string, id int, sport string) (string, error) {
	q := `SELECT name FROM teams WHERE id = $1 AND sport = $2`
	if entityType == "player" {
		q = `SELECT name FROM players WHERE id = $1 AND sport = $2`
	}
	var name string
	err := pool.QueryRow(ctx, q, id, sport).Scan(&name)
	return name, err
}
