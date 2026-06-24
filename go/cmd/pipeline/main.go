// pipeline — the once-daily compile → scrub → derive → reveal orchestrator.
//
// FIRST-GPT-AUDIT Session 8 turned this from an in-process-watermark chain into an
// ordered, durable, crash-recoverable pipeline. The handoff between stages is the
// pipeline_work queue (migration 102, go/internal/work), not the old runStart
// watermark — so a kill mid-run resumes from the database, and a re-run with no fresh
// input does no Gemma work:
//
//	requeue stale → sweep → scrub(fresh batch) → drain transfers → narratives → vibe → sigil
//
//	Stage 0  RSS sweep returns the articles that gained a FRESH link this run.
//	Stage 1  those exact articles are scrubbed IN-RUN (Gemma ID-gate, vetted=TRUE).
//	         The scrub UPDATE fires the migration-103 trigger, which enqueues each
//	         vetted entity's derive work (narratives + vibe; teams also transfers) —
//	         the trigger is the SOLE enqueuer, so there is no Go enqueue here.
//	Stages   the shared derive.Drainer drains the queue in declared order (Claim → run
//	         → Complete/Fail); a completed vibe enqueues its sigil convergence.
//
// FIRST-GPT-AUDIT Session 9 moved the drain itself into internal/derive so this
// nightly run and the real-time in-API worker share one implementation, and replaced
// the in-run Go enqueue with the durable vetted-transition trigger (closing S8's
// residual scrub→enqueue window). The async maintenance scrub ticker remains
// backlog/repair only.
//
//	go run ./cmd/pipeline -mode corpus
//	go run ./cmd/pipeline -mode corpus -sport FOOTBALL   # one-sport smoke
//
// Env: DATABASE_PRIVATE_URL (or fallbacks) + OLLAMA_* (see config.go).
package main

import (
	"context"
	"flag"
	"fmt"
	"log/slog"
	"os"
	"sort"
	"strings"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/joho/godotenv"

	"github.com/albapepper/scoracle-data/internal/config"
	"github.com/albapepper/scoracle-data/internal/corpus"
	"github.com/albapepper/scoracle-data/internal/derive"
	"github.com/albapepper/scoracle-data/internal/jobrun"
	"github.com/albapepper/scoracle-data/internal/ml"
	"github.com/albapepper/scoracle-data/internal/work"
)

func main() {
	mode := flag.String("mode", "corpus", "corpus (the only mode)")
	sport := flag.String("sport", "", "NBA | NFL | FOOTBALL | all (default all)")
	minArticles := flag.Int("min-articles", 2, "[transfers] candidate pre-filter: min distinct co-mention articles (14d)")
	rssLimit := flag.Int("rss-limit", 10, "[sweep] articles per team RSS call")
	rssPauseMs := flag.Int("rss-pause-ms", 100, "[sweep] pause between team RSS calls (polite to Google News)")
	scrubLimit := flag.Int("scrub-limit", 0, "[scrub] cap freshly-ingested articles scrubbed this run; 0 = no cap (smoke runs)")
	transferThrottleMs := flag.Int("transfer-throttle-ms", 0, "[transfers] pause between teams")
	narrateThrottleMs := flag.Int("narrate-throttle-ms", 0, "[narratives] pause between entities")
	vibeThrottleMs := flag.Int("vibe-throttle-ms", 0, "[vibe] pause between entities")
	synthThrottleMs := flag.Int("synth-throttle-ms", 0, "[sigil] pause between entities")
	limit := flag.Int("limit", 0, "cap items processed per drained stage; 0 = no cap (smoke runs)")
	flag.Parse()

	logger := slog.New(slog.NewTextHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo}))

	_ = godotenv.Load(".env.local", ".env")
	cfg, err := config.Load()
	if err != nil {
		logger.Error("config load failed", "error", err)
		os.Exit(1)
	}
	if *mode != "corpus" {
		fmt.Fprintf(os.Stderr, "unknown -mode %q; valid: corpus\n", *mode)
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

	os.Exit(runCorpus(pool, opts{
		scrubber:         ml.NewNewsScrubber(pool, ollama),
		transferGen:      ml.NewTransferGenerator(pool, ollama),
		narrator:         ml.NewNewsNarrator(pool, ollama),
		vibeGen:          ml.NewVibeGenerator(pool, ollama),
		synthGen:         ml.NewSigilGenerator(pool, ollama),
		gemmaTimeout:     cfg.OllamaTimeout,
		sportArg:         *sport,
		minArticles:      *minArticles,
		rssLimit:         *rssLimit,
		rssPauseMs:       *rssPauseMs,
		scrubLimit:       *scrubLimit,
		transferThrottle: *transferThrottleMs,
		narrateThrottle:  *narrateThrottleMs,
		vibeThrottle:     *vibeThrottleMs,
		synthThrottle:    *synthThrottleMs,
		limit:            *limit,
	}, cfg.DatabaseURL, logger))
}

type opts struct {
	scrubber         *ml.NewsScrubber
	transferGen      *ml.TransferGenerator
	narrator         *ml.NewsNarrator
	vibeGen          *ml.VibeGenerator
	synthGen         *ml.SigilGenerator
	gemmaTimeout     time.Duration
	sportArg         string
	minArticles      int
	rssLimit         int
	rssPauseMs       int
	scrubLimit       int
	transferThrottle int
	narrateThrottle  int
	vibeThrottle     int
	synthThrottle    int
	limit            int
}

// runCorpus runs the full daily pipeline and returns a process exit code
// (FIRST-GPT-AUDIT Session 13):
//
//	0  success (or a clean skip because another run holds the advisory lock)
//	1  a derive stage failed wholesale (systemic, e.g. Ollama down), or
//	   dead-lettered work remains after retries — an operator must look
//	3  partial — some per-entity items failed but are retryable (queue will retry)
//
// The run is recorded in pipeline_runs throughout, so cron can tell a partial
// failure from a clean run without reading the logs.
func runCorpus(pool *pgxpool.Pool, o opts, dbURL string, logger *slog.Logger) int {
	sports := []string{"NBA", "NFL", "FOOTBALL"}
	if s := strings.ToLower(strings.TrimSpace(o.sportArg)); s != "" && s != "all" {
		sports = []string{strings.ToUpper(o.sportArg)}
	}
	ctx := context.Background()
	start := time.Now()

	// Overlap guard + durable run record. A second pipeline run (or a manual one
	// racing the nightly cron) finds the lock held and exits cleanly.
	run, acquired, err := jobrun.Guard(ctx, pool, dbURL, "pipeline")
	if err != nil {
		logger.Error("pipeline: run-guard failed", "error", err)
		return 1
	}
	if !acquired {
		logger.Warn("pipeline: another pipeline run holds the lock — exiting cleanly")
		return 0
	}
	defer run.Close()

	logger.Info("pipeline: starting", "sports", sports)

	// Recover work abandoned by a crashed prior run before claiming anything new.
	if n, err := work.RequeueStale(ctx, pool, derive.StaleLease); err != nil {
		logger.Warn("pipeline: requeue stale failed", "error", err)
	} else if n > 0 {
		logger.Info("pipeline: recovered stale work", "rows", n)
	}

	// Stage 0 — RSS sweep. affected = articles that gained a FRESH link this run.
	// RSS failures are soft (Google throttling; the next run + maintenance ticker
	// recover), so they are logged but do not by themselves fail the run.
	_, affected, sweepOK, sweepFail := corpus.Sweep(ctx, pool, sports, o.rssLimit, o.rssPauseMs, logger)
	logger.Info("pipeline: sweep done", "rss_ok", sweepOK, "rss_fail", sweepFail, "fresh_articles", len(affected))

	// Stage 1 — scrub the fresh batch IN-RUN. The scrub UPDATE fires the migration-103
	// trigger, which durably enqueues each vetted entity's derive work. Scrub failures
	// are fail-closed (nothing vetted ⇒ nothing derived) and self-heal via the backlog
	// re-scrub, so they count toward "partial" but never a hard failure.
	scrubbed, scrubFailed := scrubFresh(ctx, pool, o, affected, logger)

	// Stages 2–5 — drain the durable queue in declared order via the shared drainer.
	drainer := &derive.Drainer{
		Pool:             pool,
		TransferGen:      o.transferGen,
		Narrator:         o.narrator,
		VibeGen:          o.vibeGen,
		SynthGen:         o.synthGen,
		GemmaTimeout:     o.gemmaTimeout,
		MinArticles:      o.minArticles,
		TransferThrottle: o.transferThrottle,
		NarrateThrottle:  o.narrateThrottle,
		VibeThrottle:     o.vibeThrottle,
		SynthThrottle:    o.synthThrottle,
		Limit:            o.limit,
		Logger:           logger,
	}
	res := drainer.DrainAll(ctx)
	drainOK, drainFail, _ := res.Totals()

	logRemaining(ctx, pool, logger)

	// Dead-letters: work parked past the retry cap. Any present means real work is
	// permanently stuck and an operator must intervene (cmd/work dead-letters).
	dead, derr := work.DeadLetters(ctx, pool)
	if derr != nil {
		logger.Warn("pipeline: dead-letter query failed", "error", derr)
	}

	// Decide the outcome.
	status := jobrun.StatusSuccess
	exit := 0
	var runErr error
	switch {
	case res.WholeStageFailure() != "":
		status, exit = jobrun.StatusFailed, 1
		runErr = fmt.Errorf("derive stage %q failed entirely (0 succeeded, %d failed)", res.WholeStageFailure(), drainFail)
	case len(dead) > 0:
		status, exit = jobrun.StatusFailed, 1
		runErr = fmt.Errorf("%d dead-lettered work item(s) remain after retries", len(dead))
	case drainFail > 0 || scrubFailed > 0:
		status, exit = jobrun.StatusPartial, 3
		runErr = fmt.Errorf("partial: scrub_fail=%d drain_fail=%d (retryable)", scrubFailed, drainFail)
	}

	counts := jobrun.Counts{
		Attempted: scrubbed + scrubFailed + drainOK + drainFail,
		Succeeded: scrubbed + drainOK,
		Failed:    scrubFailed + drainFail,
	}
	if ferr := run.Finish(ctx, status, counts, runErr); ferr != nil {
		logger.Warn("pipeline: record run failed", "error", ferr)
	}
	logger.Info("pipeline: complete", "status", status, "exit", exit,
		"scrubbed", scrubbed, "drain_ok", drainOK, "drain_fail", drainFail,
		"dead_letters", len(dead), "elapsed", time.Since(start).Round(time.Second))
	return exit
}

// scrubFresh scrubs the freshly-ingested articles (Gemma ID-gate, persisting
// vetted=TRUE + scrubbed_at). The persist is one UPDATE per article, and that UPDATE
// fires the migration-103 trigger that enqueues the article's vetted entities into
// pipeline_work — so the enqueue is a durable side-effect of the scrub commit (no Go
// enqueue, closing S8's residual scrub→enqueue window). A re-run with no fresh
// articles scrubs nothing and therefore enqueues nothing.
func scrubFresh(ctx context.Context, pool *pgxpool.Pool, o opts, affected map[int64]string, logger *slog.Logger) (scrubbed, failed int) {
	if len(affected) == 0 {
		logger.Info("pipeline/scrub: no fresh articles; nothing to derive")
		return 0, 0
	}

	ids := make([]int64, 0, len(affected))
	for id := range affected {
		ids = append(ids, id)
	}
	sort.Slice(ids, func(i, j int) bool { return ids[i] < ids[j] }) // deterministic
	if o.scrubLimit > 0 && len(ids) > o.scrubLimit {
		logger.Info("pipeline/scrub: capping batch", "from", len(ids), "to", o.scrubLimit)
		ids = ids[:o.scrubLimit]
	}

	for _, id := range ids {
		if ctx.Err() != nil {
			break
		}
		sctx, cancel := context.WithTimeout(ctx, o.gemmaTimeout+10*time.Second)
		_, err := o.scrubber.ScrubArticle(sctx, id, affected[id], false /* persist */)
		cancel()
		if err != nil {
			failed++
			logger.Warn("pipeline/scrub: article failed", "article_id", id, "sport", affected[id], "error", err)
			continue
		}
		scrubbed++
	}
	logger.Info("pipeline/scrub: done", "scrubbed", scrubbed, "failed", failed)
	return scrubbed, failed
}

// logRemaining prints the pipeline_work backlog after the run — the operator's
// "what's still pending/running/failed?" snapshot (anything left is real-time or
// dead-lettered work for the in-API worker to drain).
func logRemaining(ctx context.Context, pool *pgxpool.Pool, logger *slog.Logger) {
	counts, err := work.Counts(ctx, pool)
	if err != nil {
		logger.Warn("pipeline: work counts failed", "error", err)
		return
	}
	for _, c := range counts {
		logger.Info("pipeline: work remaining",
			"stage", c.Stage, "status", c.Status, "n", c.Count, "max_attempts", c.MaxAttempts)
	}
}
