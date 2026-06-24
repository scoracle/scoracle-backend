# FIRST-GPT-AUDIT Session 13 — Observable, non-overlapping, correctly-failing batch jobs

**Date:** 2026-06-23 (archbox) · **Code:** `c35e1ba` · **Migration:** `106_pipeline_runs` (applied per-file) · **Deployed live**

## Goal

> An operator can tell whether last night's backend work actually completed without reading thousands of
> log lines — jobs are non-overlapping, report a durable per-run record, and exit non-zero on real failure.

The three Gemma batch jobs (`cmd/pipeline`, `cmd/statcommentary`, `cmd/vibesynth`) counted their failures
but frequently exited 0, had no overlap guard, no durable run record, and no dead-letter report. Plus a
known correctness gap: an API restart mid-drain stranded the derive worker's leased batch for up to 30m
(F-018).

## What shipped

### 1. Durable run record — `pipeline_runs` (migration 106, additive)
One append-only row per job invocation: `job`, `started_at`/`finished_at`, `source_commit`
(`buildinfo.Commit`), `status` (`running`/`success`/`partial`/`failed`/`skipped`), `attempted`/
`succeeded`/`skipped`/`failed` counts, summarized `error`. Plus a `pipeline_runs_latest` view (last run
per job, with `duration_s`) — the "did last night complete?" dashboard. It is the run-level companion to
`pipeline_work` (the per-entity queue from migration 102).

### 2. Overlap guard + run writer — `internal/jobrun` (F-012 RESOLVED)
`jobrun.Guard(ctx, pool, dbURL, job)` takes a per-job **session advisory lock**
(`pg_try_advisory_lock(hashtext('scoracle.job.'+job))`) on a dedicated connection held for the run's life,
and opens the `running` `pipeline_runs` row. If the lock is held, it records a `skipped` row and returns
`acquired=false` → the caller logs and **exits 0**. `Run.Finish` stamps the outcome; `Run.Close` releases
the lock. Job lock names: `pipeline`, `statcommentary`, `vibesynth` (backfill + nightly/reconcile share the
"vibesynth" lock so they can't run at once).

The in-API derive worker deliberately **does not** take the lock — it is meant to drain alongside the
nightly cron, and `Claim`'s `FOR UPDATE SKIP LOCKED` already keeps their claimed rows disjoint. The lock
guards JOB-vs-JOB overlap only.

### 3. Correct exit codes
Each job now returns a process exit code from real counts:
- **0** — success, or a clean overlap-skip.
- **3** — partial: some per-entity items failed but are retryable (the work queue retries them).
- **1** — enumeration failed for all sports, a derive stage failed wholesale (`OK==0 && Failed>0` —
  systemic, e.g. Ollama down), or dead-lettered work remains after retries.

Only `cmd/pipeline` gates on the **global** dead-letter state (it owns the queue-drain); `statcommentary`/
`vibesynth` exit codes are run-scoped (see F-033).

### 4. Graceful-shutdown settle — F-018 RESOLVED
`internal/derive`:
- `drainStage`/`DrainAll` now return a per-stage `Result` (OK/Failed/Requeued) so callers decide exit codes.
- Queue bookkeeping (`Complete`/`Fail`, and the vibe→sigil `Enqueue`) runs on a **context detached** from
  the drain (`context.Background()` + short timeout), so a successful generation is still recorded complete
  even as a graceful shutdown cancels the drain ctx — fixing the old "mark-failed failed: context canceled"
  no-op that orphaned rows.
- On shutdown, the leased-but-unprocessed batch is handed back to `pending` via the new
  `work.Requeue` (single row, status-guarded, **no attempt burned**) so the rows are immediately
  reclaimable instead of stranded `running` for the 30m stale lease.
- `cmd/api` waits (bounded 8s) on the worker goroutine's done channel before the deferred `pool.Close()`,
  so the settle actually lands before the process exits.

### 5. Dead-letter report — `cmd/work dead-letters`
Lists the two dead-letter classes an operator must act on:
- `pipeline_work` rows parked past the retry cap (`status='failed'` AND `available_at > now()+50yr` — the
  F-019/F-020 class), via the new `work.DeadLetters`.
- Fixtures stuck at the seed-retry cap (`seed_attempts >= cap`, default 3, past their seed window) that
  `get_pending_fixtures` therefore no longer selects.

### 6. Docs
The three `cron-*.sh` wrappers + `crontab.example` now document the overlap lock, the exit-code contract,
and the `pipeline_runs_latest` / `cmd/work dead-letters` operator queries.

## Decisions

- **One generic `pipeline_runs` table**, mirroring the migration-102 "one queue table" choice — not a table
  per job.
- **Advisory lock via a dedicated `pgx.Conn`** (not a pooled conn), so the session lock lives exactly as
  long as the run regardless of pool churn. Key = `hashtext('scoracle.job.'+job)` (int4→int8 implicit cast
  resolves the `bigint` overload; namespaced to stay clear of any other advisory-lock use).
- **Overlap is not an error** → exit 0 + a `skipped` run row (the schedule is visibly skipped, not silently
  missed), rather than exit non-zero.
- **Settle on a detached context** rather than threading a second context everywhere — the drain ctx governs
  Claim + the Gemma run; bookkeeping always lands.
- **Pipeline exit-1 on any dead-letter** (global queue state, not just this run) — deliberate daily nag
  until cleared (F-033).

## Verification

- `go build ./...`, `go vet ./...`, `gofmt -l` all clean.
- `work` integration tests (incl. new `TestRequeueHandsBackLeasedRow`, `TestRequeueIsStatusGuarded`,
  `TestDeadLettersReportsParkedRows`) pass against a **throwaway Postgres** (migrations 102 + 106 applied);
  migration 106 applies cleanly.
- Advisory lock validated cross-session on a throwaway PG: session A holds `scoracle.job.pipeline` → `t`;
  session B same lock → `f` (excluded); session B different lock → `t` (isolated). `pipeline_runs` +
  `pipeline_runs_latest` round-trip works.
- **F-025** prepared-statement boot (throwaway `db.New` against the live schema) → OK before restart.

## Deploy (archbox)

1. `git fetch` — in sync with `origin/main`; only S13 files dirty; `099` left untracked (parallel session).
2. Applied `106_pipeline_runs.sql` per-file (F-006) + recorded in `schema_migrations`. Verified table +
   view + index + check constraint.
3. F-025 throwaway `db.New` boot → OK.
4. Committed code (`c35e1ba`) so `release.sh` stamps a clean commit.
5. `release.sh` → built all 4 binaries @ `c35e1ba`, masked `scoracle-api.path` (F-016), restarted API,
   `/health/db` 200, serving `c35e1ba7ae2d`.
6. **F-018 post-deploy reconcile:** the shipping restart ran under the OLD (pre-fix) binary's shutdown, so
   it stranded **1** `running` row (transfers NBA team 14, frozen pre-deploy). Requeued precisely by
   timestamp (`updated_at < deploy_start`), leaving the new worker's 6 fresh post-restart claims untouched.
   This is the **last** time the manual requeue is needed — the fix is now live.
7. Verified live: `cmd/work status` + `cmd/work dead-letters` run against prod; `pipeline_runs` queryable
   (empty until the 00:00 cron writes the first row); migration recorded.

## Open follow-ups (see FINDINGS)

- **F-032** — `cmd/work dead-letters` surfaced **2 pre-fix narratives dead-letters** (FOOTBALL `team/6898`,
  `team/3513`, `{"narratives": []}`, attempts=5). The deployed `parseNarratives` returns `ok=true` for a
  clean empty array (verified), so these are pre-S11 stragglers that were never swept up. **Requeue them
  once** to let the fixed worker Complete them as markers — the targeted UPDATE was blocked by the
  deploy-mode prod-write guard, so it's left as an explicit operator action:
  ```sql
  UPDATE pipeline_work SET status='pending', attempts=0, available_at=NOW(), last_error=NULL
  WHERE stage='narratives' AND status='failed' AND available_at > NOW() + INTERVAL '50 years';
  ```
  Until cleared, the nightly `cmd/pipeline` will `exit 1` (F-033) — the machinery correctly nagging.
- **F-033** — pipeline exit-1 keys off global dead-letter state by design.
- The first real `pipeline_runs` rows land tonight: `pipeline` 00:00, `statcommentary` 03:00, `vibesynth`
  05:00 — `SELECT * FROM pipeline_runs_latest;` to confirm.

## Quick reference

```bash
# Did last night's jobs complete?
psql "$DATABASE_PRIVATE_URL" -c "SELECT * FROM pipeline_runs_latest;"

# What is permanently stuck? (pipeline_work dead-letters + fixtures at the retry cap)
go run ./cmd/work dead-letters            # optional arg: fixture retry cap (default 3)

# Outstanding derive work by stage
go run ./cmd/work status
```

## File layout (new/changed)

- `sql/migrations/106_pipeline_runs.sql` — **new** (additive table + view).
- `go/internal/jobrun/jobrun.go` — **new** (advisory lock + run record).
- `go/internal/work/work.go` — `Requeue`, `DeadLetters` (+ `DeadLetter` type).
- `go/internal/work/work_test.go` — `Requeue*` + `DeadLetters` tests.
- `go/internal/derive/derive.go` — `Result`/`StageResult`, detached settle, shutdown requeue.
- `go/cmd/api/main.go` — wait for the derive worker to settle on shutdown.
- `go/cmd/pipeline|statcommentary|vibesynth/main.go` — `jobrun.Guard` + run record + exit codes.
- `go/cmd/work/main.go` — `dead-letters` subcommand.
- `scripts/hosting/cron-{pipeline,statcommentary,vibesynth}.sh`, `crontab.example` — operator docs.
