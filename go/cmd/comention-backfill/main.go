// comention-backfill — one-shot maintenance for the co-mention proximity gate
// (migration 033). Recomputes news_article_entities.title_pos for existing rows
// from the stored article title, using the SAME matcher as the news write path,
// so historical co-mentions get the position the proximity gate needs. Idempotent.
//
//	go run ./cmd/comention-backfill              # all sports
//	go run ./cmd/comention-backfill -sport FOOTBALL
//
// Env: DATABASE_PRIVATE_URL (or fallbacks) — see config.go.
package main

import (
	"context"
	"flag"
	"log/slog"
	"os"
	"strings"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/joho/godotenv"

	"github.com/albapepper/scoracle-data/internal/config"
	"github.com/albapepper/scoracle-data/internal/thirdparty"
)

func main() {
	sport := flag.String("sport", "all", "NBA | NFL | FOOTBALL | all")
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

	svc := thirdparty.NewNewsService(pool, logger)

	sports := []string{"NBA", "NFL", "FOOTBALL"}
	if *sport != "" && strings.ToLower(*sport) != "all" {
		sports = []string{strings.ToUpper(*sport)}
	}

	total := 0
	for _, sp := range sports {
		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Minute)
		start := time.Now()
		n, err := svc.BackfillTitlePositions(ctx, sp)
		cancel()
		if err != nil {
			logger.Error("backfill failed", "sport", sp, "updated", n, "error", err)
			os.Exit(1)
		}
		logger.Info("backfill done", "sport", sp, "rows_updated", n, "duration", time.Since(start).Round(time.Millisecond))
		total += n
	}
	logger.Info("backfill complete", "total_rows_updated", total)
}
