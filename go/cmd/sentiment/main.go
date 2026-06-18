// sentiment — CLI for generating entity sentiment scores (1-100).
//
// Two modes:
//
//	single (default) — generate one score for a given entity.
//	  go run ./cmd/sentiment -entity-type player -entity-id 237 -sport NBA
//	  go run ./cmd/sentiment -entity-type team -entity-id 14 -sport NBA
//
//	corpus — RSS-sweep every team across NBA/NFL/FOOTBALL, then run Gemma
//	         only on entities that picked up fresh news in this run. The
//	         corpus presence is the candidate signal — every Gemma call
//	         is guaranteed real input. Cross-entity linking inside the
//	         news write-through pulls in co-mentioned players for free,
//	         so the player layer is included without per-player RSS calls.
//	         Intended for a noon + midnight cron pair.
//	  go run ./cmd/sentiment -mode corpus
//	  go run ./cmd/sentiment -mode corpus -sport NBA  # one-sport smoke run
//
// Real-time coverage between corpus runs is handled inside the API by the
// news-volume LISTEN/NOTIFY worker (internal/listener/news_volume_worker.go),
// not by this CLI.
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
	mode := flag.String("mode", "single", "single | corpus")

	// single-mode flags
	entityType := flag.String("entity-type", "player", "[single] player | team")
	entityID := flag.Int("entity-id", 0, "[single] canonical entity id")
	trigger := flag.String("trigger", "manual", "[single] manual | periodic | news_spike")

	// shared + corpus-mode flags
	sport := flag.String("sport", "", "NBA | NFL | FOOTBALL | all (single requires it; corpus defaults to all)")
	throttleMs := flag.Int("throttle-ms", 0, "[corpus] pause N ms between generations; 0 = back-to-back")
	corpusSkipHours := flag.Int("corpus-skip-recent-hours", 10, "[corpus] skip entities with a sentiment row newer than this; <= half the cron cadence")
	corpusRSSPause := flag.Int("corpus-rss-pause-ms", 100, "[corpus] pause between team RSS calls to be polite to Google News")
	corpusRSSLimit := flag.Int("corpus-rss-limit", 10, "[corpus] articles per team RSS call")

	flag.Parse()

	logger := slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo}))

	_ = godotenv.Load(".env.local", ".env")
	cfg, err := config.Load()
	if err != nil {
		logger.Error("config load failed", "error", err)
		os.Exit(1)
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
	gen := ml.NewVibeGenerator(pool, ollama)

	switch *mode {
	case "single":
		runSingle(pool, gen, *entityType, *entityID, *sport, *trigger, cfg.OllamaTimeout, logger)
	case "corpus":
		runCorpus(pool, gen, *sport, *corpusSkipHours, *throttleMs, *corpusRSSPause, *corpusRSSLimit, cfg.OllamaTimeout, logger)
	default:
		fmt.Fprintf(os.Stderr, "unknown -mode %q; valid: single | corpus\n", *mode)
		os.Exit(2)
	}
}

// ---------------------------------------------------------------------------
// Single mode
// ---------------------------------------------------------------------------

func runSingle(
	pool *pgxpool.Pool, gen *ml.VibeGenerator,
	entityType string, entityID int,
	sport string, trigger string,
	timeout time.Duration, logger *slog.Logger,
) {
	if entityID <= 0 || sport == "" {
		fmt.Fprintln(os.Stderr, "-entity-id and -sport are required in single mode")
		os.Exit(2)
	}

	ctx, cancel := context.WithTimeout(context.Background(), timeout+10*time.Second)
	defer cancel()

	sportUpper := strings.ToUpper(sport)
	entityName, err := corpus.LookupEntityName(ctx, pool, entityType, entityID, sportUpper)
	if err != nil {
		logger.Error("entity lookup failed", "error", err)
		os.Exit(1)
	}

	result, err := gen.Generate(ctx, ml.VibeRequest{
		EntityType:  entityType,
		EntityID:    entityID,
		EntityName:  entityName,
		Sport:       sportUpper,
		TriggerType: trigger,
	})
	if err != nil {
		logger.Error("sentiment generate failed", "error", err)
		os.Exit(1)
	}

	fmt.Printf("\n--- Sentiment for %s (%s %d, %s) ---\n", entityName, entityType, entityID, sportUpper)
	if result.SkippedNoCorpus {
		fmt.Println("Sentiment: (no data — corpus empty)")
	} else {
		fmt.Printf("Sentiment: %d/100\n", result.Sentiment)
	}
	fmt.Printf("\n(model=%s prompt=%s duration=%s news=%d tweets=%d)\n",
		result.Model, result.PromptVersion, result.Duration.Round(10*time.Millisecond),
		len(result.InputNewsIDs), len(result.InputTweetIDs))
}

// ---------------------------------------------------------------------------
// Corpus mode — RSS sweep + corpus-driven Gemma queue
// ---------------------------------------------------------------------------

func runCorpus(
	pool *pgxpool.Pool, gen *ml.VibeGenerator,
	sportArg string, skipRecentHours, throttleMs, rssPauseMs, rssLimit int,
	gemmaTimeout time.Duration, logger *slog.Logger,
) {
	sports := []string{"NBA", "NFL", "FOOTBALL"}
	if s := strings.ToLower(strings.TrimSpace(sportArg)); s != "" && s != "all" {
		sports = []string{strings.ToUpper(sportArg)}
	}

	ctx := context.Background()

	// Phase 1 — RSS sweep refreshes the corpus; runStart marks "fresh from this run".
	runStart, _, _ := corpus.Sweep(ctx, pool, sports, rssLimit, rssPauseMs, logger)

	// Phase 2 — Gemma queue every entity whose corpus changed since runStart
	// (the queried teams + any players/teams co-mentioned via cross-entity linking).
	touched, err := corpus.LoadTouchedEntities(ctx, pool, runStart, sports)
	if err != nil {
		logger.Error("corpus: load touch-set failed", "error", err)
		return
	}
	logger.Info("corpus: gemma queue starting", "candidates", len(touched))

	gemmaStart := time.Now()
	ok, fail, skipped, noCorpus := 0, 0, 0, 0
	for i, e := range touched {
		if recentlySentimentScored(pool, e, skipRecentHours) {
			skipped++
			continue
		}

		lctx, lcancel := context.WithTimeout(ctx, 5*time.Second)
		name, err := corpus.LookupEntityName(lctx, pool, e.EntityType, e.EntityID, e.Sport)
		lcancel()
		if err != nil || name == "" {
			fail++
			logger.Warn("corpus: entity lookup failed",
				"entity_type", e.EntityType, "entity_id", e.EntityID, "sport", e.Sport, "error", err)
			continue
		}

		gctx, cancel := context.WithTimeout(ctx, gemmaTimeout+10*time.Second)
		result, err := gen.Generate(gctx, ml.VibeRequest{
			EntityType:  e.EntityType,
			EntityID:    e.EntityID,
			EntityName:  name,
			Sport:       e.Sport,
			TriggerType: "periodic",
		})
		cancel()

		switch {
		case err != nil:
			fail++
			logger.Warn("corpus: generate failed",
				"sport", e.Sport, "entity", name, "id", e.EntityID, "error", err)
		case result.SkippedNoCorpus:
			noCorpus++
		default:
			ok++
		}

		if (i+1)%25 == 0 {
			logger.Info("corpus: progress",
				"done", i+1, "total", len(touched),
				"ok", ok, "fail", fail, "skipped", skipped, "no_corpus", noCorpus)
		}

		if throttleMs > 0 {
			time.Sleep(time.Duration(throttleMs) * time.Millisecond)
		}
	}

	logger.Info("corpus: complete",
		"ok", ok, "fail", fail, "skipped_recent", skipped, "no_corpus", noCorpus,
		"gemma_elapsed", time.Since(gemmaStart).Round(time.Second),
		"total_elapsed", time.Since(runStart).Round(time.Second))
}

// recentlySentimentScored checks whether this entity already has a sentiment row
// within the debounce window so repeated runs don't duplicate work.
func recentlySentimentScored(pool *pgxpool.Pool, e corpus.Entity, skipRecentHours int) bool {
	if skipRecentHours <= 0 {
		return false
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	var exists bool
	err := pool.QueryRow(ctx, `
		SELECT EXISTS (
			SELECT 1 FROM vibe_scores
			WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
			  AND generated_at > NOW() - ($4 || ' hours')::interval
		)
	`, e.EntityType, e.EntityID, e.Sport, fmt.Sprintf("%d", skipRecentHours)).Scan(&exists)
	if err != nil {
		return false
	}
	return exists
}
