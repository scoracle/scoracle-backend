// transfer — CLI for vetting transfer/trade rumor heat with Gemma.
//
// Two modes:
//
//	single (default) — vet one team's candidate rumors.
//	  go run ./cmd/transfer -team-id 18 -sport FOOTBALL
//
//	corpus — every team in a sport (or all). Each team's co-mention candidates
//	         are heat-scored (deterministic) + Gemma-vetted. Intended for an
//	         external cron, staggered after the vibe corpus run (which does the
//	         RSS sweep that populates news_article_entities).
//	  go run ./cmd/transfer -mode corpus -sport FOOTBALL
//
// Real-time coverage between corpus runs is the news-spike LISTEN/NOTIFY worker
// (internal/listener/transfer_worker.go, Phase 3), not this CLI.
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

// perTeamTimeout bounds one team's full analysis (many Gemma calls, one per
// candidate). Generous — the team's candidate count is the real bound.
const perTeamTimeout = 10 * time.Minute

func main() {
	mode := flag.String("mode", "single", "single | corpus")
	teamID := flag.Int("team-id", 0, "[single] team id")
	sport := flag.String("sport", "", "NBA | NFL | FOOTBALL | all (single requires it; corpus defaults to all)")
	trigger := flag.String("trigger", "manual", "manual | periodic | news_spike")
	minArticles := flag.Int("min-articles", 2, "candidate pre-filter: min distinct co-mention articles (14d)")
	throttleMs := flag.Int("throttle-ms", 0, "[corpus] pause N ms between teams")
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
	gen := ml.NewTransferGenerator(pool, ollama)

	switch *mode {
	case "single":
		runSingle(pool, gen, *teamID, *sport, *trigger, *minArticles, logger)
	case "corpus":
		runCorpus(pool, gen, *sport, *trigger, *minArticles, *throttleMs, logger)
	default:
		fmt.Fprintf(os.Stderr, "unknown -mode %q; valid: single | corpus\n", *mode)
		os.Exit(2)
	}
}

func runSingle(pool *pgxpool.Pool, gen *ml.TransferGenerator, teamID int, sport, trigger string, minArticles int, logger *slog.Logger) {
	if teamID <= 0 || sport == "" {
		fmt.Fprintln(os.Stderr, "-team-id and -sport are required in single mode")
		os.Exit(2)
	}
	sportUpper := strings.ToUpper(sport)
	ctx, cancel := context.WithTimeout(context.Background(), perTeamTimeout)
	defer cancel()

	name, err := lookupTeamName(ctx, pool, teamID, sportUpper)
	if err != nil {
		logger.Error("team lookup failed", "error", err)
		os.Exit(1)
	}
	res, err := gen.GenerateForTeam(ctx, ml.TransferRequest{
		TeamID: teamID, TeamName: name, Sport: sportUpper, TriggerType: trigger, MinArticles: minArticles,
	})
	if err != nil {
		logger.Error("transfer generate failed", "error", err)
		os.Exit(1)
	}
	fmt.Printf("\n--- %s rumors for %s (%s %d) ---\n",
		map[bool]string{true: "Trade", false: "Transfer"}[sportUpper == "NBA" || sportUpper == "NFL"],
		name, sportUpper, teamID)
	fmt.Printf("candidates=%d  rumors=%d  cleared=%d  unknown=%d  skipped=%d  errored=%d  duration=%s\n",
		res.Candidates, res.Rumors, res.Cleared, res.Unknown, res.Skipped, res.Errored,
		res.Duration.Round(100*time.Millisecond))
}

func runCorpus(pool *pgxpool.Pool, gen *ml.TransferGenerator, sport, trigger string, minArticles, throttleMs int, logger *slog.Logger) {
	sports := []string{"NBA", "NFL", "FOOTBALL"}
	if sport != "" && strings.ToLower(sport) != "all" {
		sports = []string{strings.ToUpper(sport)}
	}
	ctx := context.Background()
	for _, sp := range sports {
		teams, err := loadTeams(ctx, pool, sp)
		if err != nil {
			logger.Error("load teams failed", "sport", sp, "error", err)
			continue
		}
		logger.Info("transfer corpus: sport start", "sport", sp, "teams", len(teams))
		var totRumors, totCleared, totUnknown int
		for _, t := range teams {
			tctx, cancel := context.WithTimeout(ctx, perTeamTimeout)
			res, err := gen.GenerateForTeam(tctx, ml.TransferRequest{
				TeamID: t.id, TeamName: t.name, Sport: sp, TriggerType: trigger, MinArticles: minArticles,
			})
			cancel()
			if err != nil {
				logger.Warn("team failed", "team", t.name, "error", err)
				continue
			}
			if res.Candidates > 0 {
				logger.Info("team done", "team", t.name, "candidates", res.Candidates,
					"rumors", res.Rumors, "cleared", res.Cleared, "unknown", res.Unknown)
			}
			totRumors += res.Rumors
			totCleared += res.Cleared
			totUnknown += res.Unknown
			if throttleMs > 0 {
				time.Sleep(time.Duration(throttleMs) * time.Millisecond)
			}
		}
		logger.Info("transfer corpus: sport done", "sport", sp, "rumors", totRumors, "cleared", totCleared, "unknown", totUnknown)
	}
}

type teamRow struct {
	id   int
	name string
}

func loadTeams(ctx context.Context, pool *pgxpool.Pool, sport string) ([]teamRow, error) {
	rows, err := pool.Query(ctx, `SELECT id, name FROM teams WHERE sport = $1 ORDER BY id`, sport)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var out []teamRow
	for rows.Next() {
		var t teamRow
		if err := rows.Scan(&t.id, &t.name); err != nil {
			return nil, err
		}
		out = append(out, t)
	}
	return out, rows.Err()
}

func lookupTeamName(ctx context.Context, pool *pgxpool.Pool, teamID int, sport string) (string, error) {
	var name string
	err := pool.QueryRow(ctx, `SELECT name FROM teams WHERE id = $1 AND sport = $2`, teamID, sport).Scan(&name)
	return name, err
}
