// Package derive is the shared consumer of the durable pipeline_work queue
// (migration 102, go/internal/work). It drains the per-entity derive stages —
// transfers → narratives → vibe → sigil — in declared order, one stage fully
// before the next, with each item claimed/completed/failed through the queue.
//
// FIRST-GPT-AUDIT Session 8 first wired this drain inside cmd/pipeline (the nightly
// cron). Session 9 extracts it here so the SAME implementation backs both the
// nightly run and the real-time in-API drain worker (worker.go) — there is exactly
// one drain code path, so the two can never drift. The producers (who enqueue work)
// are the migration-103 vetted-transition trigger and drainVibe's own vibe→sigil
// hand-off; this package only consumes.
package derive

import (
	"context"
	"fmt"
	"log/slog"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/corpus"
	"github.com/albapepper/scoracle-data/internal/ml"
	"github.com/albapepper/scoracle-data/internal/work"
)

const (
	// perTeamTimeout bounds one team's transfer analysis (many Gemma calls, one per
	// candidate). Generous — the candidate count is the real bound.
	perTeamTimeout = 10 * time.Minute
	// StaleLease is how long a 'running' row may sit before work.RequeueStale treats
	// it as a crashed worker's abandoned lease and re-opens it. Longer than any single
	// item's processing budget (transfers can take perTeamTimeout) so a slow-but-alive
	// drainer is not stolen — which also keeps the nightly cron and the in-API worker
	// from yanking each other's live leases. Overlap prevention proper is Session 13.
	StaleLease = 30 * time.Minute
	// failBackoff delays a failed item before it is claimable again; after maxAttempts
	// it is parked as a visible dead-letter.
	failBackoff = 30 * time.Minute
	maxAttempts = 5
	// claimBatch is how many ready rows a stage leases per Claim. Ollama serializes
	// generation anyway, so this just bounds the lease set, not real concurrency.
	claimBatch = 10
)

// Drainer holds the generators + tuning the stage runners need. Build it with a
// struct literal; the real-time worker leaves the throttles/Limit at zero, while
// cmd/pipeline wires them from its smoke-run flags.
type Drainer struct {
	Pool         *pgxpool.Pool
	TransferGen  *ml.TransferGenerator
	Narrator     *ml.NewsNarrator
	VibeGen      *ml.VibeGenerator
	SynthGen     *ml.SigilGenerator
	GemmaTimeout time.Duration
	MinArticles  int // [transfers] candidate pre-filter

	// Smoke-run governors (0 = off). The real-time worker uses 0 throughout.
	TransferThrottle int
	NarrateThrottle  int
	VibeThrottle     int
	SynthThrottle    int
	Limit            int // cap items processed per drained stage; 0 = no cap

	Logger *slog.Logger
}

// StageResult is the per-stage outcome of a drain: how many items completed (OK),
// failed and were recorded for retry/dead-letter (Failed), and were handed back
// unprocessed to 'pending' on a graceful shutdown (Requeued).
type StageResult struct {
	Stage    work.Stage
	OK       int
	Failed   int
	Requeued int
}

// Result aggregates the per-stage outcomes of one DrainAll, so a caller (e.g.
// cmd/pipeline) can decide an exit code from real counts.
type Result struct {
	Stages []StageResult
}

// Totals sums the per-stage counts.
func (r Result) Totals() (ok, failed, requeued int) {
	for _, s := range r.Stages {
		ok += s.OK
		failed += s.Failed
		requeued += s.Requeued
	}
	return ok, failed, requeued
}

// WholeStageFailure returns the name of the first stage that processed only
// failures (OK==0, Failed>0) — a likely systemic outage (e.g. Ollama down) rather
// than a per-entity problem — or "" if no stage failed wholesale.
func (r Result) WholeStageFailure() string {
	for _, s := range r.Stages {
		if s.OK == 0 && s.Failed > 0 {
			return string(s.Stage)
		}
	}
	return ""
}

// DrainAll drains the durable queue in declared order: transfers → narratives →
// vibe → sigil. Each stage is fully drained (subject to Limit) before the next, so
// narratives ground on the fresh transfer heat, vibe reads the fresh narratives, and
// a completed vibe enqueues its sigil. Honors ctx cancellation between items, and on
// a graceful shutdown hands the leased-but-unprocessed batch back to 'pending'
// (F-018). Returns the per-stage outcome.
func (d *Drainer) DrainAll(ctx context.Context) Result {
	return Result{Stages: []StageResult{
		d.drainTransfers(ctx),
		d.drainNarratives(ctx),
		d.drainVibe(ctx), // each completion enqueues a sigil convergence
		d.drainSigil(ctx),
	}}
}

// drainStage claims and runs ready work for one stage until the queue drains. On
// success the row is deleted (Complete); on a genuine error it is failed with
// backoff (dead-lettered after maxAttempts). run owns its own per-item timeout —
// stages differ (a team's transfer analysis is many Gemma calls). Honors Limit as a
// per-stage processing cap for smoke runs.
//
// Graceful shutdown (FIRST-GPT-AUDIT Session 13, F-018): when the drain context is
// cancelled, the leased-but-unprocessed rows in the current batch are handed back to
// 'pending' (work.Requeue) on a fresh context so a restart mid-drain finds them
// immediately claimable instead of stranded 'running' until the 30m stale lease. The
// queue bookkeeping (Complete/Fail/Requeue) therefore runs on a detached context, so
// it always lands even as the drain context is going down.
func (d *Drainer) drainStage(ctx context.Context, stage work.Stage, throttle int, run func(context.Context, work.Item) error) StageResult {
	res := StageResult{Stage: stage}
	processed := 0
	for ctx.Err() == nil {
		batch := claimBatch
		if d.Limit > 0 {
			remaining := d.Limit - processed
			if remaining <= 0 {
				break
			}
			if remaining < batch {
				batch = remaining
			}
		}
		items, err := work.Claim(ctx, d.Pool, stage, batch)
		if err != nil {
			d.Logger.Error("derive: claim failed", "stage", stage, "error", err)
			break
		}
		if len(items) == 0 {
			break
		}
		for i, it := range items {
			// Shutdown before we run this item: hand back this item and the rest of
			// the leased batch (still 'running') so they don't strand.
			if ctx.Err() != nil {
				res.Requeued += d.requeueLeased(items[i:], stage)
				d.logStage(res)
				return res
			}
			rerr := run(ctx, it)
			switch {
			case rerr == nil:
				res.OK++
				d.complete(it)
			case ctx.Err() != nil:
				// The error is the shutdown cancelling the in-flight run, not the
				// item — hand it back (and the rest) instead of burning an attempt.
				res.Requeued += d.requeueLeased(items[i:], stage)
				d.logStage(res)
				return res
			default:
				res.Failed++
				d.Logger.Warn("derive: work failed", "stage", stage,
					"entity_type", it.EntityType, "entity_id", it.EntityID, "sport", it.Sport,
					"attempt", it.Attempts+1, "error", rerr)
				d.fail(it, rerr.Error())
			}
			processed++
			if throttle > 0 {
				time.Sleep(time.Duration(throttle) * time.Millisecond)
			}
		}
	}
	d.logStage(res)
	return res
}

// complete deletes a finished work item on a context detached from the drain, so a
// successful generation is recorded complete even if a shutdown just cancelled ctx.
func (d *Drainer) complete(it work.Item) {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	if err := work.Complete(ctx, d.Pool, it); err != nil {
		d.Logger.Error("derive: complete failed", "stage", it.Stage, "error", err)
	}
}

// fail records a genuine per-item failure (with backoff / eventual dead-letter) on
// a detached context.
func (d *Drainer) fail(it work.Item, cause string) {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	if err := work.Fail(ctx, d.Pool, it, cause, failBackoff, maxAttempts); err != nil {
		d.Logger.Error("derive: mark-failed failed", "stage", it.Stage, "error", err)
	}
}

// requeueLeased returns a worker's leased rows to 'pending' on a fresh context
// during a graceful shutdown (F-018), so they are immediately reclaimable rather
// than stranded 'running' for StaleLease. Returns how many were handed back.
func (d *Drainer) requeueLeased(items []work.Item, stage work.Stage) int {
	if len(items) == 0 {
		return 0
	}
	ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	n := 0
	for _, it := range items {
		if err := work.Requeue(ctx, d.Pool, it); err != nil {
			d.Logger.Warn("derive: shutdown requeue failed", "stage", stage,
				"entity_type", it.EntityType, "entity_id", it.EntityID, "sport", it.Sport, "error", err)
			continue
		}
		n++
	}
	if n > 0 {
		d.Logger.Info("derive: handed leased work back on shutdown", "stage", stage, "rows", n)
	}
	return n
}

// logStage emits the per-stage drain summary (only when the stage did something).
func (d *Drainer) logStage(res StageResult) {
	if res.OK+res.Failed+res.Requeued > 0 {
		d.Logger.Info("derive: stage drained", "stage", res.Stage,
			"ok", res.OK, "fail", res.Failed, "requeued", res.Requeued)
	}
}

// drainTransfers — team-scoped transfer analysis off the fresh, scrubbed corpus.
// A pair that hit a Gemma model failure is persisted UNKNOWN (is_rumor NULL, never
// served) and reported as res.Unknown; we then FAIL the item so the queue's own
// backoff re-enqueues this team's transfers stage — the fail-closed retry path
// (FIRST-GPT-AUDIT Session 10), no new mechanism. After maxAttempts the item
// dead-letters and the UNKNOWN rows simply stay unserved.
func (d *Drainer) drainTransfers(ctx context.Context) StageResult {
	return d.drainStage(ctx, work.StageTransfers, d.TransferThrottle, func(ctx context.Context, it work.Item) error {
		if it.EntityType != "team" {
			return fmt.Errorf("transfers: non-team entity %s/%d", it.EntityType, it.EntityID)
		}
		name, err := d.nameOf(ctx, it)
		if err != nil {
			return err
		}
		tctx, cancel := context.WithTimeout(ctx, perTeamTimeout)
		defer cancel()
		res, gerr := d.TransferGen.GenerateForTeam(tctx, ml.TransferRequest{
			TeamID: it.EntityID, TeamName: name, Sport: it.Sport, TriggerType: "periodic", MinArticles: d.MinArticles,
		})
		if gerr != nil {
			return gerr
		}
		if res != nil && res.Unknown > 0 {
			return fmt.Errorf("transfers: %d unresolved pair(s) (model failure) — retrying team %d", res.Unknown, it.EntityID)
		}
		return nil
	})
}

// drainNarratives — per-entity narratives, grounded on the fresh transfer heat.
func (d *Drainer) drainNarratives(ctx context.Context) StageResult {
	return d.drainStage(ctx, work.StageNarratives, d.NarrateThrottle, func(ctx context.Context, it work.Item) error {
		name, err := d.nameOf(ctx, it)
		if err != nil {
			return err
		}
		gctx, cancel := context.WithTimeout(ctx, d.GemmaTimeout+10*time.Second)
		defer cancel()
		_, gerr := d.Narrator.Generate(gctx, ml.NarrativesRequest{
			EntityType: it.EntityType, EntityID: it.EntityID, EntityName: name, Sport: it.Sport, TriggerType: "periodic",
		})
		return gerr
	})
}

// drainVibe — per-entity sentiment off the fresh narratives + heat. A completed vibe
// enqueues its sigil convergence BEFORE the vibe item is completed, so a crash in
// between re-runs vibe (idempotent) rather than dropping the sigil. (The migration-103
// trigger covers vetted transitions, not vibe-completion → sigil, so this hand-off
// stays in Go.)
func (d *Drainer) drainVibe(ctx context.Context) StageResult {
	return d.drainStage(ctx, work.StageVibe, d.VibeThrottle, func(ctx context.Context, it work.Item) error {
		name, err := d.nameOf(ctx, it)
		if err != nil {
			return err
		}
		gctx, cancel := context.WithTimeout(ctx, d.GemmaTimeout+10*time.Second)
		res, gerr := d.VibeGen.Generate(gctx, ml.VibeRequest{
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
		// Enqueue on a detached context so the sigil hand-off lands even if a
		// graceful shutdown just cancelled the drain — the vibe row's own Complete
		// also runs detached, preserving "enqueue sigil before completing vibe."
		ectx, ecancel := context.WithTimeout(context.Background(), 10*time.Second)
		if eerr := work.Enqueue(ectx, d.Pool, sig); eerr != nil {
			d.Logger.Warn("derive/vibe: enqueue sigil failed",
				"entity_type", it.EntityType, "entity_id", it.EntityID, "sport", it.Sport, "error", eerr)
		}
		ecancel()
		return nil
	})
}

// drainSigil — terminal convergence. The real-time queue is inherently CURRENT-SEASON:
// the SigilGenerator resolves the sport's current_season (SigilRequest.Season nil →
// current) and STAMPS it, reads its three pillars season-exact, and skips the Gemma
// call on an unchanged input hash (SkipUnchanged) — so an unchanged-pillar enqueue is a
// cheap no-op (FIRST-GPT-AUDIT Session 12). Historical seasons are only (re)synthesized
// by an explicit-season backfill, never by this queue.
func (d *Drainer) drainSigil(ctx context.Context) StageResult {
	return d.drainStage(ctx, work.StageSigil, d.SynthThrottle, func(ctx context.Context, it work.Item) error {
		name, err := d.nameOf(ctx, it)
		if err != nil {
			return err
		}
		gctx, cancel := context.WithTimeout(ctx, d.GemmaTimeout+10*time.Second)
		defer cancel()
		_, gerr := d.SynthGen.Generate(gctx, ml.SigilRequest{
			EntityType: it.EntityType, EntityID: it.EntityID, EntityName: name, Sport: it.Sport,
			TriggerType: "periodic", SkipUnchanged: true,
		})
		return gerr
	})
}

// nameOf resolves the entity display name for a Gemma prompt, treating an empty name
// as an error so the work item fails (and retries) rather than silently generating
// against a blank prompt.
func (d *Drainer) nameOf(ctx context.Context, it work.Item) (string, error) {
	lctx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	name, err := corpus.LookupEntityName(lctx, d.Pool, it.EntityType, it.EntityID, it.Sport)
	if err != nil {
		return "", fmt.Errorf("lookup %s/%d (%s): %w", it.EntityType, it.EntityID, it.Sport, err)
	}
	if name == "" {
		return "", fmt.Errorf("empty name for %s/%d (%s)", it.EntityType, it.EntityID, it.Sport)
	}
	return name, nil
}

// vibeVersion fingerprints a vibe result for the sigil queue's input_version. It is
// coarse on purpose — the SigilGenerator's own pillar input-hash is the real
// convergence gate; this only keeps the queue row's reopen/dedupe sane.
func vibeVersion(res *ml.VibeResult) string {
	if res == nil {
		return ""
	}
	return fmt.Sprintf("s%d", res.Sentiment)
}
