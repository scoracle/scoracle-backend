# 2026-06-09 — Backfill NFL position on old seasons (migration 048)

## Goal
Fix a bug surfaced by the Phase-3 counting-stat pizza: on NFL seasons before 2023 the
Composite card reverted to the 3 facet z-pizzas and lost the counting-stat / fantasy
layout (reported on Dak Prescott).

## Root cause
`player_stats.position` was only captured from 2023 on — 2018-2022 rows are 100% empty
(9,159 rows). With no position, `public.position_group('NFL','')` → NULL → `template_block`
→ NULL → frontend z-pizza fallback (and `nflSideOfBall` can't pick a side → all 3 facets).
A pre-existing data gap that the template feature exposed.

## Decision
Backfill `position` from `players.meta->>'position_abbreviation'` ("QB","WR",…) — the
canonical source, present for active + retired players; `position_group` already handles
abbreviations. 'UNK' (genuinely unknown) is skipped → stays empty → z-pizza (correct).
Each old season was 100% empty, so the whole season is backfilled uniformly (abbrev) →
per-season percentile/rating cohorts stay internally consistent. Pure data fix — no code
change; the deployed `template_block` + frontend render the template once position resolves.

## Accomplishments
- `sql/migrations/048_backfill_nfl_position.sql` — backfill empty NFL positions from meta,
  then `recalculate_percentiles` + `compute_rating` for the affected seasons (their
  position cohorts changed), with the milestone NOTIFY trigger disabled during the bulk
  recalc + a smoke gate (Dak 2022 → 'quarterback').

## Verification
- Prod dry-run (ROLLBACK): UPDATE 3814, smoke OK (Dak 2022 → quarterback). Applied for
  real → COMMIT. 5,345 rows stay empty (position_abbreviation='UNK' — correct). Dak's
  2018-2022 now position 'QB' → quarterback. Live API (no restart needed): Dak 2022
  sparkline carries the QB `template` (passing_attempts/yards/TDs/INT/rush) + `fantasy`;
  2024 still works.

## Follow-on (not done)
- Durability: the Python seeder / `finalize_fixture` should populate `position` (fall back
  to `players.meta`) so a future re-aggregation of an old season doesn't re-empty it.
- ~58% of old NFL rows are `position_abbreviation='UNK'` (deep-roster/non-skill) and stay
  z-pizza; only QB/RB/WR/TE get templates anyway, so notable skill players are covered.
