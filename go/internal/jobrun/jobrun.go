// Package jobrun makes the backend batch jobs observable and non-overlapping
// (FIRST-GPT-AUDIT Session 13). It bundles two things every cron job needs:
//
//   - a per-job PostgreSQL advisory lock so two runs of the same job (e.g. the
//     nightly cron racing a manual invocation, or two crons) cannot overlap — the
//     second one finds the lock held and exits cleanly; and
//   - a durable run record in pipeline_runs (migration 106): job name, start/end,
//     source commit, outcome, attempted/succeeded/skipped/failed counts, and a
//     summarized error — so an operator can tell whether last night's work
//     completed without reading the logs.
//
// Usage:
//
//	run, ok, err := jobrun.Guard(ctx, pool, dbURL, "pipeline")
//	if err != nil { log + exit 1 }
//	if !ok { log "another run holds the lock" + exit 0 }   // overlap; recorded 'skipped'
//	defer run.Close()                                       // releases the lock
//	... do the work, tallying counts ...
//	run.Finish(ctx, status, counts, runErr)                 // stamps the outcome
//
// The advisory lock is a session lock held on a dedicated connection for the life
// of the run; closing that connection (Close) releases it. pipeline_work (the
// per-entity queue) is unaffected — Guard governs whole JOBS, while
// FOR UPDATE SKIP LOCKED governs individual work items (the Rust cognition worker
// is meant to drain alongside a cron run, so it deliberately does not take this
// lock).
package jobrun

import (
	"context"
	"fmt"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/albapepper/scoracle-data/internal/buildinfo"
)

// Status is the terminal outcome stamped into pipeline_runs.status.
type Status string

const (
	// StatusSuccess — completed with zero failures.
	StatusSuccess Status = "success"
	// StatusPartial — some per-entity items failed but are retryable (the failure
	// count is below the job's threshold). The work queue will retry them.
	StatusPartial Status = "partial"
	// StatusFailed — enumeration failed, a stage failed wholesale, or dead-lettered
	// work remains after retries.
	StatusFailed Status = "failed"
	// StatusSkipped — the advisory lock was held by another run; this invocation
	// did no work. Recorded so the schedule is visibly skipped, not silently missed.
	StatusSkipped Status = "skipped"
)

// lockNamespace prefixes the job name before hashing it into the advisory-lock key,
// keeping these locks clear of any other pg_advisory_lock use.
const lockNamespace = "scoracle.job."

// Counts is the per-run tally written to pipeline_runs.
type Counts struct {
	Attempted int
	Succeeded int
	Skipped   int
	Failed    int
}

// Run is a live pipeline_runs record that holds the job's advisory lock until
// Close. Build it with Guard.
type Run struct {
	pool *pgxpool.Pool
	id   int64
	job  string
	lock *pgx.Conn // dedicated connection holding the session advisory lock
	key  string
}

// Guard takes the per-job advisory lock and opens a 'running' pipeline_runs record.
//
// If another instance already holds the lock, Guard records a 'skipped' run and
// returns (nil, false, nil): the caller should log and exit 0 — the holder is
// already doing the work. On success it returns a live *Run (acquired=true) that
// the caller must Finish (stamp the outcome) and Close (release the lock).
func Guard(ctx context.Context, pool *pgxpool.Pool, dbURL, job string) (*Run, bool, error) {
	key := lockNamespace + job

	// A dedicated connection so the session lock lives exactly as long as the run,
	// independent of the pool's connection churn.
	conn, err := pgx.Connect(ctx, dbURL)
	if err != nil {
		return nil, false, fmt.Errorf("advisory-lock connect: %w", err)
	}

	var acquired bool
	if err := conn.QueryRow(ctx, `SELECT pg_try_advisory_lock(hashtext($1))`, key).Scan(&acquired); err != nil {
		conn.Close(context.Background())
		return nil, false, fmt.Errorf("pg_try_advisory_lock(%s): %w", job, err)
	}
	if !acquired {
		conn.Close(context.Background()) // closing the session also drops any of its locks
		recordSkipped(ctx, pool, job)
		return nil, false, nil
	}

	var id int64
	if err := pool.QueryRow(ctx, `
		INSERT INTO pipeline_runs (job, source_commit, status)
		VALUES ($1, $2, 'running') RETURNING id`,
		job, buildinfo.Commit).Scan(&id); err != nil {
		// We hold the lock; release it before bailing so a retry isn't blocked.
		_, _ = conn.Exec(context.Background(), `SELECT pg_advisory_unlock(hashtext($1))`, key)
		conn.Close(context.Background())
		return nil, false, fmt.Errorf("open pipeline_runs row: %w", err)
	}
	return &Run{pool: pool, id: id, job: job, lock: conn, key: key}, true, nil
}

// recordSkipped logs an overlap as a finished 'skipped' run row. Best-effort — an
// overlap is not itself an error, so a failure to record it is swallowed.
func recordSkipped(ctx context.Context, pool *pgxpool.Pool, job string) {
	rctx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()
	_, _ = pool.Exec(rctx, `
		INSERT INTO pipeline_runs (job, source_commit, status, finished_at)
		VALUES ($1, $2, 'skipped', NOW())`, job, buildinfo.Commit)
}

// Finish stamps the run's outcome (status, counts, summarized error). The DB write
// is detached from a short timeout so it lands even if the job's own context is on
// its way down; the caller decides the exit code independently of this record.
func (r *Run) Finish(ctx context.Context, status Status, c Counts, runErr error) error {
	var errStr *string
	if runErr != nil {
		s := truncate(runErr.Error(), 2000)
		errStr = &s
	}
	fctx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	_, err := r.pool.Exec(fctx, `
		UPDATE pipeline_runs
		   SET finished_at = NOW(), status = $2,
		       attempted = $3, succeeded = $4, skipped = $5, failed = $6, error = $7
		 WHERE id = $1`,
		r.id, string(status), c.Attempted, c.Succeeded, c.Skipped, c.Failed, errStr)
	if err != nil {
		return fmt.Errorf("finish pipeline_runs row %d: %w", r.id, err)
	}
	return nil
}

// Close releases the advisory lock by closing its connection (which drops the
// session lock). Idempotent; call it with defer right after Guard succeeds.
func (r *Run) Close() {
	if r == nil || r.lock == nil {
		return
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	_, _ = r.lock.Exec(ctx, `SELECT pg_advisory_unlock(hashtext($1))`, r.key)
	_ = r.lock.Close(context.Background())
	r.lock = nil
}

// truncate bounds the summarized error before it is stored.
func truncate(s string, max int) string {
	if len(s) <= max {
		return s
	}
	return s[:max]
}
