// Package work is the durable per-entity derivation work queue introduced by
// FIRST-GPT-AUDIT Session 7. It replaces the news pipeline's in-process
// `runStart` watermark and best-effort LISTEN/NOTIFY with explicit, durable
// state in the pipeline_work table (migration 102): the database can answer
// "what derivation work is pending, running, or failed?" and a crash can no
// longer lose the set of affected entities.
//
// Lifecycle of a row:
//
//	Enqueue → 'pending'                (idempotent; reopens on a changed input)
//	Claim   → 'running'                (FOR UPDATE SKIP LOCKED; leased)
//	Complete→ row deleted              (only while still 'running')
//	Fail    → 'failed' + backoff       (retryable until maxAttempts, then dead-letter)
//	RequeueStale: 'running' → 'pending' (recover a crashed worker's lease)
//
// Only outstanding work is ever stored — completed rows are removed — so a
// simple GROUP BY (the pipeline_work_status view) is the operator dashboard.
//
// The producers (where Enqueue is called) and the consumer (which drains a
// stage) are wired in Session 8; this package is the substrate they build on.
package work

import (
	"context"
	"fmt"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
	"github.com/jackc/pgx/v5/pgxpool"
)

// Stage names the derivation step a work item belongs to. Most stages are per-entity
// (player/team); StageScrub is the exception — it is ARTICLE-keyed (entity_type='article',
// entity_id=news_articles.id), the news ID-gate that on writing vetted fires the mig-103
// trigger enqueuing the per-entity derive stages. The Rust ScrubHandler drains it (the Go
// Drainer has no scrub handler); Go only ENQUEUES it (the maintenance scrub ticker, flag-gated).
type Stage string

const (
	StageScrub      Stage = "scrub"
	StageTransfers  Stage = "transfers"
	StageNarratives Stage = "narratives"
	StageVibe       Stage = "vibe"
	StageMomentum   Stage = "momentum"
	StageSigil      Stage = "sigil"
)

// Querier is the subset of pgx shared by *pgxpool.Pool and pgx.Tx, so Enqueue/
// Complete/Fail can run inside an existing transaction (committing atomically
// with whatever produced the input) or standalone against the pool.
type Querier interface {
	Exec(ctx context.Context, sql string, args ...any) (pgconn.CommandTag, error)
	Query(ctx context.Context, sql string, args ...any) (pgx.Rows, error)
	QueryRow(ctx context.Context, sql string, args ...any) pgx.Row
}

// Item identifies one unit of derivation work for an entity.
type Item struct {
	Stage        Stage
	EntityType   string // "player" | "team"
	EntityID     int
	Sport        string
	InputVersion string // hash/version of the stage inputs; "" when unused
	Attempts     int    // failures so far (populated by Claim)
}

// Enqueue records that (stage, entity, sport) needs work. Idempotent and safe to
// call inside an existing transaction (pass a pgx.Tx).
//
// Conflict policy: a fresh row starts 'pending'. An existing row is REOPENED to
// 'pending' (attempts reset, backoff cleared) only when its input_version
// changed or it was 'failed' — so a changed input reopens completed/failed work.
// An already-pending or in-flight 'running' row of the SAME input_version is left
// untouched, so duplicate enqueues collapse to one row without yanking a live
// lease (Complete is status-guarded, so a reopen mid-flight is not lost either).
func Enqueue(ctx context.Context, q Querier, it Item) error {
	_, err := q.Exec(ctx, `
		INSERT INTO pipeline_work
		    (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
		VALUES ($1, $2, $3, $4, 'pending', $5, NOW(), NOW())
		ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
		    status        = 'pending',
		    attempts      = 0,
		    available_at  = NOW(),
		    updated_at    = NOW(),
		    last_error    = NULL,
		    input_version = EXCLUDED.input_version
		WHERE pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
		   OR pipeline_work.status = 'failed'
	`, string(it.Stage), it.EntityType, it.EntityID, it.Sport, nullIfEmpty(it.InputVersion))
	if err != nil {
		return fmt.Errorf("enqueue %s %s/%d (%s): %w", it.Stage, it.EntityType, it.EntityID, it.Sport, err)
	}
	return nil
}

// Claim atomically leases up to limit ready rows for a stage, marking them
// 'running'. Ready = pending or failed with available_at <= now (a failed row is
// retried once its backoff elapses). Concurrent claimers receive disjoint rows
// via FOR UPDATE SKIP LOCKED. Run RequeueStale periodically to recover rows whose
// worker died mid-lease.
func Claim(ctx context.Context, pool *pgxpool.Pool, stage Stage, limit int) ([]Item, error) {
	tx, err := pool.Begin(ctx)
	if err != nil {
		return nil, fmt.Errorf("claim %s: begin: %w", stage, err)
	}
	defer tx.Rollback(ctx)

	rows, err := tx.Query(ctx, `
		WITH ready AS (
		    SELECT entity_type, entity_id, sport
		    FROM pipeline_work
		    WHERE stage = $1
		      AND status IN ('pending', 'failed')
		      AND available_at <= NOW()
		    ORDER BY available_at
		    FOR UPDATE SKIP LOCKED
		    LIMIT $2
		)
		UPDATE pipeline_work w
		   SET status = 'running', updated_at = NOW()
		  FROM ready r
		 WHERE w.stage = $1
		   AND w.entity_type = r.entity_type
		   AND w.entity_id = r.entity_id
		   AND w.sport = r.sport
		RETURNING w.entity_type, w.entity_id, w.sport, w.input_version, w.attempts
	`, string(stage), limit)
	if err != nil {
		return nil, fmt.Errorf("claim %s: %w", stage, err)
	}

	var items []Item
	for rows.Next() {
		it := Item{Stage: stage}
		var iv *string
		if err := rows.Scan(&it.EntityType, &it.EntityID, &it.Sport, &iv, &it.Attempts); err != nil {
			rows.Close()
			return nil, fmt.Errorf("claim %s: scan: %w", stage, err)
		}
		if iv != nil {
			it.InputVersion = *iv
		}
		items = append(items, it)
	}
	rows.Close()
	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("claim %s: rows: %w", stage, err)
	}
	if err := tx.Commit(ctx); err != nil {
		return nil, fmt.Errorf("claim %s: commit: %w", stage, err)
	}
	return items, nil
}

// Complete removes a finished work item — but only while it is still 'running',
// i.e. the caller still holds the lease. If a newer input reopened the row to
// 'pending' while the worker ran, Complete is a no-op and the reopened work
// survives for reprocessing.
func Complete(ctx context.Context, q Querier, it Item) error {
	_, err := q.Exec(ctx, `
		DELETE FROM pipeline_work
		 WHERE stage = $1 AND entity_type = $2 AND entity_id = $3 AND sport = $4
		   AND status = 'running'
	`, string(it.Stage), it.EntityType, it.EntityID, it.Sport)
	if err != nil {
		return fmt.Errorf("complete %s %s/%d: %w", it.Stage, it.EntityType, it.EntityID, err)
	}
	return nil
}

// Fail marks a leased item 'failed', records the cause, bumps attempts, and
// schedules a backoff before it is claimable again. Once attempts reach
// maxAttempts the row is parked far in the future — a visible dead-letter rather
// than an infinite retry. Like Complete, it only acts on a row still 'running'.
func Fail(ctx context.Context, q Querier, it Item, cause string, backoff time.Duration, maxAttempts int) error {
	_, err := q.Exec(ctx, `
		UPDATE pipeline_work
		   SET status = 'failed',
		       attempts = attempts + 1,
		       last_error = $5,
		       updated_at = NOW(),
		       available_at = CASE
		           WHEN attempts + 1 >= $6 THEN NOW() + INTERVAL '100 years'
		           ELSE NOW() + make_interval(secs => $7)
		       END
		 WHERE stage = $1 AND entity_type = $2 AND entity_id = $3 AND sport = $4
		   AND status = 'running'
	`, string(it.Stage), it.EntityType, it.EntityID, it.Sport,
		truncate(cause, 2000), maxAttempts, int(backoff.Seconds()))
	if err != nil {
		return fmt.Errorf("fail %s %s/%d: %w", it.Stage, it.EntityType, it.EntityID, err)
	}
	return nil
}

// Requeue hands a single leased ('running') item back to 'pending', immediately
// claimable, WITHOUT counting an attempt. It is the graceful-shutdown counterpart
// of RequeueStale: a worker shutting down mid-drain returns its leased rows now,
// on a fresh context, instead of stranding them 'running' until the stale lease
// expires (FIRST-GPT-AUDIT Session 13, F-018). Status-guarded like Complete/Fail —
// a no-op unless the row is still 'running' (a newer input may already have
// reopened it to 'pending'), so it never clobbers reopened work or another claim.
func Requeue(ctx context.Context, q Querier, it Item) error {
	_, err := q.Exec(ctx, `
		UPDATE pipeline_work
		   SET status = 'pending', updated_at = NOW(), available_at = NOW()
		 WHERE stage = $1 AND entity_type = $2 AND entity_id = $3 AND sport = $4
		   AND status = 'running'
	`, string(it.Stage), it.EntityType, it.EntityID, it.Sport)
	if err != nil {
		return fmt.Errorf("requeue %s %s/%d: %w", it.Stage, it.EntityType, it.EntityID, err)
	}
	return nil
}

// RequeueStale flips 'running' rows whose lease has expired (updated_at older
// than lease) back to 'pending', recovering work abandoned by a crashed worker.
// Returns the number of rows recovered.
func RequeueStale(ctx context.Context, q Querier, lease time.Duration) (int64, error) {
	tag, err := q.Exec(ctx, `
		UPDATE pipeline_work
		   SET status = 'pending', updated_at = NOW(), available_at = NOW()
		 WHERE status = 'running'
		   AND updated_at < NOW() - make_interval(secs => $1)
	`, int(lease.Seconds()))
	if err != nil {
		return 0, fmt.Errorf("requeue stale: %w", err)
	}
	return tag.RowsAffected(), nil
}

// StageStatus is one row of the pipeline_work_status view — a (stage, status)
// bucket with its count and the oldest item waiting.
type StageStatus struct {
	Stage       string
	Status      string
	Count       int
	Oldest      *time.Time
	MaxAttempts int
}

// Counts returns the pipeline_work_status view — the operator answer to "what
// derivation work is pending / running / failed?"
func Counts(ctx context.Context, q Querier) ([]StageStatus, error) {
	rows, err := q.Query(ctx, `
		SELECT stage, status, n, oldest_available_at, max_attempts
		FROM pipeline_work_status
	`)
	if err != nil {
		return nil, fmt.Errorf("work counts: %w", err)
	}
	defer rows.Close()

	var out []StageStatus
	for rows.Next() {
		var s StageStatus
		if err := rows.Scan(&s.Stage, &s.Status, &s.Count, &s.Oldest, &s.MaxAttempts); err != nil {
			return nil, fmt.Errorf("work counts: scan: %w", err)
		}
		out = append(out, s)
	}
	return out, rows.Err()
}

// DeadLetter is a pipeline_work row that has exhausted its retries — the
// F-019/F-020 dead-letter class: Fail parked it far in the future after
// maxAttempts, so it will never retry on its own and needs an operator.
type DeadLetter struct {
	Stage      string
	EntityType string
	EntityID   int
	Sport      string
	Attempts   int
	LastError  string
	UpdatedAt  time.Time
}

// DeadLetters returns the dead-lettered work — 'failed' rows parked beyond any
// real backoff. It keys off the far-future available_at that Fail sets at the
// retry cap (NOW() + 100 years), so it stays correct regardless of the maxAttempts
// constant. The operator answer to "what work is permanently stuck?"
func DeadLetters(ctx context.Context, q Querier) ([]DeadLetter, error) {
	rows, err := q.Query(ctx, `
		SELECT stage, entity_type, entity_id, sport, attempts, COALESCE(last_error, ''), updated_at
		FROM pipeline_work
		WHERE status = 'failed' AND available_at > NOW() + INTERVAL '50 years'
		ORDER BY updated_at DESC
	`)
	if err != nil {
		return nil, fmt.Errorf("dead letters: %w", err)
	}
	defer rows.Close()

	var out []DeadLetter
	for rows.Next() {
		var d DeadLetter
		if err := rows.Scan(&d.Stage, &d.EntityType, &d.EntityID, &d.Sport, &d.Attempts, &d.LastError, &d.UpdatedAt); err != nil {
			return nil, fmt.Errorf("dead letters: scan: %w", err)
		}
		out = append(out, d)
	}
	return out, rows.Err()
}

// nullIfEmpty maps "" to a SQL NULL so input_version stays NULL rather than ”.
func nullIfEmpty(s string) any {
	if s == "" {
		return nil
	}
	return s
}

// truncate bounds an error string before it is stored in last_error.
func truncate(s string, max int) string {
	if len(s) <= max {
		return s
	}
	return s[:max]
}
