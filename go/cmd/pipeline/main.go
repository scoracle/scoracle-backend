// pipeline — the daily sweep for the Scoracle corpus: news in, stats in.
//
// -mode ingest: fetch articles via Google News RSS, normalize, write
// news_articles, and enqueue the Editor's read. Every LLM derivation stage
// lives in the Rust Cognition Harness (rust/src), which drains the durable
// pipeline_work queue. This binary performs no model work.
//
// -mode data: the gap-driven stats importer (internal/dataimport,
// PLAN-weekly-fantasy-rail.md). Refresh schedules + rosters from the free
// feeds, ask the DB which finished fixtures have no stat rows, promote exactly
// those through finalize_fixture(). The feed detects the event; a miss is
// still in the gap tomorrow. NFL (nflverse) for now; NBA and FPL follow.
//
//	go run ./cmd/pipeline -mode ingest
//	go run ./cmd/pipeline -mode ingest -sport FOOTBALL   # one-sport smoke
//	go run ./cmd/pipeline -mode data
//	go run ./cmd/pipeline -mode data -season 2025        # one-season backfill
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
	"github.com/albapepper/scoracle-data/internal/dataimport"
	"github.com/albapepper/scoracle-data/internal/jobrun"
)

func main() {
	mode := flag.String("mode", "ingest", "ingest (RSS sweep) | data (stats gap-fill) | headshots (provider-image repair)")
	sport := flag.String("sport", "", "NBA | NFL | FOOTBALL | all (default all)")
	season := flag.Int("season", 0, "[data/headshots] one season only (0 = current + next / all stored NFL seasons)")
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

	// Production commands normally run from the repository root, while `go run
	// ./cmd/pipeline` is conventionally invoked from the Go module directory.
	// Load the same sole env file from either location so an operational repair
	// does not silently lose its database connection merely because of cwd.
	for _, path := range []string{".env.local", "../.env.local"} {
		_ = godotenv.Load(path)
	}
	cfg, err := config.Load()
	if err != nil {
		logger.Error("config load failed", "error", err)
		os.Exit(1)
	}
	if *mode != "ingest" && *mode != "data" && *mode != "headshots" {
		fmt.Fprintf(os.Stderr, "unknown -mode %q; supported modes are ingest, data, and headshots\n", *mode)
		os.Exit(2)
	}

	pool, err := pgxpool.New(context.Background(), cfg.DatabaseURL)
	if err != nil {
		logger.Error("db connect failed", "error", err)
		os.Exit(1)
	}
	defer pool.Close()

	if *mode == "data" {
		os.Exit(runData(pool, cfg.DatabaseURL, *season, logger))
	}
	if *mode == "headshots" {
		os.Exit(runHeadshots(pool, cfg.DatabaseURL, *sport, *season, logger))
	}
	os.Exit(runIngestOnly(pool, cfg.DatabaseURL, *sport, *rssLimit, *rssPauseMs, logger))
}

// runHeadshots repairs the two US-league image columns from stable provider
// identities. It intentionally has its own guard: this can read years of NFL
// data and must not overlap a second repair, while the daily stats rail stays
// free to run.
func runHeadshots(pool *pgxpool.Pool, dbURL, sport string, season int, logger *slog.Logger) int {
	ctx := context.Background()
	run, acquired, err := jobrun.Guard(ctx, pool, dbURL, "pipeline-headshots")
	if err != nil {
		logger.Error("pipeline headshots: run-guard failed", "error", err)
		return 1
	}
	if !acquired {
		logger.Warn("pipeline headshots: another repair holds the lock - exiting cleanly")
		return 0
	}
	defer run.Close()

	want := strings.ToUpper(strings.TrimSpace(sport))
	if want == "" || want == "ALL" {
		want = "ALL"
	}
	if want != "ALL" && want != "NFL" && want != "NBA" {
		logger.Error("pipeline headshots: sport must be NBA, NFL, or all", "sport", sport)
		return 2
	}

	var attempted, changed, failed int
	if want == "ALL" || want == "NBA" {
		f, err := dataimport.BackfillNBAHeadshots(ctx, pool)
		if err != nil {
			logger.Error("pipeline headshots: NBA repair failed", "error", err)
			failed++
		} else {
			changed += f.Updated
			logger.Info("pipeline headshots: NBA repair complete", "updated", f.Updated)
		}
	}
	if want == "ALL" || want == "NFL" {
		f, err := dataimport.BackfillNFLHeadshots(ctx, pool, season, logger)
		attempted += f.SourceRows
		changed += f.Updated
		if err != nil {
			logger.Error("pipeline headshots: NFL repair failed", "error", err, "updated", f.Updated, "cleared", f.Cleared, "unbound", f.Unbound)
			failed++
		} else {
			logger.Info("pipeline headshots: NFL repair complete", "source_rows", f.SourceRows, "updated", f.Updated, "cleared", f.Cleared, "unbound", f.Unbound)
		}
	}
	status, exit := jobrun.StatusSuccess, 0
	if failed > 0 {
		status, exit = jobrun.StatusFailed, 1
	}
	if err := run.Finish(ctx, status, jobrun.Counts{Attempted: attempted, Succeeded: changed, Failed: failed}, nil); err != nil {
		logger.Warn("pipeline headshots: record run failed", "error", err)
	}
	return exit
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

// runData is the stats gap-fill under its own jobrun guard ("pipeline-data"),
// separate from the RSS sweep's lock — the two steps run back to back in cron
// but neither should ever block the other's retry.
func runData(pool *pgxpool.Pool, dbURL string, season int, logger *slog.Logger) int {
	ctx := context.Background()
	run, acquired, err := jobrun.Guard(ctx, pool, dbURL, "pipeline-data")
	if err != nil {
		logger.Error("pipeline data: run-guard failed", "error", err)
		return 1
	}
	if !acquired {
		logger.Warn("pipeline data: another data run holds the lock - exiting cleanly")
		return 0
	}
	defer run.Close()

	funnel, runErr := dataimport.RunNFL(ctx, pool, season, logger)

	// The football arm (the FPL remap, 2026-09-06). One sport's trouble must
	// not block the other's fill: FPL failure downgrades the run, never aborts
	// it, and its funnel folds into the same partial/retry accounting.
	fplFunnel, fplErr := dataimport.RunFPL(ctx, pool, logger)
	if fplErr != nil {
		logger.Error("pipeline data: fpl run failed", "error", fplErr)
		if runErr == nil {
			runErr = fplErr
		}
	}
	funnel.Gaps += fplFunnel.Gaps
	funnel.GapsFilled += fplFunnel.GapsFilled
	funnel.GapsWaiting += fplFunnel.GapsWaiting
	funnel.GapsFailed += fplFunnel.GapsFailed
	funnel.EventPlayers += fplFunnel.EventPlayers
	funnel.EventTeams += fplFunnel.EventTeams
	funnel.PlayersUnmatched += fplFunnel.PlayersUnmatched
	funnel.TeamsUnmatched += fplFunnel.TeamsUnmatched

	status, exit := jobrun.StatusSuccess, 0
	switch {
	case runErr != nil:
		status, exit = jobrun.StatusFailed, 1
		logger.Error("pipeline data: run failed", "error", runErr)
	case funnel.GapsFailed > 0 || funnel.PlayersUnmatched > 0 || funnel.TeamsUnmatched > 0:
		// Retryable next run by construction: everything skipped is still in the gap.
		status, exit = jobrun.StatusPartial, 3
		runErr = fmt.Errorf("gaps_failed=%d players_unmatched=%d teams_unmatched=%d (gap query re-offers next run)",
			funnel.GapsFailed, funnel.PlayersUnmatched, funnel.TeamsUnmatched)
	}
	counts := jobrun.Counts{
		Attempted: funnel.Gaps,
		Succeeded: funnel.GapsFilled,
		Skipped:   funnel.GapsWaiting,
		Failed:    funnel.GapsFailed,
	}
	if ferr := run.Finish(ctx, status, counts, runErr); ferr != nil {
		logger.Warn("pipeline data: record run failed", "error", ferr)
	}
	return exit
}

func runIngestOnly(pool *pgxpool.Pool, dbURL, sportArg string, rssLimit, rssPauseMs int, logger *slog.Logger) int {
	sports := []string{"NBA", "NFL", "FOOTBALL"}
	if s := strings.ToLower(strings.TrimSpace(sportArg)); s != "" && s != "all" {
		sports = []string{strings.ToUpper(sportArg)}
	}
	ctx := context.Background()

	// The durable run record + overlap lock (jobrun) — dropped by mistake in the
	// lean sweep (f4c9556), which left pipeline_runs_latest showing the LAST
	// pre-sweep run (Jun 28) forever while ingest ran nightly. The watchdog reads
	// data freshness so it survived the gap, but the run ledger is the place a
	// human asks first; it must not lie.
	run, acquired, err := jobrun.Guard(ctx, pool, dbURL, "pipeline")
	if err != nil {
		logger.Error("pipeline ingest: run-guard failed", "error", err)
		return 1
	}
	if !acquired {
		logger.Warn("pipeline ingest: another pipeline run holds the lock - exiting cleanly")
		return 0
	}
	defer run.Close()

	ok, fail := corpus.Sweep(ctx, pool, sports, rssLimit, rssPauseMs, logger)
	logger.Info("pipeline ingest: complete", "sports", sports, "rss_ok", ok, "rss_fail", fail)

	status, exit := jobrun.StatusSuccess, 0
	var runErr error
	switch {
	case ok == 0 && fail > 0:
		status, exit = jobrun.StatusFailed, 1
		runErr = fmt.Errorf("every RSS sweep failed (%d)", fail)
	case fail > 0:
		status, exit = jobrun.StatusPartial, 3
		runErr = fmt.Errorf("%d RSS sweep(s) failed (retryable next run)", fail)
	}
	if ferr := run.Finish(ctx, status, jobrun.Counts{Attempted: ok + fail, Succeeded: ok, Failed: fail}, runErr); ferr != nil {
		logger.Warn("pipeline ingest: record run failed", "error", ferr)
	}
	return exit
}
