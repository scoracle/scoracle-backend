# Migration 056 — team templates + team datapoints block

**Date:** 2026-06-10
**Scope:** `sql/migrations/056_team_templates.sql` (new), canonical `sql/{shared,nba,nfl,football}.sql` sync, `go/internal/db/db.go` (sparkline team branch). Build-order item ③ — teams join the 055 counting-stat world.

## Goal

Flip the team Composite (all three sports, Regular model) from z-pizzas to curated
offense/defense counting-stat template pizzas, and add a generic team datapoints
block — the team siblings of 055's player machinery. Reversible per sport:
`DELETE FROM stat_templates WHERE sport='X' AND position_group='team'` → that
sport's teams fall back to the z-pizza.

## Design decisions

- **Separate team functions, NOT a generalized player function.** Generalizing
  `datapoints_block` would change its signature → DROP/recreate → the live API's
  prepared statement errors between migration apply and Go restart. Separate
  `team_template_block` / `team_datapoints_block` have no compat gap and zero
  player-path risk.
- **No rate-sibling exclusion on team datapoints.** Teams carry no rate siblings,
  and the player exclusion (`right(key, …) = rm.suffix`) would wrongly drop genuine
  NFL team base keys that merely end in a mode suffix (`points_per_game`,
  `yards_per_game`, `points_allowed_per_game`).
- **Facets = 'offense'/'defense'** — the same keys as the team z facets and
  `rating_categories`, so the frontend's per-facet sub-score footers (`cardScore` →
  `catPct`) line up unchanged.
- **`{'default': items}` only** — teams have no rate modes (triggers emit no
  siblings); the frontend's `templateForMode` falls back to `default` for any
  requested mode. Rate/model selectors are already player-gated in ContentShell.
- **Scoped percentile label**: `league` (FOOTBALL) / `conference` (NBA, NFL),
  mirroring `recalculate_percentiles`' team cohort scoping. The percentiles meta
  keys (`_sample_size`, `scope_type`, …) drop out naturally via the
  `stat_definitions` INNER JOIN (entity_type='team').

## Seed curation (39 rows, position_group='team')

Grounded in measured non-zero coverage on a prod copy of team_stats (NFL 256 /
NBA 240 / FOOTBALL 582 team-season rows); every seeded key ≥94% coverage; all
negative-direction keys verified `is_inverse=true`.

| sport | offense (sort 10s) | defense (sort 20s) |
|---|---|---|
| NFL (7+7) | points_for, total_yards, passing_yards, rushing_yards, passing_touchdowns, rushing_touchdowns, turnovers(inv) | points_against(inv), defensive_sacks, defensive_interceptions, takeaways, total_tackles, passes_defended, tackles_for_loss |
| NBA (6+5) | pts, ast, fg3m, true_shooting_pct, oreb, turnover(inv) | pts_allowed(inv), reb, stl, blk, dreb |
| FOOTBALL (7+7) | goals_for, shots_on_target, big_chances_created, key_passes, assists, possession_pct, pass_accuracy | goals_against(inv), tackles, interceptions, clearances, blocked_shots, saves, aerials_won |

Excluded for coverage: NFL qb_hits (33%), first-down/red-zone family (75%, absent
2018-19); FOOTBALL chances_created (33%), ball_recovery (38%), passes_final_third
(38%). NBA def_fg_pct/def_fg3_pct lack stat_definitions rows.

## What was done

- **§1** 39 seed rows + `ANALYZE stat_templates`.
- **§2** `team_template_block(sport, stats, pct)` — `{'default': [...]}`, labels
  from stat_definitions (entity_type='team'), value/pct COALESCE→0, NULL when the
  sport has no team rows (z-pizza fallback).
- **§3** `team_datapoints_block(sport, stats, pct, scoped)` — every
  percentile-ranked team stat, labeled/faceted/sorted from stat_definitions;
  `scoped_pct` keyed league/conference; no exclusion (see above).
- **Go** sparkline team branch: `NULL::jsonb AS fantasy` stays;
  `team_template_block(...) AS template`, `team_datapoints_block(...) AS datapoints`
  replace the 055 NULLs. Player branch untouched.
- Canonical sync: functions → `sql/shared.sql`; seeds → `sql/nba.sql`,
  `sql/nfl.sql`, `sql/football.sql` (per-sport SQL boundary).

## Gates (all green locally, 2.3 s)

1. **Gate 1** — seed integrity: counts (FOOTBALL=14, NBA=11, NFL=14), facets only
   offense/defense, every key resolves to a team stat definition, negative keys
   flagged is_inverse.
2. **Gate 2** — template shape: every team row → exactly its sport's seed count
   items, all faceted (1078 team rows templated).
3. **Gate 3** — datapoints invariants: NULL ⟺ no qualifying key; no meta-key
   leaks; scope labels league/conference correct; NFL `points_per_game` present
   (datapoints emitted for 1078 team rows).

## Verification (local throwaway DB)

- Go gofmt/vet/build/test green; API booted on :8099 against the migrated DB
  (db.New prepares both new functions at boot).
- curl team sparklines: NFL 14 faceted items (points_for 557 → pct 93.5) + 78
  datapoints all conference-scoped, `points_per_game` present (pct 77.4, NOT
  excluded); NBA 11 items + 28 datapoints; FOOTBALL 14 items + 92 datapoints
  league-scoped; zero meta leaks anywhere.
- curl player regression: NFL QB (6-item template default+per_game, fantasy, 22
  datapoints), football GK (12-item faceted, 3 modes, 49 datapoints), NBA (7-item,
  3 modes, fantasy, 26 datapoints) — all byte-equivalent to the 055 world, no
  rate-sibling leaks.
- Frontend Playwright (dev server → :8099): all three sports' team profiles render
  offense/defense counting-stat pizzas with correct values/pcts and per-facet
  footers (NFL 87.1/87.1 — verified genuinely equal in rating_categories; NBA
  82.8/65.5; FOOTBALL 13.7/77.9). is_inverse visible (Patriots 24 turnovers →
  pct 29 wedge). Player regression: NFL QB single offense z-pizza + composite
  footer, football GK Shot-Stopping/Passing template pizzas — both unchanged.

## Frontend

Zero code changes needed — the 055 template machinery (`template()`,
`pizzaGroups`, `toTemplateStat`, `cardScore`) is entity-agnostic and the team
facets match `rating_categories` keys. Doc comments updated in
`sparkline.server.ts` + `CompositeCard.tsx` (see the frontend progress doc).

## Rollout (COMPLETE — 2026-06-10)

- Prod dry-run (COMMIT→ROLLBACK) green, `migrate.sh` apply recorded 056, then
  `systemctl --user restart scoracle-api` → "Database connected", health 200.
  Prod gates: 39 template rows (FOOTBALL=14, NBA=11, NFL=14); 1078 team rows
  templated; datapoints meta-free with correct scope labels. Payload
  spot-checks matched local (NFL team 14 items/78 dp conference; NBA 11/28;
  FOOTBALL 14/92 league; QB regression intact).
- Frontend cf:deploy shipped (carried the share unplug + team-facet-footer
  edit); live Playwright sweep on scoracle.com green.
- **Incident note:** between the Go rebuild and the migration apply, a `pkill
  -f "bin/scoracle-api"` aimed at a local test instance also matched the prod
  systemd service (same binary path). systemd restarted the NEW binary against
  the OLD 055 schema → `prepare "sparkline": function
  public.team_template_block does not exist` → degraded mode, all endpoints
  503 for ~1.6h (08:19–09:57 EDT) — the "leaderboard 503" report. Resolved by
  the apply + restart. Lesson: never `pkill` by binary-name pattern on the
  prod box; the prod service execs the repo's `go/bin/scoracle-api` path.

## Files changed

- `sql/migrations/056_team_templates.sql` — the migration (new)
- `sql/shared.sql`, `sql/nba.sql`, `sql/nfl.sql`, `sql/football.sql` — canonical BASE synced
- `go/internal/db/db.go` — sparkline team branch gains template + datapoints
