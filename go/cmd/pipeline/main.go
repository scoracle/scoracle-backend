// pipeline — the RSS ingest sweep for the Scoracle corpus.
//
// Fetch articles via Google News RSS, normalize, and write news_articles +
// news_article_entities. Every LLM
// derivation stage (scrub, transfers, narratives, vibe, sigil, rating) lives
// in the Rust Cognition Harness (rust/src), which drains the durable
// pipeline_work queue. This binary performs no model work.
//
//	go run ./cmd/pipeline -mode ingest
//	go run ./cmd/pipeline -mode ingest -sport FOOTBALL   # one-sport smoke
//
// Env: DATABASE_PRIVATE_URL (or fallbacks) — see config.go.
package main

import (
	"context"
	"flag"
	"fmt"
	"log/slog"
	"os"
	"strings"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/joho/godotenv"

	"github.com/albapepper/scoracle-data/internal/config"
	"github.com/albapepper/scoracle-data/internal/corpus"
)

func main() {
	mode := flag.String("mode", "ingest", "compatibility flag; only ingest is supported")
	sport := flag.String("sport", "", "NBA | NFL | FOOTBALL | all (default all)")
	rssLimit := flag.Int("rss-limit", 0, "[sweep] max articles per team RSS call; 0 = no truncation")
	rssPauseMs := flag.Int("rss-pause-ms", 100, "[sweep] pause between team RSS calls (polite to Google News)")
	flag.Parse()

	logger := slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo}))

	_ = godotenv.Load(".env.local", ".env")
	cfg, err := config.Load()
	if err != nil {
		logger.Error("config load failed", "error", err)
		os.Exit(1)
	}
	if *mode != "ingest" {
		fmt.Fprintf(os.Stderr, "unknown -mode %q; the only supported mode is ingest\n", *mode)
		os.Exit(2)
	}

	pool, err := pgxpool.New(context.Background(), cfg.DatabaseURL)
	if err != nil {
		logger.Error("db connect failed", "error", err)
		os.Exit(1)
	}
	defer pool.Close()

	os.Exit(runIngestOnly(pool, *sport, *rssLimit, *rssPauseMs, logger))
}

func runIngestOnly(pool *pgxpool.Pool, sportArg string, rssLimit, rssPauseMs int, logger *slog.Logger) int {
	sports := []string{"NBA", "NFL", "FOOTBALL"}
	if s := strings.ToLower(strings.TrimSpace(sportArg)); s != "" && s != "all" {
		sports = []string{strings.ToUpper(sportArg)}
	}
	ctx := context.Background()
	_, _, ok, fail := corpus.Sweep(ctx, pool, sports, rssLimit, rssPauseMs, logger)
	logger.Info("pipeline ingest: complete", "sports", sports, "rss_ok", ok, "rss_fail", fail)
	if ok == 0 && fail > 0 {
		return 1
	}
	if fail > 0 {
		return 3
	}
	return 0
}
