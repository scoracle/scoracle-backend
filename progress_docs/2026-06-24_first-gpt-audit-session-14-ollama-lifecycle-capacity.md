# FIRST-GPT-AUDIT Session 14 — Harden Ollama/local model lifecycle and capacity

**Date:** 2026-06-24 (archbox, production)
**Commit:** `cf4f26069df6` (code) — deployed live via `release.sh`; API serving `cf4f26069df6`, `/health/db` 200.
**Migration:** none (code + `.env.local` only; next free migration stays **107**).

## Goal

Make Ollama downtime *delay* enrichment without losing work or changing truth
semantics; stop gating worker availability on a one-time API boot ping; bound all
local model work on the box by a shared governor. (FIRST-GPT-AUDIT Session 14.)

## What shipped

### 1. F-014 — worker readiness decoupled from the boot ping

- **`cmd/api`** now builds the local model generators **unconditionally** and always starts
  the derive worker (gated only on `DERIVE_WORKER_ENABLED`). The old `Ping → generators
  nil → worker+scrub disabled until restart` gate is gone. A boot probe remains but only
  **logs** ("Ollama reachable" / "unreachable … will defer"); it changes no behavior.
- **`derive.DrainAll`** reachability-**pre-gates** each cycle: if Ollama is unreachable
  it returns `Result{Deferred:true}` having claimed **nothing** and burned **no** retries.
  The durable queue is the source of truth, so pending `pipeline_work` simply drains on a
  later cycle once Ollama returns — **no API restart needed**.
- Mid-drain protection: if a generation errors and `ml.IsUnavailable(err)` classifies it
  as an outage (connection refused / DNS / dial — *not* a timeout), `drainStage`
  **requeues** the leased batch via `work.Requeue` (no attempt burned) and stops the
  stage. So an outage that begins mid-drain still can't dead-letter good work.
- **Maintenance scrub ticker** got the same pre-gate: the cheap SQL auto-vet of primaries
  still runs, but the local model disambiguation phase is skipped while Ollama is down (and
  stops mid-sweep on an `IsUnavailable` error).
- **`cmd/pipeline`** boot ping is now **non-fatal**: the RSS sweep keeps ingesting raw
  articles (durable), the derive drain defers, and the run records a **partial** outcome
  (`exit 3`) instead of `exit 1`. Raw ingestion continues; durable work accumulates;
  it drains after recovery — the audit's defined outage behavior.

### 2. Shared GPU concurrency governor

- A **process-wide** semaphore in `internal/ml` (`Setlocal modelConcurrency`, default **1**,
  env `OLLAMA_MAX_CONCURRENT`), acquired around **every** `OllamaClient.Generate`. It is
  package-level, not per-client, so the derive worker + maintenance scrub + any in-process
  local model serialize on the single 8GB card together instead of piling on and ballooning each
  call's wall time. Acquire respects `ctx` (a per-op deadline or shutdown unblocks the
  wait). `cmd/api`, `cmd/pipeline`, `cmd/statcommentary`, `cmd/vibesynth` all call
  `Setlocal modelConcurrency` at startup; un-configured binaries serialize at 1.
- **Cross-process** note: the Go gate bounds one process. Cron-vs-API-worker overlap is
  governed by Ollama's own server-side serialization (a single local-model:tag instance). The
  *explicit* cross-process governor — `OLLAMA_NUM_PARALLEL=1` + `OLLAMA_MAX_LOADED_MODELS=1`
  on the ollama systemd service — is **not** set (a root systemd change); recommended as
  an ops follow-up (F-035).

### 3. Operation-specific timeouts + keep-warm

- `OLLAMA_TIMEOUT_SECONDS` is now the **long-op budget** (narratives, `NumPredict 4000`)
  **and** the HTTP hard backstop (so a per-call context deadline always governs). New
  `OLLAMA_SHORT_TIMEOUT_SECONDS` (default **120s**) bounds scrub/vibe/sigil/transfer so
  they fail fast and retry instead of waiting the long budget. The drainer applies them
  per-stage (`local modelTimeout` for narratives, `local modelShortTimeout` for vibe/sigil); maintenance
  scrub uses the short budget; transfers stay team-scoped (`perTeamTimeout`).
- `OLLAMA_KEEP_ALIVE` (default **30m**), sent on every request, keeps local-model:tag resident
  between calls so the cold reload that blew the old flat 600s timeout is rare. Measured
  live: back-to-back calls show `load_ms ≈ 350` (warm) — a true cold load is 100s+.
- This **supersedes the F-014 600s stopgap** (`.env.local`): the same value for every op
  was the bug — scrub/vibe shouldn't wait 600s for a model that only needed it on cold load.
- Committed defaults: long **300s**, short **120s**, concurrency **1**, keep_alive **30m**.
  archbox `.env.local`: long 300, short 120, concurrency 1, keep_alive 30m.

### 4. Per-call metrics

- `OllamaClient.Generate` logs one timed line per call — `op`, `model`, `wall_ms`,
  `eval_count` on success; `op`/`wall_ms`/`reason` (`unavailable`/`timeout`/`canceled`/
  `error`) on failure. Each generator passes an `Op` label (scrub/vibe/transfer/narratives/
  sigil/rating). An operator now sees model latency in `journalctl`, not just job outcome.
  (A *durable* per-call metric — a table or `pipeline_runs` columns — was deferred; see F-036.)

### Deliberately deferred

- **Simplification A** (move the derive worker out of the API process) — bigger
  architectural change; F-014 already removes its main motivation (an API restart no longer
  disables ML until the next restart). Scope with Scott before doing it. See F-034.

## Verification

- **Build / vet / gofmt / `go test ./...`** all clean.
- New tests: `ml.TestIsUnavailable` (outage vs slow-op vs parse-error classification),
  `ml.Testlocal modelGate` / `Testlocal modelConcurrencyTwo` (bound + ctx-cancellation),
  `derive.TestDrainAllDefersWhenOllamaUnreachable` (DrainAll defers, claims nothing — no DB
  needed, the pre-gate returns before any claim), `derive.TestShortTimeoutFallback`.
- **Live, post-deploy:**
  - New process booted with `Ollama reachable model=local-model:tag max_concurrent=1
    keep_alive=30m short_timeout=2m0s long_timeout=5m0s` — all config loaded; no degraded mode.
  - F-018 confirmed again on the OLD process's shutdown: "Derive worker settled its leased
    work" + narratives `requeued=7` (leased rows handed back, not stranded).
  - Worker draining: serial `model call op=transfer wall_ms=5000–16000` lines (the governor
    keeps calls one-at-a-time); `load_ms ≈ 350` proves the model stays warm.
  - Serving stays responsive under heavy drain: `/health/db` 0.6ms, `/api/v1/nba/meta` 27ms
    (local model is off the request path).
  - Zero dead-letters; no spurious defer/unavailable events (Ollama up).
- **Outage drill (defer→recover) is proven by test**, not run live: bouncing the *system*
  ollama.service needs root and would disturb the in-flight nightly. Operator drill to run
  with sudo: `systemctl stop ollama` → watch the API log emit
  `derive: Ollama unreachable — deferring drain` (no claims, no burned attempts) →
  `systemctl start ollama` → within the 30s safety-net the worker drains the backlog. No
  API restart in between.

## Deploy notes

- Deployed **mid-nightly-run** (the 00:00 `cmd/pipeline` cron was still draining). Safe:
  the running cron process keeps its inode through the binary swap, and the API restart's
  mid-drain interruption is F-018-settled. The large live backlog (≈130 narratives / 110
  transfers / 160 vibe) is the nightly sweep's enqueued day-of-work draining under GPU
  capacity, not a regression.
- Sequence: commit `cf4f260` → `release.sh` (masks `scoracle-api.path`, builds all 4
  binaries @ `cf4f260`, restarts API, verifies `/health/db` + served commit) → verify.
- **No migration** (F-006 not exercised); `099_team_rosters.sql` left untracked; `.env.local`
  edited (gitignored, prod-local).

## Quick reference

| Knob | Env | Default | archbox |
|------|-----|---------|---------|
| Long/narratives + HTTP backstop | `OLLAMA_TIMEOUT_SECONDS` | 300 | 300 |
| Short ops (scrub/vibe/sigil/transfer) | `OLLAMA_SHORT_TIMEOUT_SECONDS` | 120 | 120 |
| In-process GPU governor | `OLLAMA_MAX_CONCURRENT` | 1 | 1 |
| Model residency | `OLLAMA_KEEP_ALIVE` | 30m | 30m |

- Outage behavior: raw ingestion continues (sweep), durable `pipeline_work` accumulates,
  no unverified output published (scrub/transfer fail-closed unchanged), drains on recovery.
- Per-call latency: `journalctl --user -u scoracle-api | grep 'model call'`.
