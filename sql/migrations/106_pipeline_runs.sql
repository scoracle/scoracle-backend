-- 106_pipeline_runs.sql  (FIRST-GPT-AUDIT Session 13 — observable, correctly-failing batch jobs)
--
-- The batch jobs (cmd/pipeline, cmd/statcommentary, cmd/vibesynth) count their
-- failures but historically exited zero, so cron could not tell a partial failure
-- from a clean run, and an operator had to read thousands of log lines to learn
-- whether last night's work actually completed. This table is the durable per-run
-- record those jobs now write: one row per invocation with the source commit, the
-- outcome, the attempted/succeeded/skipped/failed counts, and a summarized error.
--
-- It is the run-level companion to pipeline_work (migration 102): pipeline_work is
-- the queue of OUTSTANDING per-entity work; pipeline_runs is the append-only log of
-- the JOBS that drain it. The advisory lock that prevents two runs of the same job
-- from overlapping (go/internal/jobrun) is a PostgreSQL session lock, not a table —
-- nothing to migrate there; this table only records the runs.
--
-- status:
--   running  — opened at job start, updated on finish.
--   success  — completed with zero failures.
--   partial  — some per-entity items failed but are retryable (below threshold).
--   failed   — enumeration failed, a stage failed wholesale, or dead-lettered work
--              remains after retries.
--   skipped  — the advisory lock was held by another run; this invocation did
--              nothing (recorded so an operator sees the schedule was skipped, not
--              silently missed).
--
-- Additive + idempotent.

BEGIN;

CREATE TABLE IF NOT EXISTS pipeline_runs (
    id            BIGSERIAL   PRIMARY KEY,
    job           TEXT        NOT NULL,
    started_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    finished_at   TIMESTAMPTZ,
    source_commit TEXT,
    status        TEXT        NOT NULL DEFAULT 'running'
                  CHECK (status IN ('running', 'success', 'partial', 'failed', 'skipped')),
    attempted     INTEGER     NOT NULL DEFAULT 0,
    succeeded     INTEGER     NOT NULL DEFAULT 0,
    skipped       INTEGER     NOT NULL DEFAULT 0,
    failed        INTEGER     NOT NULL DEFAULT 0,
    error         TEXT
);

COMMENT ON TABLE pipeline_runs IS
    'Append-only per-run record of the backend batch jobs (FIRST-GPT-AUDIT Session '
    '13). One row per cmd/pipeline | cmd/statcommentary | cmd/vibesynth invocation: '
    'source commit, outcome (running/success/partial/failed/skipped), '
    'attempted/succeeded/skipped/failed counts, and a summarized error. The '
    'run-level companion to pipeline_work (the per-entity queue).';

-- Operator lookups: most-recent runs per job.
CREATE INDEX IF NOT EXISTS idx_pipeline_runs_job_started
    ON pipeline_runs (job, started_at DESC);

-- "Did last night's jobs complete?" — the latest run per job at a glance.
CREATE OR REPLACE VIEW pipeline_runs_latest AS
SELECT DISTINCT ON (job)
    job,
    id,
    status,
    started_at,
    finished_at,
    EXTRACT(EPOCH FROM (COALESCE(finished_at, NOW()) - started_at))::int AS duration_s,
    source_commit,
    attempted,
    succeeded,
    skipped,
    failed,
    error
FROM pipeline_runs
ORDER BY job, started_at DESC;

COMMIT;
