# 2026-06-24 — Rust Cognition Harness: the library-first build plan (POINTER)

> **This is a pointer.** The full plan lives in the vault as a `Plan -` note:
> **`scoracleWiki/wiki/Plan - Rust Cognition Harness build.md`**
> (it executes the conception `scoracleWiki/wiki/Architecture/Rust Cognition Harness.md`).
> The plan is kept in the vault — beside the conception it executes, `[[wiki-linked]]`, synced
> across archx220 + archbox. This in-repo stub exists so the code repo's trail references it.

## What it is

The actionable, phased engineering plan that turns the **Rust Cognition Harness** conception (locked
2026-06-24) into a build. The thesis, verbatim:

> Build the **capability library** (route · resolve · extract+validate · embed+cluster · normalize ·
> persist) **first**, then **re-express the already-proven `vibe` stage as its first composition**.
> Vibe is the **fixture, not the deliverable** — same bytes out (re-proven byte-for-byte at temp-0
> against Go), but the primitives now exist and are tested. **Library-first, NOT a parity-port of the
> five Go `internal/ml` stages.** Address models by **role**, never by name. Deterministic math stays
> in **Postgres**. *Rust touches a row only to make a model smarter about it.*

## Scope line (full detail in the vault note)

- **IN (build now):** the six primitives as concrete Rust interfaces — real impls for **Route, Extract,
  Persist**; shaped stubs for Resolve, Embed, Normalize · **vibe re-expressed** as `route + extract +
  persist` and re-proven at temp-0 · the **Router + A/B eval hook** (`bin/eval.rs`).
- **HORIZON (designed-for, not built):** stat_resolve / box-score scraping (provider-API as the parity
  oracle) · multilang (`normalize` + a router-A/B-chosen model) · the other stage ports (rating,
  narratives + embed/cluster, transfers, sigil, scrub).

## Build order + the validation gate

`L0` library scaffold (introduce `Harness`; extract `Inference` over `OllamaClient`; `Parser<T>`,
`Extracted<T>`, `Provenance`) → `L1` re-express vibe → `L2` Router + `bin/eval.rs` → then each later
stage additively via **shadow → temp-0 parity → per-stage cutover**.

**Validation gate:** the re-expressed vibe must STILL pass the Phase-1 temp-0 parity harness
(`rust/src/bin/parity.rs` + Go `TestVibeParityDump`) — **4/4 identical** on SCORE, VIBE, prompt bytes,
request body. The proof is reused as the regression test for the refactor; a passing parity run *is* the
definition of "L1 done".

## Built host this plan builds on (UNCHANGED in mechanism)

`rust/src/worker.rs` (Worker loop) and `rust/src/work.rs` (queue client) are inviolate — the library
slots *inside* `StageHandler::handle`, whose signature generalizes `(pool, ollama)` → `(&Harness, item)`.
`ollama.rs::OllamaClient` becomes the first `impl Inference`. `vibe.rs` is rewritten as a recipe.

See the vault note for: the six primitive interfaces (with the host piece each leans on), the
Route/router design + eval discipline, the stage-by-stage composition map with effort sizing, and the
landmines.
