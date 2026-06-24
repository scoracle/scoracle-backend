# scoracle-cognition — the Rust Cognition Harness

> **Naming:** this crate is the **Rust Cognition Harness** — the layer that *empowers* the local models. Package + binary: `scoracle-cognition` (renamed from `scoracle-scrubber` — the original clean-the-data framing). Canonical architecture: [`scoracleWiki/wiki/Architecture/Rust Cognition Harness.md`](../../../scoracleWiki/wiki/Architecture/Rust%20Cognition%20Harness.md).

The Rust home for all LLM derivation (cognition). It is a durable `pipeline_work`
queue consumer + Ollama client, wired to a LISTEN/NOTIFY drain loop — the **host**
the per-stage handlers plug into. **Status:** Phase 0 host + Phase 1 `vibe` handler
(temp-0 parity proven) shipped; the direction now is **library-first** — build the
capability library, then re-express `vibe` as its first composition (not a
parity-port of all five Go stages). The older phased plan is
[`scoracleWiki/raw/scoracle-rust-scrubber-implementation-plan.md`](../../../scoracleWiki/raw/scoracle-rust-scrubber-implementation-plan.md)
(superseded on *sequencing*; its integration contract still holds).

## Why a separate layer (and why Rust)
The whole platform is joined at one seam: the Postgres `pipeline_work` queue.
Stages read inputs from SQL tables and write outputs to SQL tables; work is
leased via `FOR UPDATE SKIP LOCKED`. So a Rust worker drops in beside the Go
Drainer with **zero changes to Python, Postgres, or Go serving**, and cuts over
one stage at a time, reversibly.

Honest scope: this does **not** make orchestration faster than Go (the GPU is
the throughput ceiling). It exists to (a) own the per-role **model router**
(Gemma for stats/logic, candidate Mistral for emotional news), (b) make the
fail-closed semantics compiler-enforced, and (c) seat future compute-bound work
(embeddings/clustering, eventual in-process inference). See the plan's §1.

## Layout
```
rust/
├── Cargo.toml
└── src/
    ├── main.rs      # entrypoint; registers stage handlers (none in Phase 0)
    ├── config.rs    # env config; mirrors Go var names (.env.local)
    ├── db.rs        # sqlx Postgres pool
    ├── work.rs      # pipeline_work client: claim/complete/fail/requeue_stale/enqueue
    ├── ollama.rs    # Ollama HTTP client (mirrors go/internal/ml/ollama.go)
    ├── stage.rs     # StageHandler trait — the per-stage plug-in point
    ├── worker.rs    # LISTEN(pipeline_work_ready) + safety-net drain loop
    └── util.rs      # shared helpers
```

`work.rs` and `ollama.rs` mirror `go/internal/work/work.go` and
`go/internal/ml/ollama.go` — the live contract is the spec; keep them in sync.

## Build & run
```bash
cd rust
cargo build              # NOT yet verified on this machine — Rust toolchain absent
cargo run                # reads DATABASE_PRIVATE_URL/DATABASE_URL + OLLAMA_* from env
```
Install Rust if needed: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`.

Env (same names as the Go backend; loaded from your shell / `.env.local`):
`DATABASE_PRIVATE_URL` (or `DATABASE_URL`), `OLLAMA_BASE_URL`, `OLLAMA_MODEL`,
`OLLAMA_TIMEOUT_SECONDS`, plus `COGNITION_DB_MAX_CONNS`,
`COGNITION_SAFETY_NET_SECONDS`, `COGNITION_STALE_LEASE_SECONDS`.

## Phase 0 safety property
With **no handlers registered**, the worker connects, pings Ollama, LISTENs, and
**performs zero queue writes** (`tick` short-circuits before `requeue_stale`/
drain). Safe to run against any DB while reviewing the foundation.

## Status & caveats
- ⚠️ **Not yet compiled** — `cargo`/`rustc` were not installed in the authoring
  environment. Run `cargo build` to verify. Two lines are the likely first-build
  snags (both flagged in `Cargo.toml`): the **sqlx** runtime/TLS feature set and
  the **reqwest** feature set.
- `#![allow(dead_code)]` is set in Phase 0 (the client API is complete but not
  all called until handlers land). Remove it with the first handler.
- TODO before any shared-queue run: align `COGNITION_STALE_LEASE_SECONDS` with the
  Go `derive.StaleLease` value (see `config.rs`).

## Next (library-first)
Phase 1 `vibe` is done (temp-0 parity proven). The next move is **not** porting the
remaining four Go stages to parity — it is building the **capability library**
(route · resolve · extract+validate · embed+cluster · normalize · persist) and
re-expressing `vibe` as its first composition. Every later stage is then a
composition + a prompt + a role. See
[`Rust Cognition Harness`](../../../scoracleWiki/wiki/Architecture/Rust%20Cognition%20Harness.md).
