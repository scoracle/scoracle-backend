# 2026-06-23 — Rust scrubber layer: Phase 1 (vibe handler + temp-0 parity)

## Goal
Implement the first stage handler in `scoracle-scrubber` — **vibe** — and PROVE it
matches the Go vibe stage byte-for-byte at temperature 0, all offline against a shadow
table. No change to the live read path: Go's `drainVibe` stays the vibe owner until the
per-stage cutover (Phase 2). Builds on the Phase 0 scaffold (commit `6d2c3a5`).

## Context / decisions
- **Parity is provable because temp 0 is deterministic.** Verified empirically up front:
  `gemma4:e4b` with an EXPLICIT `temperature: 0` returns byte-identical output across
  calls. So a byte-identical prompt + identical options ⇒ identical SCORE/VIBE; any diff
  is a port bug (SQL read, prompt string, options, or parse), not model noise.
- **The temperature landmine.** BOTH the Go and Rust Ollama clients omit `temperature`
  when `<= 0`, which silently un-pins temp-0 to Ollama's ~0.8 default (non-deterministic).
  Fixed in Rust by making `GenerateOptions.temperature` an `Option<f64>` (`Some(0.0)` is
  sent, `None` omits); the Go parity side POSTs an explicit `temperature: 0` directly.
- **The harness writes ONLY the shadow table.** It never INSERTs `vibe_scores`, never
  claims/enqueues `pipeline_work`, never runs the Go stage. Confirmed by code + a live
  check (0 `vibe_scores` rows for the parity entities during the run).
- **Crate restructured to lib + bins.** Added `src/lib.rs` so the service binary
  (`src/main.rs`) and the standalone parity harness (`src/bin/parity.rs`) share the same
  code. Making the foundation a library also means its `pub` items are no longer "dead",
  so the crate-level `#![allow(dead_code)]` is gone with a clean build.
- **`StaleLease` aligned.** `SCRUBBER_STALE_LEASE_SECONDS` default 600 → 1800 to match Go
  `derive.StaleLease` (30 min) before any shared-queue run.

## What was done
- `rust/src/vibe.rs` (new) — the vibe core + `VibeHandler`, a faithful port of
  `go/internal/ml/vibe.go` + `transfer_heat.go`:
  - `load_latest_narratives` / `load_transfer_heat` / `lookup_entity_name` — the same SQL.
  - `build_sentiment_prompt` — byte-identical to `buildSentimentPrompt` (incl.
    `strings.Title`, the byte-based 280-char body truncation, the em-dash heat lines).
  - `VIBE_SYSTEM_PROMPT` — verified byte-identical to Go (897 bytes, sha `b553eaf1…`).
  - `parse_sentiment_and_prompt` / `parse_sentiment` — mirror the SCORE/VIBE two-line
    parse + first-integer fallback + 1-100 clamp.
  - `VibeHandler::handle` — read → score (temp 0.7) → write `vibe_scores` → enqueue the
    `sigil` convergence BEFORE completing (mirrors drainVibe's hand-off), with the
    fail-closed no-corpus NULL marker. 6 unit tests for the parser/prompt builder.
- `rust/src/ollama.rs` — `temperature: Option<f64>`; `build_request` (single source of
  truth) + `request_body()` so the harness records the EXACT body sent.
- `rust/src/lib.rs` (new), `rust/src/main.rs` — lib target; main registers
  `VibeHandler`, drops the crate-level `#![allow(dead_code)]`.
- `rust/src/bin/parity.rs` (new) — the offline harness: runs the same core at explicit
  temp 0 over a set of entities, writes `source='rust'` rows (+ exact prompt + request
  jsonb) to `vibe_scores_shadow`.
- `rust/src/config.rs`, `rust/Cargo.toml` — stale-lease alignment; `[lib]` + `parity` bin.
- `sql/migrations/105_vibe_scores_shadow.sql` (new) — the shadow/diagnostic table
  (mirrors `vibe_scores` + `source`/`temperature`/`built_prompt`/`ollama_request`), no FK
  or trigger so it can't perturb live data.
- `go/internal/ml/vibe_parity_test.go` (new) — env-gated (`VIBE_PARITY_DB`) Go side of the
  proof: reuses the package's unexported loaders/prompt/parse, calls Ollama at explicit
  temp 0, writes `source='go'` rows. Writes ONLY the shadow table; skipped in normal `go test`.

## Files
- `rust/src/vibe.rs`, `rust/src/lib.rs`, `rust/src/bin/parity.rs` (new)
- `rust/src/{main,ollama,config}.rs`, `rust/Cargo.toml` (edited)
- `sql/migrations/105_vibe_scores_shadow.sql` (new)
- `go/internal/ml/vibe_parity_test.go` (new)

## Verification
- `cargo build` → Finished, **0 errors / 0 warnings**; `cargo test --lib` → **6/6 pass**.
- `gofmt -l` clean, `go vet ./internal/ml/` clean, parity test compiles + skips by default.
- System prompt byte-equality (Go `vibeSystemPrompt` vs Rust `VIBE_SYSTEM_PROMPT`):
  identical, 897 bytes, sha256 `b553eaf1ded5feb1…`.
- **Temp-0 parity run** over 4 entities (3 with corpus — NFL player, two FOOTBALL teams —
  + 1 no-corpus NBA player), both implementations at explicit temp 0 into `vibe_scores_shadow`:

  | entity | rust score | go score | score_eq | vibe_eq | prompt_eq | request_eq |
  |---|---|---|---|---|---|---|
  | team/625 FOOTBALL | 58 | 58 | ✓ | ✓ | ✓ | ✓ |
  | player/13874268 NFL | 70 | 70 | ✓ | ✓ | ✓ | ✓ |
  | team/597 FOOTBALL | 82 | 82 | ✓ | ✓ | ✓ | ✓ |
  | player/1 NBA | NULL (marker) | NULL (marker) | ✓ | ✓ | ✓ | ✓ |

  **4/4 on every axis** — SCORE identical, VIBE identical, built prompt byte-identical
  (octet_length 412/1429/1890 equal), Ollama request body jsonb-identical, model
  `gemma4:e4b`, prompt_version `v6`. Including the fail-closed no-corpus NULL marker.
- **Latency**: Rust 111.5s vs Go 112.6s for the same 3 model calls (~1%, well within the
  ~10% bar). Orchestration is I/O-bound; the GPU eval dominates either language.
- **Safety**: harness wrote only `vibe_scores_shadow` (8 rows: 4 rust + 4 go); live
  `vibe_scores` got 0 writes for the parity entities; `pipeline_work` untouched.

## Result
Phase 1 done and proven. The Rust vibe stage is a byte-for-byte match for the Go stage at
temp 0; the pattern (shadow → temp-0 parity → per-stage cutover) is validated end-to-end
and every later stage is the same loop. `VibeHandler` is registered in `main.rs` but is
NOT to be run against the live DB until the Phase 2 cutover (it would double-claim `vibe`
and burn the GPU twice while Go still owns the stage).

## Landmines hit / notes
- **`099_team_rosters` swept up.** `099_team_rosters.sql` was on disk but untracked and
  unrecorded in `schema_migrations` (a parallel session's WIP). Running `migrate.sh` to
  apply 105 also applied 099 (it applied cleanly under `ON_ERROR_STOP=1` and created
  `public.team_rosters`). It is now recorded; the file stays untracked (not ours to
  commit). Surface to the parallel-session owner.
- Not committed — left staged for review per the no-auto-commit rule. Suggested message:
  `feat(scrubber): Phase 1 vibe handler + temp-0 parity (Rust scrubber)`.

## Next (Phase 2)
- Promote vibe to the live queue: disable Go's `drainVibe`, let Rust claim real `vibe`
  items (`FOR UPDATE SKIP LOCKED` makes the cutover mutually exclusive). Keep `drainVibe`
  flag-gated for instant rollback. Soak 48–72h (queue depth, fail rate, sigil enqueue).
- Then port narratives → transfers → sigil via the same shadow/temp-0/cutover loop.
