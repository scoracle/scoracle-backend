// pipeline — the RSS ingest sweep for the Scoracle corpus.
//
// Fetch articles via Google News RSS, normalize, write news_articles, and
// enqueue the Editor's read. Every LLM derivation stage lives in the Rust
// Cognition Harness (rust/src), which drains the durable pipeline_work queue.
// This binary performs no model work.
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
	// 100 is one page: Google News RSS returns at most 100 items per request, so this takes
	// what a single search gives and truncates nothing that was ever offered. At 12 the cap
	// never bound on a quiet club (Spezia returns 3) and bound ONLY on the entities with the
	// most news (Arsenal returns 100, kept 12) -- it exclusively starved the biggest stories,
	// which are the ones this product exists to tell.
	//
	// Keeping a number rather than 0 is deliberate: the fetch loop's early exit stops querying
	// alias lanes once the cap is reached, so a busy club is satisfied by its primary query
	// alone while a quiet one still runs every lane looking for the little that exists. 0 would
	// disable that and make every entity pay for all three.
	rssLimit := flag.Int("rss-limit", 100, "[sweep] max articles per entity, one Google News page; 0 = no truncation")
	rssPauseMs := flag.Int("rss-pause-ms", 100, "[sweep] pause between team RSS calls (polite to Google News)")
	logLevel := flag.String("log-level", "info", "debug | info | warn | error; debug adds the per-team fetch funnel")
	flag.Parse()

	logger := slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{Level: parseLogLevel(*logLevel)}))

	_ = godotenv.Load(".env.local")
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

// parseLogLevel maps the -log-level flag onto slog. An unrecognized value falls
// back to Info rather than exiting: a typo in a cron line should not silence the
// sweep entirely.
func parseLogLevel(s string) slog.Level {
	switch strings.ToLower(strings.TrimSpace(s)) {
	case "debug":
		return slog.LevelDebug
	case "warn", "warning":
		return slog.LevelWarn
	case "error":
		return slog.LevelError
	default:
		return slog.LevelInfo
	}
}

func runIngestOnly(pool *pgxpool.Pool, sportArg string, rssLimit, rssPauseMs int, logger *slog.Logger) int {
	sports := []string{"NBA", "NFL", "FOOTBALL"}
	if s := strings.ToLower(strings.TrimSpace(sportArg)); s != "" && s != "all" {
		sports = []string{strings.ToUpper(sportArg)}
	}
	ctx := context.Background()
	ok, fail := corpus.Sweep(ctx, pool, sports, rssLimit, rssPauseMs, logger)
	logger.Info("pipeline ingest: complete", "sports", sports, "rss_ok", ok, "rss_fail", fail)
	if ok == 0 && fail > 0 {
		return 1
	}
	if fail > 0 {
		return 3
	}
	return 0
}
