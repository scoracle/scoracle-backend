# Optimization Ledger — Wave 1 + cheap perf (O11, O10, O4)

**Date:** 2026-06-19 · Backend (one tracked migration `096`; covering index built CONCURRENTLY — **no service restart**, prepared statements unchanged).

## Goal
First pass against the [Optimization Ledger](../../scoracleWiki/wiki/Optimization%20Ledger%20%2B%20Handoff%20%28eager%20%2B%20convergence%29.md):
knock out the zero-/low-risk items whose verification gate I could resolve directly on Archbox (DB access), before the
heavier momentum precompute (O1) and the decommission pass.

## What Was Done
- **O11 — stale binary cleanup.** Removed 12 stale `go/bin/scoracle-api.bak*` binaries (~370 MB; `go/bin` is gitignored so
  no commit). Kept the live `scoracle-api` and `scoracle-api.bak-preeager` (the Phase A rollback). `go/bin` 522 MB → 152 MB.
- **O10 — verified, no index warranted (no-op).** EXPLAIN'd both flagged scans on prod:
  - `entity_meta` player branch → **0.124 ms**, clean PK pushdown (`players_pkey` index-only + the `player_current_team`
    *view* resolving through `player_stats_pkey`). No heap problem.
  - `GetTeamResults` home/away `OR` → walks `idx_fixtures_sport_date` backward with the OR as an inline filter, **7 ms** under
    `LIMIT 20` on an 8.5 MB / 25k-row table. Within budget on a cached card endpoint; a composite index isn't warranted.
- **O4 — covering partial index for the statcommentary nightly enumerate.** The enumerate filters `rating_composite_score
  IS NOT NULL` (the magnitude score) — a *different* column from the existing `idx_*_rating_composite` partial (predicated on
  `rating_composite`, the displayed z), so the planner couldn't reuse it and bitmap-scanned `idx_player_stats_position` then
  heap-fetched ~4.6k rows out of the 1.9 GB `player_stats` heap. New `(sport, season, id) WHERE rating_composite_score IS NOT
  NULL` covering indexes make the enumerate **index-only**.

## Files Changed
- `sql/migrations/096_rated_enum_covering_index.sql` (new) — `idx_player_stats_rated_enum`, `idx_team_stats_rated_enum`.

## Verification
- `migrate.sh` applied `096` only (watermark was `095`). Both `CREATE INDEX` succeeded.
- Re-EXPLAIN of the NBA enumerate: `Index Only Scan using idx_player_stats_rated_enum`, **Heap Fetches: 0**, 14 buffers,
  **4.74 ms → 1.09 ms**.

## Result
Wave 1 complete (O11 ✅, O10 ✅ no-op, O4 ✅). No request-path behavior change; nightly enumerate de-heaped. Next: O19 (Sigil
leaderboard board), then O1 (momentum cohort precompute, with an equivalence harness), then the decommission pass.
