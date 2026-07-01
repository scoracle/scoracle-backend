# 2026-06-24 — Rust Cognition Harness: L0 (capability-library scaffold) + L1 (vibe re-expressed)

## Goal
Build the FIRST increment of the **library-first** Cognition Harness plan
(`scoracleWiki/wiki/Plan - Rust Cognition Harness build.md`): stand up the capability-library
floor (**L0**) and re-express the proven `vibe` stage as a composition of those primitives
(**L1**), stopping at the temp-0 parity gate. Vibe is the **fixture, not the deliverable** —
same bytes out at temp 0, but now the primitives (route · extract · persist) exist and are
tested, and the floor is *shaped* for resolve · embed · normalize. Builds on the Phase 0/1
host (commit `e128d99` lineage); **the host loop is untouched**.

The binding discipline (carried from the conception): library-first not parity-port; address
models **by role, never by name**; deterministic math stays in Postgres; **Rust touches a row
only to make a model smarter about it**; fail-closed lives in the type system.

## Context / decisions
- **One context object + two real traits, not six dyn-primitives.** `Harness { pool, router,
  embedder }` is the capability context; the primitives are *methods* on it. The only two
  genuine swap points are traits: **`Inference`** (the model backend) and **`Parser<T>`** (the
  per-stage output plug-in). Everything else is concrete — the *models* and *parsers* swap, not
  the primitives.
- **`Inference` extracted over `OllamaClient` without touching `ollama.rs`.** The trait's three
  methods (`generate` / `model` / `request_body`) are exactly `OllamaClient`'s inherent methods,
  so `impl Inference for OllamaClient` (in `route.rs`) is a thin delegation. `ollama.rs` is
  byte-unchanged — the struct stays, the trait wraps it.
- **L1 router is deliberately minimal.** `Router::single(Arc<dyn Inference>)` → every `Role`
  resolves to the one configured local model. Enough for vibe to route `EmotionalNews → local model`
  byte-identically. L2 swaps `single` for the config-driven `from_config`/`candidate_for` map
  (`COGNITION_ROUTE_*` + the A/B eval) **without `for_role`'s contract — or any stage — moving**.
- **`extract` captures the EXACT wire body it sent.** `Harness::extract` sources
  `Extracted.request_body` from the same `Inference::request_body(prompt, opts)` the call used —
  the recorded request can never drift from the POSTed one (the property the Phase-1 proof
  leaned on). `VibeOutput` now carries that body, so the parity harness reads it directly
  instead of recomputing (one fewer drift surface; `parity_opts` deleted).
- **Fail-closed stays exactly where it was.** `VibeParser` wraps `parse_sentiment_and_prompt`:
  it never returns `Ok(None)` — vibe's only fail-closed path is the **pre-model no-corpus
  short-circuit** (the NULL marker), so an unparseable reply is a genuine `Err` → the work item
  backs off, identical to the Go stage. The `Parser<T>` contract (`Some`=valid · `None`=marker ·
  `Err`=retry) is shaped for transfer/scrub's `Ok(None)` JSON path later.
- **Persist routes through the `Provenance` envelope, keeps the typed INSERT.** `VibeOutput::
  provenance()` lifts `model_version` / `prompt_version` / `input_ids` (`input_hash: None` —
  vibe doesn't debounce) into the shared envelope; the stage's own `INSERT INTO vibe_scores`
  binds from it (Postgres-as-serializer, no generic row-writer).
- **`debounce_unchanged` is shipped REAL and season-aware** (its first consumer, sigil, is
  season-scoped — `sigil_synthesis (entity,sport,season)` per `sigil.go::lastSynthesisHash`). A
  NULL latest hash compares unequal to any real hash, so a marker never wrongly skips. Vibe
  doesn't call it; it's real for sigil (HORIZON).
- **Resolve / Embed / Normalize are shaped stubs** — real signatures + types (`Candidate`,
  `IdentityCard`, `EntityType`, `Resolved`, `Resolution`, `Embedder`, `Vector`, `Cluster`,
  `NormalizedText`, `RawMention`), `unimplemented!()` bodies. The floor is drawn; no
  infrastructure built on speculation.
- **The host loop is inviolate.** `StageHandler::handle` changed `(pool, ollama, item)` →
  `(&Harness, item)`; `Worker` swapped its `ollama` field for a `harness` field (and clones
  `harness.pool` for its own claim/complete/fail/LISTEN mechanics, which stay **byte-identical**).
  The only edit to the proven drain loop is the single `handle` callsite. `work.rs` untouched.

## What was done
- `rust/src/route.rs` (**new**) — the Route primitive: `Role` enum (job, not name; `Hash` for
  the L2 map); `Inference` trait + `impl Inference for OllamaClient` (delegation); minimal
  `Router` (`single` / `for_role`).
- `rust/src/harness.rs` (**new**) — the capability library: `Harness` context; **Extract**
  (`Parser<T>`, `Extracted<T>`, `Harness::extract` — REAL); **Persist** (`Provenance`,
  `EntityKey`, `Harness::debounce_unchanged` — REAL, season-aware); shaped stubs for **Resolve**
  (`resolve_one`/`resolve_set` + types), **Embed** (`embed` + `cluster` fn + `Embedder`/`Vector`/
  `Cluster`), **Normalize** (`normalize` + `NormalizedText`/`RawMention`).
- `rust/src/vibe.rs` (edited) — re-expressed as `route(EmotionalNews) + extract(VibeParser) +
  persist`: added `VibeReply` + `VibeParser` (wraps the byte-identical parse); `generate_vibe`
  now takes `&Harness` and runs `hx.extract(...)`; no-corpus `model_version` from
  `router.for_role(EmotionalNews).model()`; `VibeOutput` gains `request_body` +
  `provenance()`; `persist_to_vibe_scores` binds from the `Provenance` envelope; `VibeHandler::
  handle(&Harness, item)`; **sigil enqueue-before-complete + no-corpus NULL marker unchanged**.
  +2 unit tests for `VibeParser`.
- `rust/src/stage.rs`, `rust/src/worker.rs`, `rust/src/main.rs`, `rust/src/lib.rs` (edited) —
  the signature change + the single `Worker` callsite + harness construction at boot + the two
  new modules. The drain loop body is unchanged.
- `rust/src/bin/parity.rs` (edited) — builds the same `Harness` (router over the pinged client);
  `run_one(&Harness, …)` reads the captured `out.request_body` (no recompute); `parity_opts`
  removed. Shadow-only safety property preserved (never invokes `VibeHandler`; `generate_vibe`
  doesn't persist live).

## Files
- **New:** `rust/src/route.rs`, `rust/src/harness.rs`, `progress_docs/2026-06-24_rust-cognition-harness-L0-L1.md`
- **Edited:** `rust/src/{vibe,stage,worker,main,lib}.rs`, `rust/src/bin/parity.rs`
- **Untouched (inviolate / pre-existing):** `rust/src/{work,ollama,config,db,util}.rs` (the
  whole-crate `cargo fmt` wanted line-wraps on the pre-existing `work.rs`/`ollama.rs`; reverted
  so the commit stays surgical — those two carry pre-existing rustfmt drift, not ours).

## Verification
- `cargo build` → Finished, **0 warnings**. `cargo test --lib` → **8/8 pass** (6 existing + 2
  new `VibeParser`). `cargo clippy --all-targets -- -D warnings` → clean. `cargo fmt --check` →
  clean on all session files (only the pre-existing `ollama.rs`/`work.rs` flag, left as-is).
- **Host drains nothing new:** the service binary was NOT run; the drain loop is structurally
  unchanged (only the `handle` callsite + how the harness is held).
- **Temp-0 parity GATE — re-passed 4/4 including the no-corpus marker.** Rust harness
  (`source='rust'`, the new route+extract+persist path) vs Go `TestVibeParityDump`
  (`source='go'`), same entities, both at explicit temp 0, self-join on `vibe_scores_shadow`:

  | entity | rust | go | score | vibe | prompt bytes | request (jsonb) | model | prompt_ver |
  |---|---|---|---|---|---|---|---|---|
  | player/1 NBA | NULL | NULL | ✓ | ✓ | ✓ (marker) | ✓ | ✓ | ✓ |
  | player/13874268 NFL | 70 | 70 | ✓ | ✓ | ✓ (412/412) | ✓ | ✓ | ✓ |
  | team/597 FOOTBALL | 68 | 68 | ✓ | ✓ | ✓ (1722/1722) | ✓ | ✓ | ✓ |

  SCORE identical, VIBE sentence identical, built-prompt byte-identical, Ollama request body
  jsonb-identical, model `local-model:tag`, prompt_version `v6` — including the fail-closed no-corpus
  NULL marker. **The bytes did not move: the refactor preserved the stage exactly.**
- **Safety:** the parity harness wrote only `vibe_scores_shadow`; it never touched `vibe_scores`
  or `pipeline_work` (it never invokes `VibeHandler`, and `generate_vibe` does not persist). The
  service binary was not run, so the live `vibe` stage stayed owned by Go's `drainVibe`.

### Reproduction (the gate)
```bash
cd scoracle-backend && export PATH="$HOME/.cargo/bin:$PATH"
export DATABASE_PRIVATE_URL=…           # from .env.local (postgresql://…@localhost:5432/scoracle)
export OLLAMA_BASE_URL=http://localhost:11434 OLLAMA_MODEL=local-model:tag OLLAMA_TIMEOUT_SECONDS=300
cargo build --manifest-path rust/Cargo.toml
./rust/target/debug/parity team:597:FOOTBALL player:13874268:NFL player:1:NBA   # source='rust'
( export VIBE_PARITY_DB=1 VIBE_PARITY_ENTITIES="team:597:FOOTBALL player:13874268:NFL player:1:NBA"
  go -C go test ./internal/ml/ -run TestVibeParityDump -v -count=1 -timeout 25m )  # source='go'
# diff: DISTINCT ON (source,entity) latest, self-join source='rust' vs 'go' on
#       (entity_type,entity_id,sport) — score/vibe/built_prompt/ollama_request all equal.
```

## Result
L0 + L1 done and proven. The capability library exists (`route.rs` + `harness.rs`), `vibe` is
re-expressed as `route + extract + persist`, and the temp-0 parity gate is re-passed 4/4 — the
refactor moved zero bytes. The target has stopped moving: every later stage is now a recipe over
these primitives, not new infrastructure. `VibeHandler` remains registered in `main.rs` but is
**NOT to be run against the live DB** until the per-stage cutover (it would double-claim `vibe`
and burn the GPU twice while Go still owns the stage).

## Landmines / notes
- **`cargo fmt` is whole-crate.** It wanted line-wraps on the pre-existing `work.rs`/`ollama.rs`
  (both outside this work; `work.rs` is *inviolate* per the plan). Reverted to keep the diff
  surgical — those two retain their pre-existing rustfmt drift; not ours to fix here.
- **A parallel session committed the docs.** The `CLAUDE.md`/`ENDPOINTS.md`/`README.md` mods
  present at session start landed as commit `a2038a1` (S17 docs) during this session — already on
  `origin/main`, not ours. Left untouched.
- **`099_team_rosters.sql` still untracked** — a parallel session's WIP, not ours to commit.
- **`vibe_scores_shadow` accrues rows** (no unique key) — the diff uses `DISTINCT ON … id DESC`
  to compare the latest run. The table is throwaway diagnostic; drop it after the vibe cutover.

## Next (L2 — per the plan §3 build order)
- **Stand up the Router properly:** `RouteConfig`/`ModelSpec` + `Router::from_config` reading
  `COGNITION_ROUTE_*` (every role still → local model today, byte-identical) + `candidate_for`.
- **`bin/eval.rs`** — the A/B eval hook (labeled set through incumbent vs candidate, print the
  delta; the router NEVER auto-promotes — a human edits `COGNITION_ROUTE_*` on a measured win).
- Then every later stage is additive composition (rating → narratives → transfers → sigil →
  scrub → stat_resolve / multilang), each via shadow → temp-0 parity → per-stage cutover, with
  the Go drain flag-gated for instant rollback.
