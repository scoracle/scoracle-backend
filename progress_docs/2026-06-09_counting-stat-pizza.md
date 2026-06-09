# 2026-06-09 — Counting-stat pizza templates (Phase 3, migration 047)

## Goal
Replace the z-score Composite pizza with a per-position COUNTING-STAT template where
fantasy has standardized one — NFL offensive skill (QB/RB/WR/TE). Each wedge shows a
real counting stat (attempts, yards, TDs, INTs…) with its within-position percentile —
the "visual oomph" the cards lacked. Everything else keeps the z-score pizza.

## Decisions
- **NFL offense only** (Scott's call): NBA + football z-wheels are already intuitive, and
  NFL defenders lack a rich standard fantasy line — so a template exists ONLY for NFL
  quarterback / running-back / receiver. Absent template ⇒ payload `template` is NULL ⇒
  frontend keeps the z-score pizza (NBA, football, NFL defense/OL/ST, teams).
- **No data migration / recalc.** The wedge percentiles already live in
  `player_stats.percentiles` (partitioned by position, is_inverse applied — INT/fumbles
  were already is_inverse=true), and the rate-mode siblings were ranked by the 045/046
  recalcs. Phase 3 is pure metadata + payload.
- **rate_base column** on stat_templates handles legacy rate aliases (e.g. NBA
  turnover→tov) generically; unused by the NFL seeds but kept for future templates.

## Accomplishments
- `sql/migrations/047_stat_templates.sql` — `public.stat_templates` table + NFL seeds +
  `public.position_group()` (raw position → template group, full names + abbrevs) +
  `public.template_block()` (per-position template, pre-expanded by rate mode; NULL when
  no template) + grant.
- Canonical sync: `sql/shared.sql` (table + both functions + grant), `sql/nfl.sql` (the
  NFL template seed rows — per the per-sport boundary).
- `go/internal/db/db.go` — `sparkline` statement serves the `template` block (player
  branch via `public.template_block`; NULL for teams). Thin passthrough.

## Verification
- Throwaway PG: position_group (NBA→NULL, QB→quarterback, Safety→NULL), template_block
  (NFL QB non-null, NBA/defender NULL), the turnover rate_base alias across modes.
- Prod: 047 dry-run (ROLLBACK) clean → applied (CREATE TABLE / 17 rows / 2 fns / GRANT).
  template_block on Josh Allen → 534 att (85.7) / 4224 yds (92.1) / 29 TD (88.3) / 12 INT
  (19.7, correctly inverted) / 678 rush yds (100) / 16 rush TD (100). gofmt/build/vet
  clean. Rebuilt + restarted scoracle-api (no degraded mode). Live sparkline: NFL QB
  template present, NFL Safety + NBA NULL. Deployed.

## Quick reference
- `GET /api/v1/{sport}/{type}/{id}/sparkline` → `rating.template` = `{mode: [{key,label,
  value,pct,sort}]}` for NFL QB/RB/WR-TE; null otherwise (frontend z-pizza fallback).
- Add a template: insert rows into `public.stat_templates` (sport, position_group,
  stat_key, sort_order[, rate_base]) + map the position in `public.position_group`.
