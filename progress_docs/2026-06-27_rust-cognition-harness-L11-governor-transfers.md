# Rust Cognition Harness — L11: GPU governor + transfers port (Cutover Step 2, part 1)

**Date:** 2026-06-27
**Plan:** vault `Plan - Rust Cognition Harness build.md` → "The Cutover Plan" (Step 2) + §7 ledger (L11)
**Status:** DONE — the GPU governor is built, and the **transfers** stage is ported OFFLINE into Rust
with the **t4** single-home prompt fix, parity-gated 16/16 on the deterministic axes. No live impact:
the offline parity/shadow bins write only the shadow table; the `TransferHandler` is registered but
NOT enabled (Go still owns the live transfers stage until its own cutover, Step 3).

## Goal

Execute Cutover Step 2 (scoped this increment to **governor + transfers**, per the one-verified-thing
-per-L cadence; rating/narratives are L12/L13). Build the Rust GPU governor (the prerequisite the plan
names), port the highest-value stage (transfers — it carries the L9 false-heat root fix), and prove
machinery parity with Go offline before any cutover.

## Accomplishments

### 1. The GPU governor — `GovernedInference` (route.rs)

- A **decorator over the `Inference` swap-seam**: `Router::from_config` builds ONE shared
  `tokio::sync::Semaphore(OLLAMA_MAX_CONCURRENT)` and wraps every backend it constructs in
  `GovernedInference`, which acquires a permit before `generate` (and delegates `model`/`request_body`
  un-gated — those are pure/local, no GPU). One GPU → one budget, shared across all roles/models.
- **Why the seam, not the worker:** every model call funnels through `for_role(_).generate` (extract +
  resolve both), so governing the backend makes the bound **un-bypassable** — no caller can sneak a
  GPU call past it (a check in one handler could be forgotten). The worker's sequential drain is
  already an implicit 1; this makes it explicit so a brief Go+Rust transition overlap (Go's own
  `gemmaGate` + the Rust daemon) and any future parallel drain stay bounded.
- Config: `ollama_max_concurrent` reads `OLLAMA_MAX_CONCURRENT` (the SAME var Go's gemmaGate reads),
  default 1, clamped ≥1. Production (main.rs) passes `cfg.ollama_max_concurrent`; the offline bins pin
  1 (single-flight).
- Tests: `governor_serializes_with_one_permit` (5 concurrent calls, 1 permit → peak 1) and
  `governor_allows_exactly_the_budget` (2 permits → peak 2) — deterministic on the current-thread test
  runtime (the sleep yields, so all permit-holders contend).

### 2. The transfers port — `transfer.rs` (the t4 single-home fix)

Faithful port of `go/internal/ml/transfer.go` at the **team→pair** grain, composed as
`build_pair_request` (deterministic) → `extract`+validate (fail-closed `Option<bool>` is_rumor, JSON
mode) → the gates → persist. The **deterministic stays SQL/Postgres**: `compute_transfer_heat` (the
number), the team relationship, and `direction` — the model never computes them. The SQL loaders
(`load_candidates`, `compute_pair_heat`, `load_pair_news`, `team_relationship`, `load_tier_map`) are
**verbatim** Go (same query ⇒ same rows); `build_transfer_prompt` is **byte-identical** to Go's
`buildTransferPrompt` (the `·` U+00B7 and `—` U+2014 are significant bytes; `truncate_bytes` mirrors
Go's byte-slice `truncate`). The former-player return-signal gate + the grounding-guard
(confidence ×0.5 when no tier-1/2 source) are reproduced.

**The t4 prompt (the single-home change — the L9 false-heat root fix):** Go stays frozen at `t3` (to
be retired at cutover); Rust ships `t4` = t3 **+ the roundup/listicle clause** (a name in a
multi-subject roundup / notes column / power ranking / listicle is NOT a live rumor) **+ strengthened
never-invent-a-fee** (state a fee/bid/figure/stage ONLY when the sources give it — no fabrication, no
stage upgrade) **+ an explicit stage-evidence ladder**. This is the exact L9 root: mistral t3 confirmed
an "AFC Notes" roundup as `concrete_interest` + a fabricated $50m bid → false heat that fed the
narratives draft a phantom. Authored ONCE, in Rust.

The **subject same-person test** is realised as the verdict's `subject` field + the t4 identity-card
framing (the model returns is_rumor AND subject in ONE JSON, exactly as Go does). The standalone
embedding-backed `resolve_one` for transfers is a documented **HORIZON** — it would split the one fused
call into two (doubling per-pair GPU) and break Go-machinery parity, so it waits (Plan §1.3 "an
improvement, not parity").

`TransferHandler` is **registered in main.rs but NOT enabled** (gated on `COGNITION_STAGES` containing
`transfers`; archbox stays scrub-only) — Step-3-ready, no double-claim of the one GPU.

14 unit tests: 3 byte-fixture `build_transfer_prompt` tests (current/former/none × identity/news
variants, asserting the exact Go-matching bytes), the `TransferParser` fail-closed contract, the
`row_from_verdict` branching (UNKNOWN keeps direction/drops model; cleared drops direction/keeps
model; rumor sets all), `norm_stage`/`clamp_conf`/`has_return_signal`, and a t4-clause presence check.

### 3. The parity gate — 16/16 on the deterministic axes (the L2 finding)

- **mig 110 `transfer_rumors_shadow`** — the throwaway diagnostic (per-pair key; no FK/trigger),
  applied **surgically** (`psql --single-transaction` + ledger INSERT for ONLY 110, never sweeping the
  untracked 099).
- **`bin/transfer_parity`** (source='rust') + **`go/internal/ml/transfer_parity_test.go`**
  (`TestTransferParityDump`, source='go') — both dump the deterministic axes (built_prompt,
  ollama_request, model_version, prompt_version) for the SAME pairs (`load_candidates` → sort by
  player_id → take N, a bin-only cap that makes the two runs cover identical pairs regardless of the
  loader's tie order; the production loader stays byte-identical to Go).
- **NO model call for the gate** (the L2 finding): the parity axes are all deterministic — the verdict
  is not a temp-0 parity axis — so the gate is GPU-free and fully deterministic.
- **Result (16 pairs across Knicks / Liverpool / Arsenal / Lakers): 16/16** `built_prompt` byte-equal,
  16/16 `(ollama_request − system)` jsonb-equal, 16/16 `system` diverges (t3 vs t4 — the ONE intended
  difference), 16/16 model_version equal, 16/16 direction equal, 16/16 prompt_version t4-vs-t3.
- **Vet run** (`TRANSFER_PARITY_VET=1`, live mistral, 2 pairs) validated the FULL path end-to-end
  through the governor: Rice→Cleared, Jesus→Rumor (`concrete_interest`, £20-25m fee **from the source
  headline** attributed to "Gooner Daily", confidence halved by the grounding guard — fee stated only
  because sourced, never invented: t4 working as designed).

**Gate — PASSED.** `cargo build` 0 warnings · `cargo clippy --all-targets -- -D warnings` clean ·
`cargo test --lib` 51 passed (1 ignored real-model) · `gofmt`/`go vet` clean on the new Go test.

## Landmines (carry)

- **The transfers parity diff must be a TIGHT back-to-back run** (or a frozen snapshot). The scrub
  cutover is LIVE (`NEWS_SCRUB_VIA_QUEUE=true`), so the 14-day co-mention corpus mutates: a wider gap
  between the rust and go dumps shows a spurious single-pair news-set skew (the first run was 15/16 —
  the Salah×Liverpool pair gained/lost one headline between dumps; a same-second re-run was 16/16).
  This is NOT an assembly bug (build_transfer_prompt is deterministic given inputs; the offline
  fixtures + 15/16 prove it) — it is live-input variance. vibe/sigil had a more stable per-entity
  corpus; transfers' co-mention + live-scrub window makes it move.
- **`confidence` is `numeric(3,2)` in transfer_rumors** — the production persist binds it `$12::float8
  ::numeric` (sqlx has no numeric encode without the decimal feature — the dual of the scrub `::float8`
  READ landmine). The shadow uses `real` (display-only diagnostic). The production persist is written
  but does NOT run this session (offline) — it compiles; its first live run is the Step-3 cutover.
- **`TransferHandler` is registered, NOT enabled.** Enabling it (adding `transfers` to
  `COGNITION_STAGES`) before retiring Go's drainTransfers would double-claim the queue + burn the one
  GPU twice. That is the Step-3 cutover, not this session.
- `099_team_rosters.sql` still untracked (not ours). **F-046 still OPEN** (DB password in git history;
  a purge rewrites the cognition commits incl. this one — coordinate before any force-push).
- archx220 still lacks `mistral:7b` (`ollama pull mistral:7b`) — the one cross-machine step, not doable
  from archbox.

## Quick reference

```bash
# Build + the gate:
cargo build --manifest-path rust/Cargo.toml
cargo clippy --manifest-path rust/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path rust/Cargo.toml --lib

# Transfers parity (env from .env.local: DATABASE_PRIVATE_URL + OLLAMA_MODEL):
./rust/target/debug/transfer_parity team:20:NBA team:8:FOOTBALL   # source='rust' (t4), deterministic
( cd go && TRANSFER_PARITY_DB=1 TRANSFER_PARITY_TEAMS="team:20:NBA team:8:FOOTBALL" \
    go test ./internal/ml/ -run TestTransferParityDump -count=1 )   # source='go' (t3)
# RUN THEM BACK-TO-BACK (the live scrub mutates the co-mention corpus), then the diff self-join on
# transfer_rumors_shadow: built_prompt byte-equal + (ollama_request - 'system') equal; system = the t4 divergence.

# Full-path vet (live mistral): TRANSFER_PARITY_VET=1 TRANSFER_PARITY_MAX_PAIRS=2 ./rust/target/debug/transfer_parity team:19:FOOTBALL
```

## File layout delta

```
rust/src/config.rs                    + ollama_max_concurrent (OLLAMA_MAX_CONCURRENT, default 1)
rust/src/route.rs                     + GovernedInference (the GPU governor) + from_config(max_concurrent) + 2 tests
rust/src/main.rs                      from_config passes cfg.ollama_max_concurrent; + transfers handler arm (gated, off)
rust/src/{bin/parity,sigil_parity,eval,resolve_eval,resolve_shadow}.rs   from_config(_, _, 1)  (offline; single-flight)
rust/src/util.rs                      + truncate_bytes (Go byte-slice truncate, for prompt parity)
rust/src/transfer.rs                  NEW — the transfers port + t4 prompt + TransferHandler (registered, NOT enabled)
rust/src/lib.rs                       + pub mod transfer
rust/src/bin/transfer_parity.rs       NEW — the source='rust' parity dump (deterministic; --vet optional)
sql/migrations/110_transfer_rumors_shadow.sql   NEW — the shadow table (applied surgically)
go/internal/ml/transfer_parity_test.go          NEW — the source='go' (t3) parity dump
```

## Next — Cutover Step 2 (continued): L12 rating, L13 narratives

**L12 — rating.** It is the `cmd/statcommentary` BATCH (its own Generate loop, NOT the pipeline_work
queue, NOT DrainAll). Cutover = a Rust batch bin, OR enqueue rating + a RatingHandler. Keep the
deterministic `pctBand` percentile→tier mapping in Go/Postgres (the model verbalizes the labeled tier,
never maps percentiles — the L8 rating breakthrough). Port `go/internal/ml/rating.go` +
`go/cmd/statcommentary`; parity-gate like transfers (deterministic axes; a `*_shadow` table).

**L13 — narratives.** The largest + heaviest GPU stage: compose `embed+cluster` (the candle dedup —
group storylines + drop near-dups ≥~0.85 cosine, the genuine Rust value-add) + route + extract +
persist. Port `go/internal/ml/news_narratives.go`.

**Step 3 — full cutover** (after rating + narratives land + are parity-proven): set
`COGNITION_STAGES=scrub,transfers,narratives,vibe,sigil` (+ rating via its bin),
`DERIVE_WORKER_ENABLED=false`, retire the Go cron drainer + inline scrub + statcommentary batch. Rust
= sole GPU user (the governor + the sequential drain self-limit it). vibe/sigil fold in here.
```
