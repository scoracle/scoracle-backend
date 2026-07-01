# Eager-loading readiness — Phase A backend eager-blockers

**Date:** 2026-06-19  ·  Backend (Go API; no migration, no restart-blocking schema change).

## Goal
The platform is moving to a fully-owned-data, **eager-loading** model — every card fetches its
product on profile open and renders as received; nothing is rendered from a third-party passthrough.
An audit found the backend not ready for that fan-out: the read path ran **local model synthesis on every
cold `/sigil` open**, the cache-miss path had **no request coalescing**, `/momentum` **re-aggregated
the whole peer cohort live on every read**, and the pool was too small for ~6–9 concurrent reads per
profile open. Phase A makes the read path eager-safe (sequenced *before* the frontend mount-all).

## What Was Done
- **A1 — Removed on-read local model synthesis.** Deleted `maybeSynthesizeLazy` and its spawn in
  `GetEntityVibes`, so `/sigil` (and its `/vibes` alias) is now a pure precomputed read. Pruned the
  now-dead `synthGen` field, `SetSynthGen`, and the `NewRouter` variadic param across
  `handler.go` / `server.go` / `cmd/api/main.go` (main.go keeps the `synthGen` local for the
  event-driven listeners). Removed the orphaned `context`/`corpus`/`ml` imports from `data.go` and
  `ml` from `handler.go`/`server.go`. Coverage is preserved by the existing background synth paths
  (composite-shift listener, news-volume worker, nightly pipeline stage 4, catch-up sweep) — all of
  which dedup *before* spawning.
- **A2 — Single-flighted the cache-miss path.** Added a `singleflight.Group` to `Handler`;
  `serveStatementJSON` now coalesces concurrent identical (`cacheKey`) misses so only ONE runs the
  SQL and the rest share its result. Runs on a detached, 8s-bounded context so one caller's
  disconnect can't cancel the shared load. Covers every per-entity product **and all 5 leaderboard
  boards**.
- **A3 — `/momentum` eager-safe.** Now single-flighted (via A2) and cached at a dedicated
  `cache.TTLMomentum` (30 min) instead of `TTLData` (5 min) — the heavy peer-cohort aggregation
  re-runs at most once per window per entity, and the trajectory is slow-moving by definition. The
  full per-cohort precompute table (+ wiring `rating_history` as the trajectory source) is deferred
  to a measured post-launch optimization (needs prod `EXPLAIN` + schema verification).
- **A4 — Pool + read timeout.** `DB_POOL_MAX_CONNS` default `10 → 25`. Added the same 8s app-level
  read timeout to the no-cache serve path. **Deliberately did NOT** set a global Postgres
  `statement_timeout`: the API pool is shared with long-running maintenance jobs
  (`recalcAlltimeRanks`, `snapshot_rating_history`) that it would kill — reads are bounded at the
  app layer instead.

## Files Changed
`go/internal/api/handler/data.go` · `go/internal/api/handler/handler.go` ·
`go/internal/api/server.go` · `go/cmd/api/main.go` · `go/internal/cache/cache.go` ·
`go/internal/config/config.go`.

## Verification
`gofmt -l` clean · `go build ./...` OK · `go vet ./...` OK · `go test ./...` all pass
(`internal/api`, `auth`, `config`, `ml`, `thirdparty`).

## Result
The eager fan-out (~6–9 concurrent reads per profile open) is now safe: no local model on the read path,
concurrent misses coalesced to one DB hit, the one heavy aggregation cached long, and the pool sized
for the burst. `/sigil` serves a clean empty state for never-synthesized entities (a background path
fills them in).

**Deferred (not blockers):** the `/momentum` per-cohort precompute; a seed-time Sigil pre-warm
enqueue for brand-new entities (until then they serve `current: null` until a background synth path
picks them up).
