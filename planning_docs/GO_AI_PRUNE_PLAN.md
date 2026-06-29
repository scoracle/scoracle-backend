# Go AI Layer Pruning Plan

**Date:** 2026-06-29
**Status:** Plan — not yet executed
**Goal:** Remove all remaining AI/LLM/Gemma handling from the Go codebase, leaving Go responsible only for **ingest → queue → serve**. Every model call lives in the Rust Cognition Harness.

## Background

The Step-3 cutover (2026-06-28) moved every live LLM stage to Rust:

- `scrub` → `rust/src/scrub.rs`
- `transfers` → `rust/src/transfer.rs`
- `narratives` → `rust/src/narratives.rs`
- `vibe` → `rust/src/vibe.rs`
- `sigil` → `rust/src/sigil.rs`
- `rating` (batch) → `rust/src/bin/statcommentary.rs`

The Headlines feature (2026-06-29) added `Stage::Headlines` → `rust/src/headline.rs`.

Production already runs with `DERIVE_WORKER_ENABLED=false`, the Go `statcommentary` binary is deleted (C5), and `release.sh` builds only three Go binaries (`scoracle-api`, `pipeline`, `vibesynth`). However, Go still imports, initializes, and ships a large body of retired AI code kept as rollback scaffolding. That scaffolding is no longer needed after C5 removed the Step-3 rollback aid.

## Current Go responsibilities (production)

| Component | Live responsibility |
|---|---|
| `cmd/api` | HTTP serving from Postgres; auth; cache; ETags; maintenance tickers. **No model calls on serving requests.** |
| `cmd/pipeline -mode ingest` | RSS sweep only: fetch articles, normalize, write `news_articles` + `news_article_entities`. |
| `cmd/vibesynth -mode nightly/reconcile/restamp` | DB-only Sigil reconciliation / vocab migration. **No Gemma.** |
| `internal/corpus` | RSS sweep logic + entity-name lookup. |
| `internal/maintenance` | SQL auto-vet of `news_article_entities` primaries + enqueue scrub work to `pipeline_work`. Rust does the Gemma disambiguation. |
| `internal/work` | Durable queue client (used by maintenance + admin tools). |
| `cmd/work` / `cmd/validate-stmts` | Admin + validation tooling. |

Everything else under `go/internal/ml`, `go/internal/derive`, and several `go/cmd/*` binaries is now dead code.

## Pruning checklist

### Phase 1 — Delete retired command binaries (zero runtime risk)

These are no longer built by `release.sh` and are archaeological:

- [ ] `go/cmd/newsnarrate`
- [ ] `go/cmd/newsscrub`
- [ ] `go/cmd/sentiment`
- [ ] `go/cmd/transfer`
- [ ] `go/cmd/statcommentary` (Go version — retired by Step 3; Rust `rust/bin/statcommentary` is the live path)

### Phase 2 — Strip AI from `cmd/pipeline`

`cmd/pipeline` currently imports `internal/ml` and `internal/derive`, builds an `OllamaClient`, wires every generator, and supports `-mode corpus` (the legacy full Go LLM chain). Production only uses `-mode ingest`.

- [ ] Remove `-mode corpus` support and the `runCorpus` path.
- [ ] Remove imports of `internal/ml` and `internal/derive`.
- [ ] Remove `ml.SetGemmaConcurrency`, `ml.NewOllamaClient`, `NewNewsScrubber`, `NewTransferGenerator`, `NewNewsNarrator`, `NewVibeGenerator`, `NewSigilGenerator`.
- [ ] Make `ingest` the only mode.
- [ ] Update `scripts/hosting/cron-pipeline.sh` header comment to say "RSS sweep only; Rust owns LLM derivation."

### Phase 3 — Strip AI from `cmd/api/main.go`

The API still builds an `OllamaClient` and every generator, then conditionally starts the real-time derive worker when `DERIVE_WORKER_ENABLED=true`. Production runs `DERIVE_WORKER_ENABLED=false`.

- [ ] Remove the derive-worker startup block (`if !cfg.DeriveWorkerEnabled { ... }`).
- [ ] Remove `ml.SetGemmaConcurrency`, `ml.NewOllamaClient`, and all generator construction.
- [ ] Remove the NewsScrubber wiring for the maintenance ticker.
- [ ] Remove the `deriveDone` shutdown wait.
- [ ] Update the Swagger package description if it implies the API performs Gemma calls.

### Phase 4 — Delete `internal/derive`

Once `cmd/api` and `cmd/pipeline` no longer use it:

- [ ] Delete package `go/internal/derive` (`derive.go`, `worker.go`, `derive_test.go`).

### Phase 5 — Simplify `internal/maintenance` to SQL-only

`maintenance.go` still accepts `*ml.NewsScrubber` and `*ml.OllamaClient` and has an inline Gemma scrub branch guarded by `NewsScrubViaQueue`. The live path is `NewsScrubViaQueue=true`.

- [ ] Remove the inline Gemma scrub branch.
- [ ] Remove `Ollama` from `maintenance.Config` and the `scrubber *ml.NewsScrubber` parameter.
- [ ] Keep only the SQL auto-vet of primaries + enqueue of scrub work.
- [ ] Remove `NewsScrubViaQueue` config or make it a no-op constant `true` before dropping it in Phase 7.

### Phase 6 — Move shared constants out of `internal/ml`

`internal/corpus/corpus.go` references `ml.NewsLookback` and `ml.LookupEntityName`. These must move before `internal/ml` can be deleted.

- [ ] Move `NewsLookback` to `internal/corpus` (or a neutral `internal/model` package).
- [ ] Move `LookupEntityName` to `internal/corpus`.
- [ ] Update `corpus.go` imports.

### Phase 7 — Delete `internal/ml`

- [ ] Delete package `go/internal/ml` (all `.go` files + parity tests).

### Phase 8 — Drop AI config from `internal/config`

After no live code imports `internal/ml`:

- [ ] Remove from `Config`:
  - `OllamaBaseURL`, `OllamaModel`, `OllamaTimeout`, `OllamaShortTimeout`, `OllamaMaxConcurrent`, `OllamaKeepAlive`
  - `DeriveWorkerEnabled`, `DeriveDrainInterval`
  - `NewsScrubInterval`, `NewsScrubBatch`, `NewsScrubTimeout`, `NewsScrubViaQueue` (or collapse to a simple auto-vet interval)
- [ ] Remove the corresponding env-var reads.
- [ ] Keep `OLLAMA_*` env vars out of Go entirely — Rust reads them directly.

### Phase 9 — Optional: strip Gemma modes from `cmd/vibesynth`

`-mode single` and `-mode backfill` call Gemma inline. `-mode nightly/reconcile/restamp` are DB-only and stay.

- [ ] Remove `-mode single` and `-mode backfill`.
- [ ] Remove `ml.SetGemmaConcurrency`, `ml.NewOllamaClient`, `ml.NewSigilGenerator` wiring.
- [ ] Decision point: do we want a manual Go backfill path, or should all Gemma work route through Rust? If the latter, execute this phase. If manual backfill is still operationally useful, defer.

### Phase 10 — Final verification

- [ ] `cd go && go build ./...` succeeds.
- [ ] `cd go && go test ./...` passes (except any pre-existing unrelated WIP failures).
- [ ] `cd rust && cargo build --all-targets && cargo clippy --all-targets -- -D warnings && cargo test --lib` passes.
- [ ] `scripts/hosting/release.sh --build-only` succeeds.
- [ ] `.env` / `.env.local` templates updated to remove obsolete Go-only AI variables.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Deleting `internal/ml` breaks `corpus.go` constants | Phase 6 moves the constants first. |
| `cmd/vibesynth -mode backfill` is still used manually | Phase 9 is optional; decide before executing. |
| Some `go/docs` swagger annotations still reference Go Gemma paths | Regenerate swagger (`swag init`) or update annotations; no runtime impact. |
| Historical progress docs reference deleted files | Leave progress docs untouched — they are archival. |
| `internal/derive` tests may be the only users of some `internal/ml` paths | Delete tests alongside the package; Rust tests now cover the live behavior. |

## Definition of done

- `go/internal/ml` does not exist.
- `go/internal/derive` does not exist.
- No non-test Go file imports `internal/ml`.
- `go/cmd/api/main.go` does not build or reference an `OllamaClient`.
- `go/cmd/pipeline` only supports `-mode ingest` and has no AI imports.
- Production env no longer needs `DERIVE_WORKER_ENABLED` or Go-side `OLLAMA_*` config.
- `release.sh` still builds `scoracle-api`, `pipeline`, `vibesynth` (3 Go) + Rust binaries.
- All gates green.

## Carry

- This plan does **not** change the Rust Cognition Harness.
- It does **not** change any Postgres schema.
- It does **not** remove the live `vibesynth` nightly reconcile — that remains DB-only Go.
- `099_team_rosters.sql` and F-046 remain untouched.
