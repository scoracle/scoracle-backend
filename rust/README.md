# scoracle-cognition — the Rust Cognition Harness

> **Naming:** this crate is the **Rust Cognition Harness** — the layer that *empowers* the
> local models. Package + binary: `scoracle-cognition` (renamed from `scoracle-scrubber` —
> the original clean-the-data framing). Canonical architecture:
> [`scoracleWiki/wiki/Architecture/Rust Cognition Harness.md`](../../../scoracle-wiki/wiki/Architecture/Rust%20Cognition%20Harness.md).

The Rust home for **all LLM derivation (cognition)** on the Scoracle platform. Post the
**Step-3 cutover (2026-06-28)**, the five Go LLM derive stages are retired into Rust:

- **5 queue stages** — `scrub` → `transfers` → `narratives` → `vibe` → `sigil` — drained
  by the long-running **`scoracle-cognition`** daemon.
- **rating** runs as the **`statcommentary`** batch bin (its own Generate loop, NOT a queue
  stage — same shape as the retired Go `cmd/statcommentary`).

The Go API serves the precomputed tables the daemon + batch write; it no longer calls the
model on a serving request, and its background derive worker is retired
(`DERIVE_WORKER_ENABLED=false`). The system architecture is
**Go ingestion → Postgres data handling → Rust empowers the models → Go serves endpoints**.
For build ledger + the L0–L13 + Step-3 history see `progress_docs/`.

## Why a separate layer (and why Rust)

The whole platform is joined at one seam: the Postgres `pipeline_work` queue. Stages read
inputs from SQL tables and write outputs to SQL tables; work is leased via
`FOR UPDATE SKIP LOCKED`. So a Rust worker drops in beside — or in place of — the Go
Drainer with **zero changes to Python, Postgres, or Go serving**, and cuts over one stage
at a time, reversibly.

Honest scope: this does **not** make orchestration faster than Go (the GPU is the throughput
ceiling). It exists to (a) own the per-role **model router** (one role per job — see `route::Role`),
(b) make the **fail-closed semantics compiler-enforced** — validity IS the type (`Option<bool>`
for `is_rumor`, etc., never a fabricated-valid row), and (c) seat future compute-bound work
on the otherwise-idle CPU — **candle embeddings**, the embedding-backed asymmetric same-name
Resolve gate, the deterministic storyline cluster.

## Layout

```
rust/
├── Cargo.toml
└── src/
    ├── main.rs              # the scoracle-cognition daemon: boots Harness, registers handlers, runs Worker
    ├── config.rs            # env config; mirrors Go var names (.env.local)
    ├── db.rs                # sqlx Postgres pool (bounded — the GPU is the real ceiling)
    ├── work.rs              # pipeline_work client: claim/complete/fail/requeue_stale/enqueue + the Stage enum
    ├── ollama.rs            # Ollama HTTP client (mirrors go/internal/ml/ollama.go)
    ├── stage.rs             # StageHandler trait — the per-stage plug-in point
    ├── worker.rs            # LISTEN(pipeline_work_ready) + safety-net drain loop
    ├── route.rs             # the model-call seam (Role → concrete model); the GPU governor lives here
    ├── harness.rs           # Harness context + the capability primitives: extract, persist, debounce, resolve, embed, cluster (Plan §1)
    ├── util.rs              # shared helpers: truncate, go_json_* (Go-encoding/json byte parity), hash_components
    ├── embed.rs             # candle CPU embedder (BGE-small default) + cosine_similarity
    ├── resolve.rs           # the asymmetric embedding-hybrid relevance gate (resolve_set + resolve_one)
    ├── scrub.rs             # news-scrub stage handler (asymmetric gate, writes news_article_entities.vetted)
    ├── transfer.rs          # transfers stage: per-(team,player) rumor vetting with the t4 prompt
    ├── narratives.rs        # narratives stage: news storyline clustering + summarization
    ├── rating.rs            # rating stage per-entity core (the cmd/statcommentary batch body)
    ├── vibe.rs              # vibe stage: the sentiment + felt-read
    ├── sigil.rs             # sigil stage: the crown convergence of rating + vibe + momentum
    └── bin/
        ├── parity.rs             # offline vibe temp-0 parity harness (writes vibe_scores_shadow)
        ├── sigil_parity.rs       # offline sigil temp-0 parity harness (writes sigil_synthesis_shadow)
        ├── rating_parity.rs      # offline rating parity harness (writes stat_summaries_shadow)
        ├── transfer_parity.rs    # offline transfers parity harness (writes transfer_rumors_shadow)
        ├── narratives_parity.rs  # offline narratives parity harness (writes news_summaries_shadow)
        ├── eval.rs               # offline A/B model eval harness (incumbent vs candidate per role)
        └── statcommentary.rs     # the rating batch bin (single / nightly / backfill over rust/src/rating.rs)
```

`work.rs` and `ollama.rs` mirror `go/internal/work/work.go` and `go/internal/ml/ollama.go` —
the live contract is the spec; keep them in sync where the Go layer still runs (e.g. for a
Step-3 rollback — see RUNBOOK.md §3).

## Build & run

```bash
cd rust
cargo build                          # debug build; the production binary goes in rust/bin/scoracle-cognition
cargo build --bin scoracle-cognition --bin statcommentary    # the two live bins only
cargo test --lib                     # the offline-testable unit gate (~80 tests; 1 ignored real-model run)
cargo clippy --all-targets -- -D warnings   # the zero-warning gate
```

Operations:

- **Prod daemon:** `scoracle-cognition.service` (systemd --user) — see
  `scripts/systemd/scoracle-cognition.service` for defaults + the `COGNITION_STAGES` line.
- **Standard release:** `scripts/hosting/release.sh` builds all 5 live binaries (3 Go +
  2 Rust) from one commit, masks the watchers, places atomically, restarts + verifies.
- **One-off deploy (without release):** `cargo build --manifest-path rust/Cargo.toml &&cp rust/target/debug/scoracle-cognition rust/bin/`; the path watcher re-arms a restart within ~1s.

Env (same names as the Go backend; loaded from your shell or `.env.local`):

`DATABASE_PRIVATE_URL` (or `DATABASE_URL`), `OLLAMA_BASE_URL`, `OLLAMA_MODEL`,
`OLLAMA_TIMEOUT_SECONDS`, `OLLAMA_MAX_CONCURRENT` (the GPU governor — 1 on the single-GPU
box), `COGNITION_DB_MAX_CONNS`, `COGNITION_SAFETY_NET_SECONDS`,
`COGNITION_STALE_LEASE_SECONDS`. Per-role + per-stage overrides:
`COGNITION_ROUTE_<ROLE>` (model override per role), `COGNITION_ROUTE_<ROLE>_CANDIDATE`
(A/B eval challenger), `COGNITION_EMBED_*`, `COGNITION_RESOLVE_{KEEP,DROP}_THRESHOLD`.
All default to a single-GPU, single-Gemma, byte-identical-to-Go config; configure to
start swapping (the path the Hardware Roadmap opens).

## The capability library (the Plan §1 primitives)

The `Harness` (`harness.rs`) is the one context handed to every stage — the pool, the
config-driven `Router` (role → model), the optional CPU `Embedder`. The six primitives are
methods on `Harness` (or free fns); the two real traits are the genuine swap points:

- **Route** (`route.rs`) — `Role` + `Inference` (the model backend; `OllamaClient` is its
  only impl today; vLLM is a future arm) + `Router`. `GovernedInference` wraps every
  backend with a shared `Semaphore` — the GPU governor sits at the seam, so it is un-bypassable.
- **Extract** (`Harness::extract`) — `route(role) → generate → Parser<T>::parse`, returning
  `Extracted<T>` with the validated value (or the fail-closed `None`) + the exact wire body
  that was POSTed (parity-proof discipline). The wire body stays single-sourced from the
  same backend the call used.
- **Persist** — `Provenance` (the moat envelope: `model_version`, `prompt_version`,
  `input_ids`, optional `input_hash`) + `Harness::debounce_unchanged` (the SkipUnchanged
  gate). Each stage keeps its typed INSERT; the envelope binds the shared fields.
- **Resolve** (`resolve.rs`) — embedding-hybrid same-name disambiguation. **Asymmetric**: the
  cheap CPU cosine may fast-track an obvious keep (`≥ keep_threshold`), but it has NO
  authority to exclude — everything below goes to the model. L4 shadow proved an
  auto-drop band loses non-redundant truth, so only the diviner excludes.
- **Embed + cluster** (`embed.rs` + `harness.rs::cluster`) — candle BGE/small on the CPU (so
  embeddings never contend with the GPU); union-find single-link clustering for storyline
  grouping. Deterministic math; never a stored derived stat.
- **Normalize** — `unimplemented!` HORIZON stub; any-language text → English.

## Stage-port recipe

Every queue stage shares the same composition shape (the reproduce-Go-at-temp-0 parity
contract — see the StageHandler trait + any of `vibe.rs` / `transfer.rs` / `rating.rs` /
`narratives.rs` / `sigil.rs` as a template):

1. **Stage constants** — `*_PROMPT_VERSION`, `*_TEMPERATURE`, `*_NUM_PREDICT`, the
   (sentence-long) `*_SYSTEM_PROMPT`. Bump the version in lockstep with the Go const when
   parity matters, only in Rust when it's a Rust single-home (transfers' `t4`).
2. **Loaders** — byte-faithful SQL ports of the Go `loadX` queries (same query ⇒ same rows;
   the parity contract). Cast `numeric` columns `::float8` for sqlx.
3. **`build_*_request` (the deterministic prefix)** — runs the loaders + assembles the user
   prompt (byte-identical) + the model options + the exact wire body (sourced from the same
   backend the call will use). Returns `Build::Skipped | Build::Ready` (the fail-closed
   marker short-circuit when there's no usable input — vibe's no-corpus, rating's no-stats).
4. **`generate_*` (the composition)** — `build_*_request` → optional SkipUnchanged debounce
   (the `*_shadow`-table `input_hash` axis when the stage debounces) → `hx.extract(role,
   prompt, opts, &Parser)` → the post-model deterministic gates.
5. **`*Parser`** — `impl Parser<T>` over the model's reply. `Ok(None)` is the post-model
   fail-closed marker (transfer's `is_rumor: Option<bool>`); some stages (vibe, rating) never
   fail-close post-model and return `Ok(Some)` on every parseable reply.
6. **`persist_*`** — typed INSERT into the live product table; the `Provenance` envelope
   binds the shared fields. Trigger the downstream hand-off (e.g. vibe enqueues `sigil`
   BEFORE its own work row completes, so a crash re-runs rather than drops).

The **offline parity bin** (`bin/*_parity.rs`) writes a `*_shadow` table row instead of
the live product, runs at temperature 0, and is the regression gate. A passing parity run
(deterministic axes: built_prompt bytes, whole `ollama_request` jsonb, model_version,
prompt_version, input_hash when the stage debounces) is the definition of "the port didn't
drift." Re-run before any change to a stage's prompt, loader, or — critically — util's
shared go_json_* / hash_components (those are the debounce pre-image).

## Operations

- **Restart after a rebuild:** the `scoracle-cognition.path` watcher fires on a close-write
  in `rust/bin/`, so `cargo build && cp rust/target/debug/X rust/bin/X` restarts the daemon
  within ~1s. Disable the watcher (`systemctl --user disable --now scoracle-cognition.path`)
  to pin a running binary while you investigate.
- **One-flag rollback (the Step-3 revert):** `DERIVE_WORKER_ENABLED=true` in `.env.local` +
  `systemctl --user restart scoracle-api.service` re-arms Go derive; `systemctl --user stop
  scoracle-cognition.service` keeps Rust off. The legacy `cron-statcommentary.sh` + Go
  `go/bin/statcommentary` binary + crontab backup are the durable rollback aid; see
  RUNBOOK.md §3.
- **Logs:** `journalctl --user -u scoracle-cognition -f` (the daemon has no HTTP probe —
  its readiness IS the systemd unit state + the journal).

## Carry / known limits

- `099_team_rosters.sql` is not ours (untracked migration owned by another contributor).
- **F-046** — a DB password sits in git history; coordination needed before any force-push.
- The Rust binaries do **not** stamp commit/build-time the way Go's LDFLAGS build does — a
  future tidy (`build.rs` reading `git rev-parse` into a `const`).
- `work::Item.entity_id` is `i32`; the article-keyed scrub stage casts to `i64`, which fits
  today but would wrap past 2bn article ids. A future widening when convenient.