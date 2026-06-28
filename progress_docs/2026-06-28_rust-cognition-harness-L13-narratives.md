# Rust Cognition Harness — L13: narratives port (Cutover Step 2, final stage)

**Date:** 2026-06-28
**Plan:** vault `Plan - Rust Cognition Harness build.md` → "The Cutover Plan" (Step 2) + §7 ledger (L13)
**Status:** DONE — the **narratives** stage is ported OFFLINE into Rust, registered-but-not-enabled
behind `COGNITION_STAGES`, and parity-gated **5/5** on deterministic axes. No live impact: Go's
`DrainAll` still owns live narratives until Step 3.

## Goal

Port `go/internal/ml/news_narratives.go` into Rust as the final missing Cutover Step 2 stage:
`embed+cluster` value-add + `route(EmotionalNews) + extract + persist`. Narratives is a real
`pipeline_work` stage (`Stage::Narratives`), unlike rating, so the production artifact is a handler
registered in the Rust worker but left disabled until the full cutover.

## Accomplishments

### 1. The narratives port — `narratives.rs`

Faithful port of the Go stage at the per-entity grain:

- `load_vetted_corpus` mirrors Go's vetted 72h/25-article corpus query and preserves order.
- `build_narratives_prompt` is byte-identical to Go's `buildNarrativesPrompt`, including the shared
  transfer-heat grounding lines.
- The n3 system prompt is carried verbatim, so the whole `ollama_request` including `system` is a
  parity axis.
- `NarrativesParser` mirrors Go's balanced-brace salvager: clean empty arrays become no-story markers;
  truncated tails salvage complete objects; unsalvageable generations fail and retry.
- `ground_narratives` maps 1-indexed article numbers back to the fixed corpus, dedupes/validates them,
  and computes deterministic per-narrative impact.
- `persist_narratives` writes N `news_summaries` rows per generation, or a NULL narrative marker.

The deliberate Rust value-add is `dedup_corpus`: when the live worker loads a candle `Embedder`, it
clusters article title+description vectors and keeps one representative per near-duplicate cluster at
`0.85` cosine. The offline parity harness uses `embedder: None`, so dedup is identity and Go/Rust
prompt assembly is directly comparable. Where live dedup changes the model input set, that is an
improvement boundary, not a parity break.

### 2. Handler registration, still gated

`NarrativesHandler` is wired into `main.rs`, but only when `COGNITION_STAGES` includes `narratives`.
The default archbox posture remains scrub-only until Step 3, so Rust and Go do not double-claim the
same stage on the single GPU. The worker now loads the CPU embedder when either `scrub` or `narratives`
is enabled.

### 3. The parity gate — 5/5 entities

- **mig 112 `news_summaries_shadow`** applied surgically with `psql --single-transaction` and a ledger
  insert for only `112_news_summaries_shadow` (no `migrate.sh`; `099_team_rosters.sql` left alone).
- **`bin/narratives_parity`** writes source=`rust` rows with deterministic axes.
- **`go/internal/ml/narratives_parity_test.go`** writes source=`go` rows for the same entities.
- Gate entities: `team:18:FOOTBALL`, `team:20:NBA`, `player:70:NBA`, `player:813:NFL`,
  `player:37596384:FOOTBALL`.
- Result: **5/5** on built prompt bytes, whole `ollama_request` jsonb, model version, prompt version,
  and fixed corpus size. Prompt bytes: Chelsea 5780, Knicks 4995, Jaylen Brown 5460, Brandon Aiyuk
  4925, Nico Paz 3109.

The gate makes no model call by default; prose/storyline grouping is not a temp-0 parity axis.

## Verification

```bash
cargo build --manifest-path rust/Cargo.toml
cargo clippy --manifest-path rust/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path rust/Cargo.toml --lib
NARRATIVES_PARITY_DB=1 NARRATIVES_PARITY_ENTITIES="team:18:FOOTBALL team:20:NBA player:70:NBA player:813:NFL player:37596384:FOOTBALL" \
  go test ./internal/ml -run TestNarrativesParityDump -count=1 -v
```

All clean. `cargo test --lib`: 80 passed, 1 ignored.

## Step 3 handoff

Cutover Step 2 is complete: transfers, rating, and narratives are now Rust-ported and parity-gated.
Step 3 is the full cutover:

- set `COGNITION_STAGES=scrub,transfers,narratives,vibe,sigil`;
- keep rating as the Rust batch/bin path rather than a queue stage;
- set `DERIVE_WORKER_ENABLED=false`;
- retire the Go cron drainer, inline scrub path, and statcommentary batch;
- Rust becomes the sole GPU user; Go stays ingest + serve.

Rollback remains flag-gated: `DERIVE_WORKER_ENABLED=true` and stop the Rust daemon.

## Carry

- `099_team_rosters.sql` remains untracked and untouched.
- F-046 remains open; coordinate before any history rewrite.
- The rating numeric→f64 note and sigil private `go_json_*` cleanup still carry.
- archx220 needs no Ollama/model step; Ollama runs on archbox.
