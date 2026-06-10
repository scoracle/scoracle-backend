# 058 — Multi-scope cohort percentiles (player position-scope fix + team scopes)

**Date:** 2026-06-10

## Goals

Fix the reported "position scopes not working" on the counting-stat pizza, and add
league/conference/division cohort scopes for teams. Both reduce to the same gap: the
template/datapoints blocks carried only ONE percentile per slice (the within-position
`percentiles` column), so the cohort-scope selector had nothing to swap to. The z-pizza
+ fantasy paths worked because they carry `pct` + `scoped_pct`.

## Decisions (confirmed with Scott)

Per-sport cohorts (the `scoped_percentiles` becomes nested `{scope: {key: pct}}`):

| Entity | Cohorts |
|---|---|
| NFL player | position · conference · division · all |
| NBA player | all (positionless) · conference |
| Football player | all (positionless) · league |
| NFL/NBA team | conference · division · league (= positionless, uniform league_id) |
| Football team | league (within competition) |

`'all'` (positionless) is carried for every sport so the frontend "All" option is
meaningful on every pizza (the template's base `pct` is within-position).

## What was done

- **`recalculate_percentiles`** — the two scoped blocks become multi-scope nested
  `{scope: {key: pct}}`, driven by a single LATERAL VALUES cohort table (players +
  teams). Main `percentiles` + team `percentiles` blocks unchanged.
- **Block functions** — `template_block` + `team_template_block` gain a `p_scoped` arg
  (old single-scope arities dropped); all four (`template`/`datapoints`/
  `team_template`/`team_datapoints`) emit `scoped_pct = {scope: pct}` per slice.
- **`_compute_rating_bundle`** — the breakdown `scoped_pct` and headline
  `rating_scoped_ranks` gain the per-sport cohorts (NBA/Football drop the old
  `position` scope for `all`+conference/league; NFL gains conference/division). A
  teams LEFT JOIN supplies conference/division without changing dp multiplicity.
- **`fantasy_block`** — reads the SAME `scoped_percentiles`, so it's updated to the
  nested format too (its `scoped_ranks` becomes `{scope: pct}` per cohort). Same
  3-arg arity → replaced in place; the fantasy headline scope re-rank now matches.
- **Zero-downtime ordering** — the old block arities (`template_block/4`,
  `team_template_block/3`) are NOT dropped: the running binary keeps calling them
  until the restart swaps to the new build, so there's no sparkline error window
  between apply and restart. (`datapoints`/`team_datapoints`/`fantasy` keep their
  arity and are replaced in place — the running binary picks up the nested reads
  immediately, gracefully degrading to no scoped re-rank only on the two template
  paths until restart.)
- **Go (`db.go`)** — pass `ps.scoped_percentiles` / `ts.scoped_percentiles` to the two
  `*_template_block` calls (the only signature change → API restart required).
- Canonical `sql/shared.sql` synced (recalc + 4 blocks). `_compute_rating_bundle` is
  migration-canonical (054 → 058); `compute_team_rating` already emits the team
  headline scopes — unchanged.

## Files changed

- `sql/migrations/058_multiscope_percentiles.sql` (new)
- `sql/shared.sql` (canonical BASE synced — recalc + block functions)
- `go/internal/db/db.go` (template_block / team_template_block gain scoped arg)

## Verification

Local throwaway clone (`scoracle_test`, socket `/tmp/p2pg`):

- **Parity gate green**: rating scalars (composite/specialist/ranks/specialty)
  byte-identical across 6085 snapshotted rows — the engine math is untouched.
- Scope-coverage gate: NFL-QB {position,conference,division}=158, NBA {all,conference}
  =1151, Football {all,league}=5545, NFL teams {conference,division,league}=64, player
  headline ranks=2034.
- Distinct per-scope values confirmed (mid NFL QB passing_yards: all=50.5 / position=
  38.9 / conference=45.9 / division=41.2).
- Go API serves `scoped_pct` + `rating_scoped_ranks` per scope; rebuilt + restarted.
- Frontend Playwright (local API): the **template** pizza re-ranks — a football
  attacker's Assists slice goes 84 → 93 toggling All → By League (matches API
  all=84.2 / league=93.4); NFL QB headline re-ranks By Conference 47.4 / By Division 60.0.

## Rollout (pending authorization)

Prod dry-run (COMMIT→ROLLBACK — the parity gate runs against genuine pre-058 values) →
`migrate.sh` apply → Go rebuild + `systemctl --user restart scoracle-api` (migration
strictly BEFORE restart; the dropped block arities + new template_block signature make
the restart mandatory). Frontend cf:deploy carries the TemplateStat.scoped_pct + the
scope-aware pizza.
