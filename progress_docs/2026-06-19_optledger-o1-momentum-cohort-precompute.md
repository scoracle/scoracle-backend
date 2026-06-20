# Optimization Ledger O1 — /momentum per-cohort precompute

**Date:** 2026-06-19 · Backend (migration `097`; `trendsStatement` read path; new maintenance ticker; **service restarted**, prepared-statement validation passed at boot).

## Goal
`/momentum` (`trendsStatement`) recomputed the **peer-cohort season aggregate LIVE on every read**:
for the requesting entity's cohort it scanned every member, `jsonb_each`-exploded their stats,
normalized cumulative_total keys per-member, and `AVG`'d per key. Measured **~17.6 ms** for an NBA
guard cohort (248 members → 9,150 jsonb pairs) — and that work is *identical* for every entity in the
cohort save the leave-one-out of self. (A2 single-flight + A3 30-min TTL already made it eager-*safe*;
O1 removes the cost.)

## Design — exact, not approximate
The live read averages the cohort **excluding the requesting entity**. To stay bit-identical we
precompute, per cohort `(sport, season, league_id, entity_type, position)`, the full-cohort per-key
**SUMS + COUNTS** (not averages), plus the season-composite score sum/count and member count. The read
then reconstructs the **exact leave-one-out** average by subtracting the entity's own already-computed
normalized value (`entity_season_aggregate`, identical normalization formula) and decrementing the
count. So: same numbers as before, but a 248-member scan becomes one **0.047 ms** PK lookup + a walk
over the entity's own ~40-70 keys.

Verified bit-exact:
- **psql harness** replicating the old `peer_aggregate` SQL vs the reconstruction — 13 entities across
  all 3 sports × player/team including the highest- and lowest-rated outliers: `max_abs_diff = 0`,
  identical key sets, identical `cohort_size` (248/29/31/17) and `peer_season_score_avg` (48.3/48.6/47.8/48.2).
- **end-to-end** old binary (:8000) vs new (:8001, cache off) `/momentum` for 5 entities: **IDENTICAL**
  cohort_size, score_avg, and every peer-average key (`max_abs_diff = 0`).

Cohort-key notes: `position=''` sentinel for teams; players carry a non-null position (NULL-position
players match no cohort, as before); `league_id` raw (NBA/NFL uniformly 0, football splits by league);
read looks up `COALESCE(effective_league, 0)`. Only teams have cumulative_total comparable keys (players
have none); team divisor coverage is 100%; divisor=0 members contribute NULL → excluded from sum AND
count (mirrors AVG skipping NULL), so leave-one-out counts stay exact.

## What Was Done
- **Migration `097`** — `peer_cohort_aggregate` table (key_sums/key_cnts/score_sum/score_cnt/member_count)
  + `refresh_peer_cohort_aggregates()` (transactional DELETE+INSERT — MVCC-safe, no ACCESS EXCLUSIVE lock)
  + populate on apply. 386 cohorts (335 player, 51 team).
- **`db.go` `trendsStatement`** — replaced `player_peer_cohort`/`team_peer_cohort`/`peer_cohort`/`peer_aggregate`
  with `cohort_lookup` (PK lookup) + a reconstructed `peer_aggregate` (exact leave-one-out avgs, cohort_size,
  score_avg). Repointed `peer_season_score_avg` off the removed `peer_cohort`.
- **`maintenance.go`** — new `PeerCohortInterval` (24 h) ticker: `refreshPeerCohortAggregates` once at
  startup (post-deploy freshness) then daily. Refresh is pure SQL; season-rolled stats only move on
  seeding/finalize, so daily is ample.

## Files Changed
- `sql/migrations/097_peer_cohort_aggregate.sql` (new)
- `go/internal/db/db.go` — trendsStatement peer block
- `go/internal/maintenance/maintenance.go` — refresh ticker + worker

## Verification
- New binary booted clean (all prepared statements incl. the reconstruction validated); no degraded mode.
- Live prod `/momentum` (post-restart) matches golden values exactly across NBA/NFL/FOOTBALL × player/team.
- `cohort_lookup` EXPLAIN: **Index Scan, 0.047 ms** (was ~17.6 ms for the live aggregate).
- Startup refresh logged `Peer-cohort aggregates refreshed cohorts=386` (runs after the all-time-rank
  recompute in the startup chain). Health 200.

## Result
O1 ✅ shipped + deployed. `/momentum` peer deltas are now an indexed lookup + cheap self-reconstruction,
bit-identical to before. `rating_history` (O3) remains the future *trajectory* source once it accrues
depth (today ~2 points/entity); O1 deliberately scoped to the cohort aggregate, leaving the per-event
sparkline untouched.
