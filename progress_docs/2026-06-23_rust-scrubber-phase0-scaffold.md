# 2026-06-23 — Rust scrubber layer: Phase 0 scaffold

## Goal
Stand up `scoracle-scrubber`, a new Rust service that will own the LLM-derivation
("scrubbing") layer, integrating at the existing Postgres `pipeline_work` queue
seam so it can run **alongside** — and eventually replace — the Go
`internal/ml` + `internal/derive` drainer, one stage at a time. Phase 0 ships the
reusable foundation only (queue + Ollama clients + LISTEN loop). No stage logic.

## Context / decisions
- Layer model (canonical: `scoracleWiki/wiki/Architecture/Stack Layers.md`):
  Python seeder = ingestion; Postgres = deterministic math; **Rust = all LLM
  orchestration** (new); Go = serving + news fetch. The Rust layer makes the
  **Go** derive code redundant — **not Python** (Python was never in the LLM path).
- Why Rust (honest case): one typed home for all model logic; compiler-enforced
  fail-closed semantics; the model router (Gemma/Mistral/SQLCoder per role); a
  seat for future CPU-bound work (embeddings, in-process inference). **Not** a
  speed/energy win — the LLM layer is GPU-bound, so the orchestrator language is
  a sliver of cost.
- Seam = the Postgres work queue. Rust integrates with **zero changes** to
  Python, Postgres, or Go serving; cutover is per-stage and reversible.
- Crate lives in-repo at `rust/` (monorepo) to keep the queue/Ollama contract in
  lockstep with the Go source.

## What was done
- New binary crate `scoracle-scrubber` at `rust/`. Deps: tokio, sqlx 0.8
  (runtime-tokio + postgres; no TLS feature — localhost), reqwest 0.12 (json),
  serde, async-trait, tracing.
- `work.rs` — `pipeline_work` client mirroring `go/internal/work/work.go`
  exactly: claim (`FOR UPDATE SKIP LOCKED`), complete, fail (backoff +
  dead-letter), requeue_stale, enqueue (idempotent reopen). Same SQL, same
  policy (batch 10, max 5 attempts, 30-min backoff).
- `ollama.rs` — Ollama HTTP client mirroring `go/internal/ml/ollama.go`. Same
  request/response field names → identical wire payload (the basis for the
  Phase-1 temperature-0 parity test).
- `worker.rs` — LISTEN(`pipeline_work_ready`) + safety-net drain loop +
  stale-lease recovery + graceful SIGINT. Drains only registered stages.
- `stage.rs` — `StageHandler` trait: the per-stage plug-in point. **Zero
  handlers in Phase 0.**
- `config.rs` / `db.rs` / `util.rs` / `main.rs` — env (mirrors Go var names),
  sqlx pool, helpers, entrypoint.
- Safety property: with no handlers, `tick()` short-circuits **before any
  write** — the scaffold only connects, pings Ollama, and LISTENs.

## Files
- `rust/Cargo.toml`, `rust/Cargo.lock`, `rust/.gitignore`, `rust/README.md`
- `rust/src/{main,config,db,work,ollama,stage,worker,util}.rs`

## Verification
- Rust 1.96.0 installed (rustup, user-local).
- `cargo build` → `Finished in 25.88s`, **0 errors, 0 warnings** (183 deps).
- Smoke test against the **live** DB (477-row `pipeline_work`, migration-103
  `enqueue_derive_on_vetted` trigger present, Ollama up): boots → connects to
  Postgres → pings Ollama → `LISTEN "pipeline_work_ready"` → safety-net ticks are
  no-ops → graceful SIGINT (`UNLISTEN *`). **Zero queue writes confirmed** — only
  LISTEN/UNLISTEN emitted, no INSERT/UPDATE/DELETE.

## Result
Phase 0 foundation verified and committed. The queue/Ollama clients and the
drain loop are real and exercised; the only missing piece is per-stage logic.

## Next (Phase 1)
- Implement the **vibe** stage handler (the emotional-interpretation stage —
  where Mistral routing lands, and the simplest to parity-test).
- Add a shadow table (migration **105**) + a **temperature-0 parity harness**
  diffing Rust vibe output against the Go vibe stage.
- Align `SCRUBBER_STALE_LEASE_SECONDS` with Go `derive.StaleLease` before any
  shared-queue run.
- Coordinate the migration number with the parallel session (099 currently
  untracked; next free per memory = 105).
