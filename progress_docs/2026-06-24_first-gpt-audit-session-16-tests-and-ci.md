# FIRST-GPT-AUDIT Session 16 — Focused tests + CI

**Date:** 2026-06-24 · **Machine:** archbox · **Code:** `e128d99` (this session) ·
**NO migration, NO API restart, NO deploy** (test files + a CI YAML only; next free migration stays **107**).

## Goal

Turn the launch-critical invariants S1–S15 hardened into executable checks, and add a CI
workflow that runs them on every main-bound change so the hardening cannot silently regress.
Test **behavior**, not implementation trivia. Where a past session fixed + locked a bug, ensure
a test exists that would FAIL if the fix were reverted (a regression lock) — without
reintroducing the bug.

## What shipped

### Go — offline unit tests (run everywhere, no DB)

- **`go/internal/ml/transfer_test.go`** — `parseTransferVerdict` **fail-closed (F-020)**. The two
  failure shapes a regression could reintroduce both stay OFF the served read path: unparseable
  output is `ok=false` (→ UNKNOWN/retry), and a verdict that omits `is_rumor` parses with
  `IsRumor==nil` (→ UNKNOWN, **not** a confident "cleared"). Plus `normStage` (unknown →
  conservative `speculation`, case/space normalized) and `clampConf` ([0,1]).
- **`go/internal/ml/news_scrub_test.go`** — `parseScrubRelevant`. Out-of-range indices dropped,
  an empty `{"relevant":[]}` (or `{}`) is a **valid none-relevant verdict** (the scrub analog of
  F-019's empty-narratives — not a parse failure that would dead-letter a legitimately-irrelevant
  article), and genuinely malformed output is `ok=false` so `applyVerdicts` never runs and the
  article stays unscrubbed + retryable. (The persist itself is one atomic `UPDATE` over unnest'd
  arrays — this parser is the gate that decides what lands.)
- **`go/internal/ml/sigil_test.go`** — **event-driven convergence**, the marquee lock. The pair
  `buildSynthesisInputComponents` + `hashComponents` IS the debounce key Generate compares against
  the last stored `input_hash`. Tests assert it is **deterministic and order/body-invariant**
  (identical grounding facts → identical hash → no churn), **reconverges on a real change in every
  pillar** (Rating peak/notability, a new narrative, latest sentiment/composite, the Vibe felt-read
  prose), and **debounces noise** (sub-0.1 composite jitter via `round1`; slope-only changes, since
  slopes are display-only prose). Plus `parseSynthesisResponse` (1–100 clamp, multi-line blurb
  absorption), `linearSlope`/`trendDir`, and `synthMomentum.empty()` (no-pillars → marker).

### Go — DB-gated integration (skip unless `TEST_DATABASE_URL` set)

- **`go/internal/work/work_test.go`** (extended) — **current-season Sigil reconciliation schedules
  no duplicate generation**: a re-enqueue carrying the **same** `input_version` while a row is
  `running` is a no-op (the in-flight generation finishes; no second pending row), while a **new**
  `input_version` while running reopens it to `pending`. This is the convergence-on-change /
  debounce-on-no-change guarantee at the queue layer (ON CONFLICT WHERE clause). The existing queue
  tests (dedup, SKIP LOCKED, lease recovery, requeue-on-shutdown F-018, dead-letters) already cover
  the rest.

### Python — offline seeder tests (providers mocked, no secrets)

- **`seed/tests/test_event_retry_cap.py`** — fixture **retry-cap**. `get_pending` forwards
  `max_retries` (and its documented defaults None/10000/3) to `get_pending_fixtures()` so the loop
  can't drop the cap; `record_failure` and `record_incomplete` **both advance `seed_attempts`**
  (so a permanently-broken fixture is bounded, not retried forever) while writing **separable**
  columns (`last_seed_error` vs `last_incomplete_reason`) so the two failure modes stay
  diagnosable. (The cap is enforced inside the SQL function; the Python contract is the forwarding +
  the counter advance.)

### CI — `.github/workflows/ci.yml`

Five independent jobs, on push + PR to `main`:

- **go** — `postgres:18` service; `gofmt -l` (no diffs), `go vet`, `go build`, then provision a
  test DB **from `sql/schema/schema.sql`** (`CREATE ROLE web_user` → `createdb` → load), run
  `go run ./cmd/validate-stmts` (every prepared statement registers against the schema), and
  `go test ./... -race` (offline unit + the DB-gated queue tests light up).
- **python** — `pip install -e ./seed` + pytest; `compileall`; `pytest -q` (offline).
- **shell** — `bash -n` syntax + `shellcheck -x --severity=warning` over `scripts/ sql/ *.sh`.
- **docker** — `docker build go/` (the serving image).
- **schema** — pure-repo static check: every committed `sql/migrations/*.sql` is recorded in
  `sql/schema/schema_migrations.txt` (catches a migration missing from the snapshot).

## Key decisions

- **CI test DB from the snapshot, never by replaying migrations.** Empty-DB replay fails on the
  data-dependent gates (045/046/048) per `sql/README-migrations.md`. Loading `schema.sql` gives the
  exact live schema with no replay. Proven end-to-end this session on a throwaway pg18 cluster
  (port 5599): schema loads after `CREATE ROLE web_user`, `validate-stmts` returns OK, and **all 11
  work integ tests pass** (incl. the 2 new ones).
- **`validate-stmts` is the audit's "prepared statements register against a migrated test DB."**
  Wired as a CI step; it's the kept F-025/F-039 tool (runs `db.New` → AfterConnect →
  registerPreparedStatements → Ping without starting any worker, so it never races the live drainer).
- **Offline suite stays green without Postgres.** All DB-backed Go tests gate on `TEST_DATABASE_URL`
  and skip when unset; `cd go && go test ./...` and `cd seed && pytest` remain offline. CI provisions
  Postgres and sets the URL.
- **ShellCheck at `--severity=warning -x`.** The S15 hosting scripts are already clean at warning+;
  only benign `info` nits remain (SC1091 optional-source `.env`, SC2317 indirect-invocation in
  tunnel-smoke, SC2012 `ls` in `migrate.sh`). The deploy-path scripts (`migrate.sh`, `release.sh`)
  were **not** edited — out of S16 scope and risky to touch from a test session.

## Quick reference

```bash
# Offline (no DB) — what every contributor runs locally:
cd go && gofmt -l . && go vet ./... && go test ./...
cd seed && pytest -q

# DB-gated locally (mirror CI) — throwaway pg, schema from snapshot:
initdb -U postgres -A trust /tmp/pg && pg_ctl -D /tmp/pg -o "-p 5599" -w start
psql -h 127.0.0.1 -p 5599 -U postgres -c "CREATE ROLE web_user;"
createdb -h 127.0.0.1 -p 5599 -U postgres scoracle_test
psql -h 127.0.0.1 -p 5599 -U postgres -d scoracle_test -f sql/schema/schema.sql
export TEST_DATABASE_URL="postgres://postgres@127.0.0.1:5599/scoracle_test?sslmode=disable"
cd go && go run ./cmd/validate-stmts -db "$TEST_DATABASE_URL" && go test ./...
```

## Landmines / notes for the next session

- **`schema.sql` load needs `CREATE ROLE web_user` first** — the RLS policies in the snapshot
  reference it. (Irrelevant to `build.sh`, which clones a prod that already has the role; relevant
  to any from-snapshot standup, incl. CI.)
- **`docker-compose.yml` `build: seed/` has no `seed/Dockerfile`** (pre-existing) — `docker compose
  build` would fail on the `seed` service. CI builds `go/` only (the serving artifact). Flagged as
  F-043 for a later cleanup.
- **099 + rust/ stay untracked** — `sql/migrations/099_team_rosters.sql` and the parallel Rust
  session's `progress_docs/2026-06-24_rust-cognition-library-plan.md` were left alone. The Rust
  session owns `go/internal/ml/vibe_parity_test.go`; this session added only **new** files to
  `package ml` (no edits to the parity test). Next free migration = **107**.
- **First CI run is the real proof** — the go/python/shell/schema jobs were each validated locally
  (incl. a full schema-load + validate-stmts + work-test run); `docker build` and the PGDG
  `postgresql-client-18` install run for the first time on GitHub's runners.
