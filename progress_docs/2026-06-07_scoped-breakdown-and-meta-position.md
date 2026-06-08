# 2026-06-07 — Position-scoped breakdown (043) + autofill position (044)

## Goals
Make the **position scope re-rank the pizza SLICES**, not just the headline — "the
positional scope should work the same way the per-x scope works. Slices should
adjust to reflect the positional relative percentiles, the way the per-x ones
already do." And fix the related bug: **player position was missing from `/meta`**
(EntityMeta + OG share-card subtitle showed no position).

## Decisions
- **043 is strictly additive.** Only `_compute_rating_bundle` changes — it gains a
  per-datapoint `scoped_pct` (`{ "position": <pct> }`) in every `rating_breakdown`
  element: the `percent_rank` of that datapoint's `sign*z` WITHIN `(label, position)`,
  parallel to the existing positionless `pct`. Computed for the default AND every
  rate mode (`rating_modes`), so **per-X × position compose**. The composite / ranks
  / specialist / z / pct math is untouched. Players only (position); teams are a
  follow-on. Served automatically — the sparkline stmt already passes
  `rating_breakdown` + `rating_modes` through `row_to_json`, so `scoped_pct` rides
  along with **no Go change**.
- **044 re-exposes position on the autofill MVs.** Migration 013 had moved position
  out of `public.players` into `player_stats.position`, and recreated the MVs from
  `players` — silently dropping the column. 044 DROP+CREATEs each `{sport}.autofill_entities`
  with a top-level `position` (latest-season `player_stats.position` via a lateral for
  NBA/NFL, the existing DISTINCT-ON `ps` row for football). Teams get `NULL` (they
  carry conference/division in `meta`). The NFL `"Unknown"` sentinel is `NULLIF`'d so
  meta reads clean. Unique `(id,type)` indexes recreated for REFRESH CONCURRENTLY.

## Accomplishments
- `043_scoped_breakdown.sql` — applied to prod. In-txn parity gate compares the
  breakdown MODULO `is_specialty` AND the new `scoped_pct`; **PARITY OK**, no drift in
  any non-scoped field across all rated rows. Smoke: 15,659 default breakdowns +
  15,659 mode breakdowns carry `scoped_pct.position`.
- `044_autofill_position.sql` — applied to prod. Position populated: NBA 1231/1311,
  NFL 2211/5344 (rest are stat-less rookies + NULLIF'd "Unknown"), FOOTBALL 8210/8268.
- Verified live: `/nba/player/177/sparkline` returns `scoped_pct` per datapoint
  (Aaron Gordon F: Playmaking 48→64, Rim Protection 25→15 within Forwards) AND the
  `per_36` mode carries its own `scoped_pct` (compose confirmed). `/nba/meta` returns
  `position` for 1231 players.

## Quick reference
```bash
# dry-run (COMMIT→ROLLBACK), then apply:
sed 's/^COMMIT;$/ROLLBACK;/' sql/migrations/043_scoped_breakdown.sql > /tmp/dry.sql
psql "$DATABASE_PRIVATE_URL" -v ON_ERROR_STOP=1 -f /tmp/dry.sql   # expect: PARITY OK / ROLLBACK
psql "$DATABASE_PRIVATE_URL" -v ON_ERROR_STOP=1 -f sql/migrations/043_scoped_breakdown.sql
psql "$DATABASE_PRIVATE_URL" -v ON_ERROR_STOP=1 -f sql/migrations/044_autofill_position.sql
```
No API restart needed (both ride existing prepared statements via `row_to_json`).

## Files
`sql/migrations/043_scoped_breakdown.sql` (NEW), `sql/migrations/044_autofill_position.sql` (NEW).
