package listener

import (
	"context"
	"log/slog"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/ml"
)

// vibeThresholdPercentile is the minimum new_percentile that qualifies a
// milestone event to trigger a vibe blurb. Top-10% moments are worth the
// Gemma inference cost; middling shifts aren't narrative-worthy.
const vibeThresholdPercentile = 90.0

// vibeDebounceWindow caps how often we regenerate a blurb for the same
// entity. A big game can fire multiple threshold crossings in one burst;
// one blurb per entity per 30 min is plenty.
const vibeDebounceWindow = 30 * time.Minute

// VibeWorker bundles the Gemma generator + debounce bookkeeping.
// nil is a valid zero value — handlePercentileChange skips dispatch when
// the worker isn't configured.
type VibeWorker struct {
	pool *pgxpool.Pool
	gen  *ml.Generator
}

// NewVibeWorker returns a worker ready to dispatch milestone-driven vibes.
// Safe to pass as nil if Ollama isn't available in the current environment.
func NewVibeWorker(pool *pgxpool.Pool, gen *ml.Generator) *VibeWorker {
	if pool == nil || gen == nil {
		return nil
	}
	return &VibeWorker{pool: pool, gen: gen}
}

// Dispatch is called from handlePercentileChange for every milestone event.
// Non-blocking: the Gemma call runs in the caller's goroutine, which is
// already a spawned goroutine per event. Errors are logged but not
// surfaced — vibe is best-effort.
func (w *VibeWorker) Dispatch(ctx context.Context, event PercentileChangeEvent, logger *slog.Logger) {
	if w == nil {
		return
	}
	// Only top-10% moments get a vibe. Middling shifts aren't worth the
	// Gemma cost, and we'd drown the DB in noise.
	if event.NewPercentile < vibeThresholdPercentile {
		return
	}

	// Real-time vibes are headliner-only. Starters get a daily batch blurb
	// via cmd/vibe -mode batch; bench/inactive never get one. This cap is
	// what lets a single local GPU keep up on NFL Sundays.
	tier, entityName, err := w.lookupEntity(ctx, event)
	if err != nil || entityName == "" {
		logger.Warn("vibe: entity lookup failed",
			"entity_type", event.EntityType, "entity_id", event.EntityID,
			"sport", event.Sport, "error", err)
		return
	}
	if tier != "headliner" {
		return
	}

	if w.recentlyGenerated(ctx, event) {
		return
	}

	req := ml.VibeRequest{
		EntityType:  event.EntityType,
		EntityID:    event.EntityID,
		EntityName:  entityName,
		Sport:       event.Sport,
		TriggerType: "milestone",
		Trigger: map[string]any{
			"stat_key":       event.StatKey,
			"old_percentile": event.OldPercentile,
			"new_percentile": event.NewPercentile,
			"season":         event.Season,
		},
	}

	result, err := w.gen.Generate(ctx, req)
	if err != nil {
		logger.Warn("vibe: generate failed",
			"entity_type", event.EntityType, "entity_id", event.EntityID,
			"sport", event.Sport, "stat", event.StatKey, "error", err)
		return
	}

	logger.Info("vibe: generated",
		"entity_type", event.EntityType, "entity_id", event.EntityID,
		"sport", event.Sport, "stat", event.StatKey,
		"duration", result.Duration,
		"news", len(result.InputNewsIDs), "tweets", len(result.InputTweetIDs),
		"sentiment", result.Sentiment)
}

// recentlyGenerated returns true if a blurb for this entity was stored
// within the debounce window. DB-backed so it survives API restarts and
// works across multiple instances sharing the same Postgres.
func (w *VibeWorker) recentlyGenerated(ctx context.Context, event PercentileChangeEvent) bool {
	var exists bool
	err := w.pool.QueryRow(ctx, `
		SELECT EXISTS (
			SELECT 1 FROM vibe_scores
			WHERE entity_type = $1 AND entity_id = $2 AND sport = $3
			  AND generated_at > NOW() - $4::interval
		)
	`, event.EntityType, event.EntityID, event.Sport,
		vibeDebounceWindow.String(),
	).Scan(&exists)
	if err != nil {
		// Fail open: on a query error we'd rather produce an extra vibe
		// than silently drop a milestone. Logged upstream anyway.
		return false
	}
	return exists
}

// lookupEntity returns the entity's tier + display name in one query so the
// worker can short-circuit non-headliners before any debounce or Gemma work.
func (w *VibeWorker) lookupEntity(ctx context.Context, event PercentileChangeEvent) (tier, name string, err error) {
	var query string
	if event.EntityType == "player" {
		query = `SELECT tier, name FROM players WHERE id = $1 AND sport = $2`
	} else {
		query = `SELECT tier, name FROM teams WHERE id = $1 AND sport = $2`
	}
	err = w.pool.QueryRow(ctx, query, event.EntityID, event.Sport).Scan(&tier, &name)
	return
}
