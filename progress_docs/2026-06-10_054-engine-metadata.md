# Migration 054 — engine metadata (rate_modes, rating_thresholds, stat_definitions flags)

**Date:** 2026-06-10
**Scope:** `sql/migrations/054_engine_metadata.sql` (new), canonical `sql/{shared,nba,nfl,football}.sql` sync, `go/internal/maintenance/maintenance.go` sync. Strictly behavior-preserving — three byte-parity gates prove it.

## Goal

P2 metadata cleanup from the engine audit: centralize the rating engine's per-sport
hardcodes into metadata tables so a new mode/sport/threshold is a seed row, not a
function rewrite.

- `public.rate_modes` — rate-mode families (mode, suffix, denom, formula) that were
  per-trigger `per_X_keys` arrays + suffix literals.
- `public.rating_thresholds` — eligibility gates (NBA gp≥30 & min≥20, FOOTBALL
  app≥15, NFL gp≥8) that were inline `WHERE` literals in `_compute_rating_bundle`.
- `stat_definitions.rate_sibling` / `rate_base` — which keys emit rate siblings and
  their legacy alias bases (turnover→tov, shots_total→shots), formerly trigger
  special-cases.

Rewritten on top: derived-stats triggers (`apply_rate_siblings`), `rating_datapoints`,
`_compute_rating_bundle`, `compute_rating`, `fantasy_block`/`template_block`, notify.

## Parity gates (all green, local + prod dry-run + prod apply)

1. **Gate 1** — strip every rate sibling + fantasy_points, re-fire the new triggers,
   assert every `stats` JSONB regenerates byte-identical.
2. **Gate 2** — recompute ratings for every (sport, season) with the metadata-driven
   engine, assert all `rating_*` columns + `rating_modes` md5-match the §0 baseline.
3. **Gate 3** — assert `fantasy_block` / `template_block` payloads byte-identical.

## The gate-2 planner pathology (5.5 h hang → 67 s)

The first local run sat in gate 2 for 5.5 h+ at 100% CPU. Root cause was planner
statistics, not the engine:

- The local test DB's `player_stats` was autoanalyzed **80 s after** the migration
  transaction started — gate 2's first UPDATE plans were cached against a
  statistics-less table.
- Without stats the planner put the inlined `_compute_rating_bundle` subquery on the
  inner side of a Nested Loop **without a Materialize node** — re-executing the whole
  multi-CTE z-pipeline once per player row (~24 min per pair-mode for FOOTBALL).
- With stats, the same UPDATE runs in **0.86 s**, and the plan is good even as a
  generic (parameter-blind) plan — verified via `plan_cache_mode = force_generic_plan`.
- Diagnosis trap for next time: `timeout`ed psql kills only the client — orphaned
  server backends kept executing their old bad plans and lock-blocked fresh probes,
  masquerading as "the fix didn't work". An unanalyzed `CREATE TABLE AS` bench copy
  reproduced the same pathology for the same reason.

### Fix (planner-only, zero behavior change)

1. `compute_rating`'s two UPDATEs wrap the bundle in `WITH b AS MATERIALIZED (…)` —
   the bundle executes exactly once regardless of estimates. Also hardens the live
   path: `finalize_fixture` calls `compute_rating` on every seed.
2. `ANALYZE public.rate_modes / rating_thresholds` right after §1 seeding — tables
   created in-transaction are invisible to autovacuum and have no stats otherwise.
3. `ANALYZE public.player_stats` before gate 2 — gate 1 just rewrote every row.

## Rollout

- Local (scoracle_test): all 3 gates green, **1 m 07 s** total.
- Prod dry-run (COMMIT→ROLLBACK): all 3 gates green, **3 m 59 s**.
- Prod apply via `sql/migrate.sh`: gates green, recorded in `schema_migrations`.
- Go API rebuilt + `systemctl --user restart scoracle-api` (prepared statements
  validate the new schema at boot) + live spot-checks.
- During the migration's lock window (§2 `ALTER TABLE stat_definitions` holds
  AccessExclusive) the API stalls — ~4 min on prod. Acceptable now; the unfixed
  multi-hour version would have been an outage, which is why the apply was gated on
  the planner fix.

## Files changed

- `sql/migrations/054_engine_metadata.sql` — the migration (new)
- `sql/shared.sql`, `sql/nba.sql`, `sql/nfl.sql`, `sql/football.sql` — canonical BASE
  synced to the metadata-driven forms
- `go/internal/maintenance/maintenance.go` — catchUpSweep synced
- `go/internal/listener/transfer_worker.go` — drive-by `gofmt` fix (one comment line)

## Frontend

Byte-parity ⇒ zero frontend changes; verification only (typecheck + tests + spot-check).
