// vibesynth - DB-only CLI for Sigil maintenance and reconciliation.
//
// Modes:
//
//	nightly | reconcile (default) - bounded reconciliation:
//	  enumerate CURRENT-SEASON rated entities whose current-season Sigil is missing
//	  or stale (an input generation is newer than the Sigil) and enqueue durable
//	  sigil pipeline_work; the Rust cognition daemon drains it (current-season,
//	  hash-gated). It never synthesizes inline and never regenerates an unchanged
//	  Sigil because a schedule fired. DB-only.
//	  go run ./cmd/vibesynth -mode nightly [-limit N]
//
// Env: DATABASE_PRIVATE_URL (see config.go).
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
	"github.com/albapepper/scoracle-data/internal/jobrun"
	"github.com/albapepper/scoracle-data/internal/work"
)

func main() {
	mode := flag.String("mode", "nightly", "nightly | reconcile")
	sport := flag.String("sport", "", "NBA | NFL | FOOTBALL | all")
	limit := flag.Int("limit", 0, "[nightly] cap enqueues per run; 0 = unbounded")
	flag.Parse()

	logger := slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo}))
	_ = godotenv.Load(".env.local")
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

	switch *mode {
	case "nightly", "reconcile":
		os.Exit(runReconcile(pool, *sport, *limit, cfg.DatabaseURL, logger))
	default:
		fmt.Fprintf(os.Stderr, "unknown -mode %q; valid: nightly | reconcile\n", *mode)
		os.Exit(2)
	}
}

type target struct {
	entityType string
	entityID   int
	season     int
	sportName  string
}

// runReconcile enumerates current-season rated entities whose current-season
// Sigil is missing or stale, then enqueues durable sigil work. The Rust
// cognition daemon owns synthesis and hash-gated skips; this command only
// reconciles the queue.
func runReconcile(pool *pgxpool.Pool, sportArg string, limit int, dbURL string, logger *slog.Logger) int {
	sports := resolveSports(sportArg)
	ctx := context.Background()
	start := time.Now()

	run, acquired, err := jobrun.Guard(ctx, pool, dbURL, "vibesynth")
	if err != nil {
		logger.Error("vibesynth reconcile: run-guard failed", "error", err)
		return 1
	}
	if !acquired {
		logger.Warn("vibesynth reconcile: another vibesynth run holds the lock - exiting cleanly")
		return 0
	}
	defer run.Close()

	totalEnq, candidates, enqFail, enumErrs := 0, 0, 0, 0
	for _, sp := range sports {
		cur, cerr := currentSeason(ctx, pool, sp)
		if cerr != nil {
			enumErrs++
			logger.Error("vibesynth reconcile: current_season lookup failed", "sport", sp, "error", cerr)
			continue
		}
		targets, terr := enumStaleSigil(ctx, pool, sp, cur)
		if terr != nil {
			enumErrs++
			logger.Error("vibesynth reconcile: enumerate failed", "sport", sp, "season", cur, "error", terr)
			continue
		}
		candidates += len(targets)
		enq := 0
		for _, t := range targets {
			if limit > 0 && totalEnq >= limit {
				logger.Info("vibesynth reconcile: enqueue limit reached", "limit", limit)
				break
			}
			if eerr := work.Enqueue(ctx, pool, work.Item{
				Stage: work.StageSigil, EntityType: t.entityType, EntityID: t.entityID, Sport: sp,
			}); eerr != nil {
				enqFail++
				logger.Warn("vibesynth reconcile: enqueue failed", "sport", sp, "type", t.entityType, "id", t.entityID, "error", eerr)
				continue
			}
			enq++
			totalEnq++
		}
		logger.Info("vibesynth reconcile: sport done", "sport", sp, "season", cur, "candidates", len(targets), "enqueued", enq)
	}

	status := jobrun.StatusSuccess
	exit := 0
	var runErr error
	switch {
	case enumErrs == len(sports):
		status, exit = jobrun.StatusFailed, 1
		runErr = fmt.Errorf("enumeration failed for all %d sport(s)", len(sports))
	case enqFail > 0:
		status, exit = jobrun.StatusPartial, 3
		runErr = fmt.Errorf("%d sigil enqueue(s) failed", enqFail)
	}
	counts := jobrun.Counts{
		Attempted: candidates,
		Succeeded: totalEnq,
		Failed:    enqFail,
	}
	if ferr := run.Finish(ctx, status, counts, runErr); ferr != nil {
		logger.Warn("vibesynth reconcile: record run failed", "error", ferr)
	}
	logger.Info("vibesynth reconcile: complete", "status", status, "exit", exit,
		"enqueued", totalEnq, "candidates", candidates, "enqueue_fail", enqFail,
		"elapsed", time.Since(start).Round(time.Second))
	return exit
}

// resolveSports expands the -sport flag to the sports to process ("" / "all" => all three).
func resolveSports(sportArg string) []string {
	if s := strings.TrimSpace(sportArg); s != "" && strings.ToLower(s) != "all" {
		return []string{strings.ToUpper(s)}
	}
	return []string{"NBA", "NFL", "FOOTBALL"}
}

// currentSeason resolves the sport's current_season from public.sports.
func currentSeason(ctx context.Context, pool *pgxpool.Pool, sport string) (int, error) {
	qctx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	var s int
	err := pool.QueryRow(qctx, `SELECT current_season FROM public.sports WHERE id = $1`, strings.ToUpper(sport)).Scan(&s)
	return s, err
}

// enumStaleSigil returns the current-season rated entities whose current-season Sigil
// is missing (no season-stamped row) or stale (an input generation is newer than the
// Sigil). It is the reconciliation candidate list.
func enumStaleSigil(ctx context.Context, pool *pgxpool.Pool, sport string, season int) ([]target, error) {
	qctx, cancel := context.WithTimeout(ctx, 60*time.Second)
	defer cancel()
	rows, err := pool.Query(qctx, `
		WITH rated AS (
		    SELECT 'player'::text AS et, player_id AS id FROM player_stats
		     WHERE sport = $1 AND season = $2 AND rating_composite_score IS NOT NULL GROUP BY player_id
		    UNION ALL
		    SELECT 'team'::text, team_id FROM team_stats
		     WHERE sport = $1 AND season = $2 AND rating_composite_score IS NOT NULL GROUP BY team_id
		),
		sig AS (
		    SELECT entity_type AS et, entity_id AS id, max(generated_at) AS g
		    FROM public.sigil_synthesis WHERE sport = $1 AND season = $2 GROUP BY entity_type, entity_id
		),
		st AS (
		    SELECT entity_type AS et, entity_id AS id, max(generated_at) AS g
		    FROM public.stat_summaries WHERE sport = $1 AND season = $2 GROUP BY entity_type, entity_id
		),
		vb AS (
		    SELECT entity_type AS et, entity_id AS id, max(generated_at) AS g
		    FROM public.vibe_scores WHERE sport = $1 GROUP BY entity_type, entity_id
		),
		nw AS (
		    SELECT entity_type AS et, entity_id AS id, max(generated_at) AS g
		    FROM public.news_summaries WHERE sport = $1 GROUP BY entity_type, entity_id
		)
		SELECT r.et, r.id FROM rated r
		LEFT JOIN sig ON sig.et = r.et AND sig.id = r.id
		LEFT JOIN st  ON st.et  = r.et AND st.id  = r.id
		LEFT JOIN vb  ON vb.et  = r.et AND vb.id  = r.id
		LEFT JOIN nw  ON nw.et  = r.et AND nw.id  = r.id
		WHERE sig.g IS NULL
		   OR sig.g < GREATEST(st.g, vb.g, nw.g)
		ORDER BY r.et, r.id`, sport, season)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []target
	for rows.Next() {
		var t target
		if err := rows.Scan(&t.entityType, &t.entityID); err != nil {
			return nil, err
		}
		t.season = season
		t.sportName = sport
		out = append(out, t)
	}
	return out, rows.Err()
}
