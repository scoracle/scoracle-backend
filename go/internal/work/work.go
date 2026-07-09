// Package work is the Go producer/operator surface for the durable derivation
// work queue. Rust owns the worker lease lifecycle; Go enqueues work, recovers
// stale leases for operators, and reads queue status.
//
// Lifecycle of a row:
//
//	Enqueue     → 'pending'                (idempotent; reopens on a changed input)
//	Rust worker → 'running'/'failed'/delete (claims, backs off, completes)
//	RequeueStale: 'running' → 'pending'    (operator recovery for abandoned leases)
//
// Only outstanding work is ever stored — completed rows are removed — so a
// simple GROUP BY (the pipeline_work_status view) is the operator dashboard.
package work

import (
	"context"
	"fmt"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
)

// Stage names the derivation step a work item belongs to. Most stages are per-entity
// (player/team); StageScrub is the exception — it is ARTICLE-keyed (entity_type='article',
// entity_id=news_articles.id), the news ID-gate that on writing vetted fires the mig-103
// trigger enqueuing the per-entity derive stages. Rust drains it; Go only
// enqueues it from ingest/listener/maintenance paths.
type Stage string

const (
	StageScrub      Stage = "scrub"
	StageTransfers  Stage = "transfers"
	StageNarratives Stage = "narratives"
	StageVibe       Stage = "vibe"
	StageMomentum   Stage = "momentum"
	StageSigil      Stage = "sigil"
)

// Querier is the subset of pgx shared by *pgxpool.Pool and pgx.Tx, so Enqueue
// can run inside an existing transaction (committing atomically with whatever
// produced the input) or standalone against the pool.
type Querier interface {
	Exec(ctx context.Context, sql string, args ...any) (pgconn.CommandTag, error)
	Query(ctx context.Context, sql string, args ...any) (pgx.Rows, error)
}

// Item identifies one unit of derivation work for an entity.
type Item struct {
	Stage        Stage
	EntityType   string // "player" | "team"
	EntityID     int
	Sport        string
	InputVersion string // hash/version of the stage inputs; "" when unused
}

// Enqueue records that (stage, entity, sport) needs work. Idempotent and safe to
// call inside an existing transaction (pass a pgx.Tx).
//
// Conflict policy: a fresh row starts 'pending'. An existing row is REOPENED to
// 'pending' (attempts reset, backoff cleared) only when its input_version
// changed or it was 'failed' — so a changed input reopens completed/failed work.
// An already-pending or in-flight 'running' row of the SAME input_version is left
// untouched, so duplicate enqueues collapse to one row without yanking a live
// lease (the Rust completion path is status-guarded, so a reopen mid-flight is
// not lost either).
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

// DeadLetter is a pipeline_work row that has exhausted its retries. The Rust
// worker parks it far in the future after MAX_ATTEMPTS, so it will never retry
// on its own and needs an operator.
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
// real backoff. It keys off the far-future available_at that the Rust worker
// sets at the retry cap (NOW() + 100 years). The operator answer to "what work
// is permanently stuck?"
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
