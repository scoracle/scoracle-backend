# 069 — Fold the team 'discipline' facet into offense/defense

**Date:** 2026-06-11

## Goal
A lone-wedge Discipline pizza (football: just Yellow Cards) reads thin. Fold discipline
datapoints into offense/defense — purely a display regroup (the team composite is a flat
Σ(sign·z) over in_comp, independent of facet, so no rating changes).

## What was done
- `rating_datapoints_team`: FOOTBALL Yellow Cards → `defense`; NFL Penalty Yards For →
  `offense`, Penalty Yards Against → `defense`. Red Cards (display) → `defense`. Nothing
  uses `discipline` anymore.
- Recompute football + NFL teams (refreshes the stored breakdown facet tags).
- Gate asserts ratings are **byte-identical** (parity) and no `discipline` wedge remains.

## Files changed
- `sql/migrations/069_team_discipline_fold.sql`

## Verification
- Clone + prod dry-run green; parity held (0 rating drift). Applied to prod. NBA untouched.

## Result
Cards live in Defense, NFL penalty yards split offense/defense; the Discipline facet is gone.
