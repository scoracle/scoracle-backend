// pipeline — the once-daily compile → scrub → derive → reveal orchestrator.
//
// FIRST-GPT-AUDIT Session 8 turned this from an in-process-watermark chain into
// an ordered, durable, crash-recoverable pipeline. The handoff between stages is
// the pipeline_work queue (migration 102, go/internal/work), not the old runStart
// watermark — so a kill mid-run resumes from the database, and a re-run with no
// fresh input does no Gemma work:
//
//	requeue stale → sweep → scrub(fresh batch) → enqueue derive work
//	  → drain transfers → narratives → vibe → sigil   (in declared order)
//
//	Stage 0  RSS sweep returns the articles that gained a FRESH link this run.
//	Stage 1  those exact articles are scrubbed IN-RUN (Gemma ID-gate, vetted=TRUE),
//	         then each vetted entity is enqueued for its derive stages — teams also
//	         for transfers. input_version is a corpus fingerprint, so an unchanged
//	         corpus dedupes and a changed one reopens.
//	Stages   each stage is drained from the queue in order (Claim → run → Complete/
//	         Fail); narratives ground on the fresh transfer heat, vibe reads the
//	         fresh narratives, and a completed vibe enqueues its sigil convergence.
//
// The async maintenance scrub ticker is now backlog/repair only — the daily run
// scrubs its own fresh batch here. Real-time coverage between runs is still the
// in-API LISTEN/NOTIFY workers (Session 9 converts those to enqueue durable work).
// The terminal sigil stage uses the existing SigilGenerator (3 pillars + input-hash
// SkipUnchanged); the full event-driven Sigil convergence lifecycle is Session 12.
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
	"github.com/albapepper/scoracle-data/internal/ml"
	"github.com/albapepper/scoracle-data/internal/work"
)

const (
	// perTeamTimeout bounds one team's transfer analysis (many Gemma calls, one
	// per candidate). Generous — the candidate count is the real bound.
	perTeamTimeout = 10 * time.Minute
	// staleLease is how long a 'running' row may sit before RequeueStale treats it
	// as a crashed worker's abandoned lease and re-opens it. Longer than any single
	// item's processing budget (transfers can take perTeamTimeout) so a slow-but-
	// alive prior run is not stolen. Overlap prevention proper is Session 13.
	staleLease = 30 * time.Minute
	// failBackoff delays a failed item before it is claimable again; after
	// maxAttempts it is parked as a visible dead-letter.
	failBackoff = 30 * time.Minute
	maxAttempts = 5
	// claimBatch is how many ready rows a stage leases per Claim. Ollama serializes
	// generation anyway, so this just bounds the lease set, not real concurrency.
	claimBatch = 10
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

	runCorpus(pool, opts{
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
	}, logger)
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

func runCorpus(pool *pgxpool.Pool, o opts, logger *slog.Logger) {
	sports := []string{"NBA", "NFL", "FOOTBALL"}
	if s := strings.ToLower(strings.TrimSpace(o.sportArg)); s != "" && s != "all" {
		sports = []string{strings.ToUpper(o.sportArg)}
	}
	ctx := context.Background()
	start := time.Now()
	logger.Info("pipeline: starting", "sports", sports)

	// Recover work abandoned by a crashed prior run before claiming anything new.
	if n, err := work.RequeueStale(ctx, pool, staleLease); err != nil {
		logger.Warn("pipeline: requeue stale failed", "error", err)
	} else if n > 0 {
		logger.Info("pipeline: recovered stale work", "rows", n)
	}

	// Stage 0 — RSS sweep. affected = articles that gained a FRESH link this run.
	_, affected, ok, fail := corpus.Sweep(ctx, pool, sports, o.rssLimit, o.rssPauseMs, logger)
	logger.Info("pipeline: sweep done", "rss_ok", ok, "rss_fail", fail, "fresh_articles", len(affected))

	// Stage 1 — scrub the fresh batch IN-RUN, then enqueue derive work for the
	// entities Gemma vetted. Fresh content is vetted before it can derive.
	scrubAndEnqueue(ctx, pool, o, affected, logger)

	// Stages 2–5 — drain the durable queue in declared order. pipeline_work is the
	// cross-stage handoff; the queue (not a process watermark) survives a crash.
	drainTransfers(ctx, pool, o, logger)
	drainNarratives(ctx, pool, o, logger)
	drainVibe(ctx, pool, o, logger) // each completion enqueues a sigil convergence
	drainSigil(ctx, pool, o, logger)

	logRemaining(ctx, pool, logger)
	logger.Info("pipeline: complete", "elapsed", time.Since(start).Round(time.Second))
}

// scrubAndEnqueue scrubs the freshly-ingested articles (Gemma ID-gate, persisting
// vetted=TRUE + scrubbed_at) and enqueues each vetted entity's derive work as soon
// as ITS article is scrubbed — narratives + vibe for every entity, plus transfers
// for teams. Per-article (not batch-at-end) so a crash mid-batch preserves the work
// already scrubbed: the next run's sweep won't re-detect an already-persisted
// article, so the enqueue must be the durable handoff right behind each scrub. The
// corpus fingerprint is the input_version, so an unchanged corpus dedupes and a
// changed one reopens completed/failed work. (Session 9 adds the vetted-transition
// trigger that closes the residual single-article scrub→enqueue window.)
func scrubAndEnqueue(ctx context.Context, pool *pgxpool.Pool, o opts, affected map[int64]string, logger *slog.Logger) {
	if len(affected) == 0 {
		logger.Info("pipeline/scrub: no fresh articles; nothing to derive")
		return
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

	scrubbed, failed, enq := 0, 0, 0
	seen := make(map[corpus.Entity]bool) // dedupe an entity co-mentioned across articles
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

		// Durable handoff: enqueue this article's vetted entities now.
		entities, err := corpus.AffectedVettedEntities(ctx, pool, []int64{id})
		if err != nil {
			logger.Warn("pipeline/scrub: load vetted entities failed", "article_id", id, "error", err)
			continue
		}
		for _, e := range entities {
			if seen[e] {
				continue
			}
			seen[e] = true
			enqueueDerive(ctx, pool, e, logger)
			enq++
		}
	}
	logger.Info("pipeline/scrub: done", "scrubbed", scrubbed, "failed", failed, "entities_enqueued", enq)
}

// enqueueDerive enqueues an entity's derive stages (narratives + vibe; teams also
// transfers) keyed by its current corpus fingerprint, so an unchanged corpus is an
// idempotent no-op and a changed one reopens the work.
func enqueueDerive(ctx context.Context, pool *pgxpool.Pool, e corpus.Entity, logger *slog.Logger) {
	ver, verr := corpus.CorpusVersion(ctx, pool, e)
	if verr != nil {
		logger.Warn("pipeline/scrub: corpus version failed",
			"entity_type", e.EntityType, "entity_id", e.EntityID, "sport", e.Sport, "error", verr)
	}
	base := work.Item{EntityType: e.EntityType, EntityID: e.EntityID, Sport: e.Sport, InputVersion: ver}
	stages := []work.Stage{work.StageNarratives, work.StageVibe}
	if e.EntityType == "team" {
		stages = append(stages, work.StageTransfers)
	}
	for _, st := range stages {
		it := base
		it.Stage = st
		if err := work.Enqueue(ctx, pool, it); err != nil {
			logger.Warn("pipeline/scrub: enqueue failed", "stage", st,
				"entity_type", e.EntityType, "entity_id", e.EntityID, "sport", e.Sport, "error", err)
		}
	}
}

// drainStage claims and runs ready work for one stage until the queue drains.
// On success the row is deleted (Complete); on error it is failed with backoff
// (dead-lettered after maxAttempts). run owns its own per-item timeout — stages
// differ (a team's transfer analysis is many Gemma calls). Honors o.limit as a
// per-stage processing cap for smoke runs.
func drainStage(ctx context.Context, pool *pgxpool.Pool, stage work.Stage, throttle, limit int, run func(context.Context, work.Item) error, logger *slog.Logger) {
	ok, fail, processed := 0, 0, 0
	for ctx.Err() == nil {
		batch := claimBatch
		if limit > 0 {
			remaining := limit - processed
			if remaining <= 0 {
				break
			}
			if remaining < batch {
				batch = remaining
			}
		}
		items, err := work.Claim(ctx, pool, stage, batch)
		if err != nil {
			logger.Error("pipeline: claim failed", "stage", stage, "error", err)
			break
		}
		if len(items) == 0 {
			break
		}
		for _, it := range items {
			if rerr := run(ctx, it); rerr != nil {
				fail++
				logger.Warn("pipeline: work failed", "stage", stage,
					"entity_type", it.EntityType, "entity_id", it.EntityID, "sport", it.Sport,
					"attempt", it.Attempts+1, "error", rerr)
				if ferr := work.Fail(ctx, pool, it, rerr.Error(), failBackoff, maxAttempts); ferr != nil {
					logger.Error("pipeline: mark-failed failed", "stage", stage, "error", ferr)
				}
			} else {
				ok++
				if cerr := work.Complete(ctx, pool, it); cerr != nil {
					logger.Error("pipeline: complete failed", "stage", stage, "error", cerr)
				}
			}
			processed++
			if throttle > 0 {
				time.Sleep(time.Duration(throttle) * time.Millisecond)
			}
		}
	}
	logger.Info("pipeline: stage drained", "stage", stage, "ok", ok, "fail", fail)
}

// drainTransfers — team-scoped transfer analysis off the fresh, scrubbed corpus.
func drainTransfers(ctx context.Context, pool *pgxpool.Pool, o opts, logger *slog.Logger) {
	drainStage(ctx, pool, work.StageTransfers, o.transferThrottle, o.limit, func(ctx context.Context, it work.Item) error {
		if it.EntityType != "team" {
			return fmt.Errorf("transfers: non-team entity %s/%d", it.EntityType, it.EntityID)
		}
		name, err := nameOf(ctx, pool, it)
		if err != nil {
			return err
		}
		tctx, cancel := context.WithTimeout(ctx, perTeamTimeout)
		defer cancel()
		_, gerr := o.transferGen.GenerateForTeam(tctx, ml.TransferRequest{
			TeamID: it.EntityID, TeamName: name, Sport: it.Sport, TriggerType: "periodic", MinArticles: o.minArticles,
		})
		return gerr
	}, logger)
}

// drainNarratives — per-entity narratives, grounded on the fresh transfer heat.
func drainNarratives(ctx context.Context, pool *pgxpool.Pool, o opts, logger *slog.Logger) {
	drainStage(ctx, pool, work.StageNarratives, o.narrateThrottle, o.limit, func(ctx context.Context, it work.Item) error {
		name, err := nameOf(ctx, pool, it)
		if err != nil {
			return err
		}
		gctx, cancel := context.WithTimeout(ctx, o.gemmaTimeout+10*time.Second)
		defer cancel()
		_, gerr := o.narrator.Generate(gctx, ml.NarrativesRequest{
			EntityType: it.EntityType, EntityID: it.EntityID, EntityName: name, Sport: it.Sport, TriggerType: "periodic",
		})
		return gerr
	}, logger)
}

// drainVibe — per-entity sentiment off the fresh narratives + heat. A completed
// vibe enqueues its sigil convergence BEFORE the vibe item is completed, so a
// crash in between re-runs vibe (idempotent) rather than dropping the sigil.
func drainVibe(ctx context.Context, pool *pgxpool.Pool, o opts, logger *slog.Logger) {
	drainStage(ctx, pool, work.StageVibe, o.vibeThrottle, o.limit, func(ctx context.Context, it work.Item) error {
		name, err := nameOf(ctx, pool, it)
		if err != nil {
			return err
		}
		gctx, cancel := context.WithTimeout(ctx, o.gemmaTimeout+10*time.Second)
		res, gerr := o.vibeGen.Generate(gctx, ml.VibeRequest{
			EntityType: it.EntityType, EntityID: it.EntityID, EntityName: name, Sport: it.Sport, TriggerType: "periodic",
		})
		cancel()
		if gerr != nil {
			return gerr
		}
		sig := work.Item{
			Stage: work.StageSigil, EntityType: it.EntityType, EntityID: it.EntityID, Sport: it.Sport,
			InputVersion: vibeVersion(res),
		}
		if eerr := work.Enqueue(ctx, pool, sig); eerr != nil {
			logger.Warn("pipeline/vibe: enqueue sigil failed",
				"entity_type", it.EntityType, "entity_id", it.EntityID, "sport", it.Sport, "error", eerr)
		}
		return nil
	}, logger)
}

// drainSigil — terminal convergence. The SigilGenerator reads its three pillars
// live and skips the Gemma call on an unchanged input hash (SkipUnchanged), so an
// unchanged-pillar enqueue is a cheap no-op. The full event-driven Sigil lifecycle
// (season-scoping, follower/FCM decoupling, nightly→reconciliation) is Session 12.
func drainSigil(ctx context.Context, pool *pgxpool.Pool, o opts, logger *slog.Logger) {
	drainStage(ctx, pool, work.StageSigil, o.synthThrottle, o.limit, func(ctx context.Context, it work.Item) error {
		name, err := nameOf(ctx, pool, it)
		if err != nil {
			return err
		}
		gctx, cancel := context.WithTimeout(ctx, o.gemmaTimeout+10*time.Second)
		defer cancel()
		_, gerr := o.synthGen.Generate(gctx, ml.SigilRequest{
			EntityType: it.EntityType, EntityID: it.EntityID, EntityName: name, Sport: it.Sport,
			TriggerType: "periodic", SkipUnchanged: true,
		})
		return gerr
	}, logger)
}

// nameOf resolves the entity display name for a Gemma prompt, treating an empty
// name as an error so the work item fails (and retries) rather than silently
// generating against a blank prompt.
func nameOf(ctx context.Context, pool *pgxpool.Pool, it work.Item) (string, error) {
	lctx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	name, err := corpus.LookupEntityName(lctx, pool, it.EntityType, it.EntityID, it.Sport)
	if err != nil {
		return "", fmt.Errorf("lookup %s/%d (%s): %w", it.EntityType, it.EntityID, it.Sport, err)
	}
	if name == "" {
		return "", fmt.Errorf("empty name for %s/%d (%s)", it.EntityType, it.EntityID, it.Sport)
	}
	return name, nil
}

// vibeVersion fingerprints a vibe result for the sigil queue's input_version. It
// is coarse on purpose — the SigilGenerator's own pillar input-hash is the real
// convergence gate; this only keeps the queue row's reopen/dedupe sane.
func vibeVersion(res *ml.VibeResult) string {
	if res == nil {
		return ""
	}
	return fmt.Sprintf("s%d", res.Sentiment)
}

// logRemaining prints the pipeline_work backlog after the run — the operator's
// "what's still pending/running/failed?" snapshot (anything left is real-time or
// dead-lettered work for the maintenance/Session-9 path to drain).
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
