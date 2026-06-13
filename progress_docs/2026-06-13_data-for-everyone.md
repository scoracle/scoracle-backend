# Phase 2 — data-for-everyone (gated composite + sub-gate breakdowns)

## Goal
Per Scott's design: only players above the meaningful gate (FOOTBALL appearances ≥ 10)
get a ranked composite; sub-gate (low-minute) players still get their DATA — a full
breakdown — but are excluded from the rating. No empty profiles, no cohort inflation.

## What was done (backend; frontend is the unranked badge, separate doc)
- **`_compute_rating_bundle` (migration 080)** — `dp` now TAGS `is_ranked` instead of
  filtering. `pop` (mean/sd) and every rank/score/scoped window stay `WHERE is_ranked`, so
  the rated cohort is **byte-identical** (parity-gated: 12,457 rows, 0 drift). Sub-gate
  players get a breakdown (z vs the rated cohort; per-stat fill = `50+10·z` magnitude,
  clamped 1-99 — a fast scalar, NOT an O(n²) percentile) with `composite`/rank/score = NULL.
  Result: **8,078 sub-gate football player-seasons now carry an unranked breakdown.**
- **Reads (`db.go`)** — the `sparkline` season_pick + available_seasons now surface a season
  on `rating_breakdown IS NOT NULL` (not just `rating_composite`), so an unranked player's
  profile renders. The season_rating collapse tiebreaks on breakdown richness too. The
  **leaderboard is untouched** — `rating_composite IS NOT NULL` keeps unranked players off it.

## Note (perf)
First 080 attempt used a count-based percentile-vs-cohort for sub-gate fills — O(ungated×gated)
per stat, ran 15 min, killed (clean rollback). Replaced with the `50+10·z` magnitude fill.

## Verification
- Parity gate: rated cohort byte-identical (0 drift). Smoke: 8,078 unranked breakdowns.
- API restarted (health 200). Live: player 29809271 (9 apps) sparkline → breakdown_dp 15,
  rank/score NULL.

## Quick reference
- gate: `public.rating_thresholds` (FOOTBALL appearances ≥ 10, migration 079)
- engine: `_compute_rating_bundle` (080) — `is_ranked` tag splits ranked cohort from data-set
- reads: `sparkline` surfaces on `rating_breakdown`; leaderboard stays on `rating_composite`
