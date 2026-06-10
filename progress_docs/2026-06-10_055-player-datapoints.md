# Migration 055 — generic player datapoints + football template pizzas

**Date:** 2026-06-10
**Scope:** `sql/migrations/055_player_datapoints.sql` (new), canonical `sql/{shared,football}.sql` sync, `go/internal/db/db.go` (sparkline statement). Build-order item ② — the generic datapoints block + template-driven grouping that absorbs the frontend's GK hardcode.

## Goal

Two connected pieces, both riding the 054 metadata layer:

1. **Football template pizzas.** The football Composite flips from the flat 19-wedge
   z-pizza to counting-stat template pizzas grouped by facet — one pizza per facet
   (Shot-Stopping/Passing for GKs; Attacking/Passing/Defending for outfielders),
   seeded per position group in `stat_templates`. The frontend's hardcoded GK pizza
   becomes data. Fully reversible: `DELETE FROM stat_templates WHERE sport='FOOTBALL'`
   → the card falls back to the z-pizza.
2. **Generic datapoints block.** A new `datapoints_block()` in the sparkline payload:
   EVERY percentile-ranked base stat for a player — labeled/faceted/sorted from
   `stat_definitions`, default rate mode only (locked decision). The generic data
   layer behind future datapoint surfaces; nothing renders it yet.

## What was done

- **§1** `stat_templates.facet TEXT` — NULL = single unfaceted pizza (NFL/NBA
  unchanged); non-NULL = one pizza per facet, ordered by item sort_order.
- **§2** `position_group()` gains the FOOTBALL branch: Goalkeeper/Defender/
  Midfielder/Attacker → goalkeeper/defender/midfielder/attacker; NULL position →
  NULL → z-pizza fallback (1 rated player remains on that path).
- **§3** 63 football seed rows. Curation grounded in measured non-zero coverage on a
  prod copy (GK 402 / DEF 1778 / MID 1873 / ATT 1447 denominators); dropped
  `expected_goals` (0 provider-wide), `through_balls` (~35%), `penalty_goals` (~15%).

  | group | facets (wedges) |
  |---|---|
  | goalkeeper | shot-stopping (7) · passing (5) |
  | defender | defending (7) · passing (6) · attacking (4) |
  | midfielder | passing (6) · attacking (6) · defending (5) |
  | attacker | attacking (6) · passing (5) · defending (6) |

- **§4** `template_block()` emits `facet` per item + a mode-invariant base fallback:
  percentage keys (`save_pct`, `pass_accuracy`, …) have no rate siblings, so value/pct
  now COALESCE to the base key in per-X modes instead of zeroing. Mode emission still
  requires a true sibling (EXISTS check unchanged) — NFL/NBA byte-parity holds because
  all their template keys have real siblings.
- **§5** `datapoints_block(sport, stats, percentiles, scoped_percentiles)` — keys of
  `percentiles` minus rate-mode siblings (suffix match via
  `right(key, length(suffix))`, NOT `LIKE` — `_` is a LIKE wildcard), joined to
  `stat_definitions` for label/facet/sort. NULL when no qualifying key.
- **Go** sparkline statement: player branch adds
  `public.datapoints_block(...) AS datapoints`; team branch `NULL::jsonb`.
- Canonical `sql/shared.sql` (DDL + position_group + template_block + datapoints_block)
  and `sql/football.sql` (seed rows) synced.

## Gates (all green locally, 64 s)

1. **Gate 1** — NFL/NBA template byte-parity vs a §0 baseline, modulo the added
   `"facet": null` (stripped via `jsonb_array_elements … WITH ORDINALITY`).
2. **Gate 2** — football template shape: seed integrity, coverage (5542 players
   templated), every item faceted.
3. **Gate 3** — datapoints invariants: NULL ⟺ no qualifying key; no rate-sibling
   leaks (10260 player rows get datapoints).

## Verification (local throwaway DB)

- Migration 64 s, all gates green; Go gofmt/vet/build/test green; API booted against
  the migrated DB (db.New validates `datapoints_block` at boot).
- curl: GK shows 7+5 faceted wedges across all 3 rate modes (saves 142 → 3.737/90
  sibling rescale; save_pct 85.5 invariant via base fallback), 42 datapoints, fantasy
  null. NFL QB: facet null (unchanged single pizza), 23 datapoints.
- Frontend visual run (Playwright vs local API): GK renders Shot-Stopping + Passing
  pizzas; attacker renders Attacking/Passing/Defending; NFL/NBA regression sweep
  all-correct (see the frontend progress doc).

## Rollout status

**Prod NOT touched** — local validation only. Pending: prod dry-run (COMMIT→ROLLBACK)
→ `migrate.sh` apply → Go rebuild + `systemctl --user restart scoracle-api`
(migration strictly BEFORE restart: db.New prepares `datapoints_block` at boot) →
frontend cf:deploy → live spot-checks.

## Files changed

- `sql/migrations/055_player_datapoints.sql` — the migration (new)
- `sql/shared.sql`, `sql/football.sql` — canonical BASE synced
- `go/internal/db/db.go` — sparkline statement gains `datapoints`

## Follow-on noted

OG share-image composite body (`og-bodies.ts`, frontend) still renders the flat z-list
with its own GK value-null filter — aligning it with the template pizzas is future work.
