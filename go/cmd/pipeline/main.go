// pipeline — the once-daily staged Gemma orchestrator.
//
// Runs the full chain IN ORDER, in one process so the cross-stage "which entities
// were touched this run" handoff (the in-process runStart watermark) holds:
//
//	sweep → transfers → narratives → sentiment → synthesis
//	  0        1            2            3            4
//
// Stage 0 RSS-refreshes the corpus (the async scrub ticker vets it). Stage 1 vets
// transfer heat off the scrubbed corpus (all teams). Stages 2/3/4 process every
// entity whose corpus changed this run: narratives first (grounded on the fresh
// stage-1 heat — see ml/news_narratives.go), then sentiment (the internal ingredient),
// then vibe synthesis (the three-pillar holistic score the product shows).
// Per-stage batches; the DB is the handoff between stages.
//
//	go run ./cmd/pipeline -mode corpus
//	go run ./cmd/pipeline -mode corpus -sport FOOTBALL   # one-sport smoke
//
// Real-time coverage between daily runs is the in-API LISTEN/NOTIFY workers
// (news-volume → narrate+vibe; transfer_trigger → transfer). pipeline_stats is
// written by the maintenance ticker, not here.
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

// perTeamTimeout bounds one team's transfer analysis (many Gemma calls, one per
// candidate). Generous — the candidate count is the real bound.
const perTeamTimeout = 10 * time.Minute

func main() {
	mode := flag.String("mode", "corpus", "corpus (the only mode)")
	sport := flag.String("sport", "", "NBA | NFL | FOOTBALL | all (default all)")
	minArticles := flag.Int("min-articles", 2, "[transfers] candidate pre-filter: min distinct co-mention articles (14d)")
	rssLimit := flag.Int("rss-limit", 10, "[sweep] articles per team RSS call")
	rssPauseMs := flag.Int("rss-pause-ms", 100, "[sweep] pause between team RSS calls (polite to Google News)")
	transferThrottleMs := flag.Int("transfer-throttle-ms", 0, "[stage 1] pause between teams")
	narrateThrottleMs := flag.Int("narrate-throttle-ms", 0, "[stage 2] pause between entities")
	vibeThrottleMs := flag.Int("vibe-throttle-ms", 0, "[stage 3] pause between entities")
	synthThrottleMs := flag.Int("synth-throttle-ms", 0, "[stage 4] pause between entities")
	narrateSkipHours := flag.Int("narrate-skip-recent-hours", 10, "[stage 2] skip entities narrated within this window")
	vibeSkipHours := flag.Int("vibe-skip-recent-hours", 10, "[stage 3] skip entities vibed within this window")
	synthSkipHours := flag.Int("synth-skip-recent-hours", 24, "[stage 4] skip entities synthesized within this window")
	limit := flag.Int("limit", 0, "cap teams (stage 1) and touched entities (stages 2/3/4); 0 = no cap. For smoke runs.")
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

	transferGen := ml.NewTransferGenerator(pool, ollama)
	narrator := ml.NewNewsNarrator(pool, ollama)
	vibeGen := ml.NewVibeGenerator(pool, ollama)
	synthGen := ml.NewSigilGenerator(pool, ollama)

	runCorpus(pool, opts{
		transferGen:      transferGen,
		narrator:         narrator,
		vibeGen:          vibeGen,
		synthGen:         synthGen,
		gemmaTimeout:     cfg.OllamaTimeout,
		sportArg:         *sport,
		minArticles:      *minArticles,
		rssLimit:         *rssLimit,
		rssPauseMs:       *rssPauseMs,
		transferThrottle: *transferThrottleMs,
		narrateThrottle:  *narrateThrottleMs,
		vibeThrottle:     *vibeThrottleMs,
		synthThrottle:    *synthThrottleMs,
		narrateSkip:      time.Duration(*narrateSkipHours) * time.Hour,
		vibeSkip:         time.Duration(*vibeSkipHours) * time.Hour,
		synthSkip:        time.Duration(*synthSkipHours) * time.Hour,
		limit:            *limit,
	}, logger)
}

type opts struct {
	transferGen      *ml.TransferGenerator
	narrator         *ml.NewsNarrator
	vibeGen          *ml.VibeGenerator
	synthGen         *ml.SigilGenerator
	gemmaTimeout     time.Duration
	sportArg         string
	minArticles      int
	rssLimit         int
	rssPauseMs       int
	transferThrottle int
	narrateThrottle  int
	vibeThrottle     int
	synthThrottle    int
	narrateSkip      time.Duration
	vibeSkip         time.Duration
	synthSkip        time.Duration
	limit            int
}

func runCorpus(pool *pgxpool.Pool, o opts, logger *slog.Logger) {
	sports := []string{"NBA", "NFL", "FOOTBALL"}
	if s := strings.ToLower(strings.TrimSpace(o.sportArg)); s != "" && s != "all" {
		sports = []string{strings.ToUpper(o.sportArg)}
	}
	ctx := context.Background()
	start := time.Now()
	logger.Info("pipeline: starting", "sports", sports)

	// Stage 0 — RSS sweep refreshes the corpus; runStart bounds "fresh this run".
	runStart, _, _ := corpus.Sweep(ctx, pool, sports, o.rssLimit, o.rssPauseMs, logger)

	// Stage 1 — transfer heat + Gemma vetting off the scrubbed corpus (all teams).
	runTransfers(ctx, pool, o, sports, logger)

	// Stages 2/3 operate on the entities whose corpus changed this run.
	touched, err := corpus.LoadTouchedEntities(ctx, pool, runStart, sports)
	if err != nil {
		logger.Error("pipeline: load touch-set failed", "error", err)
		return
	}
	if o.limit > 0 && len(touched) > o.limit {
		logger.Info("pipeline: capping touched entities", "from", len(touched), "to", o.limit)
		touched = touched[:o.limit]
	}
	logger.Info("pipeline: touched entities", "count", len(touched))

	// Stage 2 — narratives (grounded on the fresh stage-1 transfer heat).
	runNarratives(ctx, pool, o, touched, logger)
	// Stage 3 — sentiment from the fresh narratives + heat.
	runVibe(ctx, pool, o, touched, logger)
	// Stage 4 — vibe synthesis (three-pillar holistic score — the LAST stage).
	runSynthesis(ctx, pool, o, touched, logger)

	logger.Info("pipeline: complete", "elapsed", time.Since(start).Round(time.Second))
}

// Stage 1 — transfers across every team (mirrors cmd/transfer corpus mode).
func runTransfers(ctx context.Context, pool *pgxpool.Pool, o opts, sports []string, logger *slog.Logger) {
	done := 0
	for _, sp := range sports {
		teams, err := corpus.LoadTeams(ctx, pool, sp)
		if err != nil {
			logger.Error("pipeline/transfers: load teams failed", "sport", sp, "error", err)
			continue
		}
		logger.Info("pipeline/transfers: sport start", "sport", sp, "teams", len(teams))
		var rumors, cleared int
		for _, t := range teams {
			if o.limit > 0 && done >= o.limit {
				break
			}
			done++
			tctx, cancel := context.WithTimeout(ctx, perTeamTimeout)
			res, err := o.transferGen.GenerateForTeam(tctx, ml.TransferRequest{
				TeamID: t.ID, TeamName: t.Name, Sport: sp, TriggerType: "periodic", MinArticles: o.minArticles,
			})
			cancel()
			if err != nil {
				logger.Warn("pipeline/transfers: team failed", "team", t.Name, "error", err)
				continue
			}
			rumors += res.Rumors
			cleared += res.Cleared
			if o.transferThrottle > 0 {
				time.Sleep(time.Duration(o.transferThrottle) * time.Millisecond)
			}
		}
		logger.Info("pipeline/transfers: sport done", "sport", sp, "rumors", rumors, "cleared", cleared)
	}
}

// Stage 2 — narratives for every touched entity.
func runNarratives(ctx context.Context, pool *pgxpool.Pool, o opts, touched []corpus.Entity, logger *slog.Logger) {
	ok, fail, skipped, noCorpus := 0, 0, 0, 0
	for i, e := range touched {
		if dbg := debounced(ctx, pool, "news_summaries", e, o.narrateSkip); dbg {
			skipped++
			continue
		}
		name := lookup(ctx, pool, e, logger)
		if name == "" {
			fail++
			continue
		}
		gctx, cancel := context.WithTimeout(ctx, o.gemmaTimeout+10*time.Second)
		res, err := o.narrator.Generate(gctx, ml.NarrativesRequest{
			EntityType: e.EntityType, EntityID: e.EntityID, EntityName: name, Sport: e.Sport, TriggerType: "periodic",
		})
		cancel()
		switch {
		case err != nil:
			fail++
			logger.Warn("pipeline/narratives: generate failed", "sport", e.Sport, "entity", name, "id", e.EntityID, "error", err)
		case res.SkippedNoCorpus:
			noCorpus++
		default:
			ok++
		}
		progress(logger, "pipeline/narratives", i+1, len(touched), ok, fail, skipped, noCorpus)
		if o.narrateThrottle > 0 {
			time.Sleep(time.Duration(o.narrateThrottle) * time.Millisecond)
		}
	}
	logger.Info("pipeline/narratives: done", "ok", ok, "fail", fail, "skipped_recent", skipped, "no_corpus", noCorpus)
}

// Stage 3 — sentiment for every touched entity (reads the stage-2 narratives + heat).
func runVibe(ctx context.Context, pool *pgxpool.Pool, o opts, touched []corpus.Entity, logger *slog.Logger) {
	ok, fail, skipped, noCorpus := 0, 0, 0, 0
	for i, e := range touched {
		if debounced(ctx, pool, "vibe_scores", e, o.vibeSkip) {
			skipped++
			continue
		}
		name := lookup(ctx, pool, e, logger)
		if name == "" {
			fail++
			continue
		}
		gctx, cancel := context.WithTimeout(ctx, o.gemmaTimeout+10*time.Second)
		res, err := o.vibeGen.Generate(gctx, ml.VibeRequest{
			EntityType: e.EntityType, EntityID: e.EntityID, EntityName: name, Sport: e.Sport, TriggerType: "periodic",
		})
		cancel()
		switch {
		case err != nil:
			fail++
			logger.Warn("pipeline/sentiment: generate failed", "sport", e.Sport, "entity", name, "id", e.EntityID, "error", err)
		case res.SkippedNoCorpus:
			noCorpus++
		default:
			ok++
		}
		progress(logger, "pipeline/sentiment", i+1, len(touched), ok, fail, skipped, noCorpus)
		if o.vibeThrottle > 0 {
			time.Sleep(time.Duration(o.vibeThrottle) * time.Millisecond)
		}
	}
	logger.Info("pipeline/sentiment: done", "ok", ok, "fail", fail, "skipped_recent", skipped, "no_corpus", noCorpus)
}

// Stage 4 — vibe synthesis for every touched entity.
func runSynthesis(ctx context.Context, pool *pgxpool.Pool, o opts, touched []corpus.Entity, logger *slog.Logger) {
	ok, fail, skipped, noPillars := 0, 0, 0, 0
	for i, e := range touched {
		dctx, dcancel := context.WithTimeout(ctx, 5*time.Second)
		recent := ml.RecentlySynthesized(dctx, pool, e.EntityType, e.EntityID, e.Sport, o.synthSkip)
		dcancel()
		if recent {
			skipped++
			continue
		}
		name := lookup(ctx, pool, e, logger)
		if name == "" {
			fail++
			continue
		}
		gctx, cancel := context.WithTimeout(ctx, o.gemmaTimeout+10*time.Second)
		res, err := o.synthGen.Generate(gctx, ml.SigilRequest{
			EntityType: e.EntityType, EntityID: e.EntityID, EntityName: name, Sport: e.Sport, TriggerType: "periodic",
		})
		cancel()
		switch {
		case err != nil:
			fail++
			logger.Warn("pipeline/synthesis: generate failed", "sport", e.Sport, "entity", name, "id", e.EntityID, "error", err)
		case res.SkippedNoPillars:
			noPillars++
		default:
			ok++
		}
		progress(logger, "pipeline/synthesis", i+1, len(touched), ok, fail, skipped, noPillars)
		if o.synthThrottle > 0 {
			time.Sleep(time.Duration(o.synthThrottle) * time.Millisecond)
		}
	}
	logger.Info("pipeline/synthesis: done", "ok", ok, "fail", fail, "skipped_recent", skipped, "no_pillars", noPillars)
}

func debounced(ctx context.Context, pool *pgxpool.Pool, table string, e corpus.Entity, within time.Duration) bool {
	dctx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	return corpus.RecentlyGenerated(dctx, pool, table, e, within)
}

func lookup(ctx context.Context, pool *pgxpool.Pool, e corpus.Entity, logger *slog.Logger) string {
	lctx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	name, err := corpus.LookupEntityName(lctx, pool, e.EntityType, e.EntityID, e.Sport)
	if err != nil || name == "" {
		logger.Warn("pipeline: entity lookup failed",
			"entity_type", e.EntityType, "entity_id", e.EntityID, "sport", e.Sport, "error", err)
		return ""
	}
	return name
}

func progress(logger *slog.Logger, stage string, done, total, ok, fail, skipped, noCorpus int) {
	if done%25 == 0 {
		logger.Info(stage+": progress",
			"done", done, "total", total, "ok", ok, "fail", fail, "skipped", skipped, "no_corpus", noCorpus)
	}
}
