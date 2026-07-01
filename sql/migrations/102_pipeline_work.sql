-- 102_pipeline_work.sql  (FIRST-GPT-AUDIT Session 7 — durable news-pipeline work state)
--
-- The local model derive pipeline (transfers → narratives → vibe → momentum → sigil)
-- currently hands off between stages with an in-process `runStart` watermark and
-- relies on transient LISTEN/NOTIFY. A crash loses the set of affected entities,
-- and time-only debounce can skip changed inputs. This table is the durable
-- substitute: the queryable record of which per-entity derivation work is
-- pending, running, or failed.
--
-- One generic table, NOT one queue per stage (the audit's "prefer one generic
-- table"). Claimed with FOR UPDATE SKIP LOCKED; reopened when an input version
-- changes; stale 'running' rows (crashed worker) are requeued after a lease.
-- Completed work is DELETED — the table only ever holds outstanding work, so
-- `pipeline_work_status` answers "what is pending / running / failed?" directly.
--
-- Scope note (Session 7 of 7–9, designed together, implemented separately):
-- this migration + go/internal/work provide the durable state and its claim/
-- enqueue/complete primitives plus an operator view. The PRODUCERS (enqueue at
-- link-insert / vetted / transfer / rating / vibe / momentum / sigil changes)
-- and the CONSUMER that drains the queue in order land in Session 8; the
-- trigger-semantics repair lands in Session 9. Until then the table stays empty
-- in production — additive and inert.
--
-- Additive + idempotent.

BEGIN;

CREATE TABLE IF NOT EXISTS pipeline_work (
    stage         TEXT        NOT NULL,
    entity_type   TEXT        NOT NULL CHECK (entity_type IN ('player', 'team')),
    entity_id     INTEGER     NOT NULL,
    sport         TEXT        NOT NULL,
    status        TEXT        NOT NULL DEFAULT 'pending'
                  CHECK (status IN ('pending', 'running', 'failed')),
    attempts      INTEGER     NOT NULL DEFAULT 0,
    available_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_error    TEXT,
    input_version TEXT,
    PRIMARY KEY (stage, entity_type, entity_id, sport)
);

COMMENT ON TABLE pipeline_work IS
    'Durable per-entity derivation work queue (FIRST-GPT-AUDIT Session 7). Holds '
    'only OUTSTANDING work — pending/running/failed; completed rows are deleted. '
    'Claimed via FOR UPDATE SKIP LOCKED (go/internal/work). input_version lets a '
    'changed input reopen completed/failed work instead of relying on elapsed '
    'time.';

-- Claim path: ready rows for a stage = pending OR failed (retry once the backoff
-- in available_at elapses), oldest first.
CREATE INDEX IF NOT EXISTS idx_pipeline_work_claim
    ON pipeline_work (stage, available_at)
    WHERE status IN ('pending', 'failed');

-- Stale-recovery path: 'running' rows whose worker died, found by lease age.
CREATE INDEX IF NOT EXISTS idx_pipeline_work_running
    ON pipeline_work (updated_at)
    WHERE status = 'running';

-- Operator answer to "what backend derivation work is pending/running/failed?"
CREATE OR REPLACE VIEW pipeline_work_status AS
SELECT
    stage,
    status,
    COUNT(*)          AS n,
    MIN(available_at) AS oldest_available_at,
    MAX(attempts)     AS max_attempts
FROM pipeline_work
GROUP BY stage, status
ORDER BY stage, status;

COMMIT;
