# 070 — Football team: merge Yellow + Red Cards into one 'Cards'

**Date:** 2026-06-11

## Goal
One discipline metric per sport (NFL = penalty yards, NBA = fouls, football = Cards) —
not broken down by sub-type. Yellow + Red → a single Cards datapoint.

## What was done
- `rating_datapoints_team` (FOOTBALL): replace `Yellow Cards` (composite) + `Red Cards`
  (display) with **`Cards` = yellow_cards_total + red_cards_total** (defense facet,
  in_comp, sign −1). Reds were too noisy alone (rel 0.15) but folded in, yellow's
  reliability carries it: combined value −0.36, reliability 0.70 (simple sum is the most
  reliable; weighting reds higher only adds noise). Reds now count (small effect — avg
  3.7 reds vs 69.9 yellows), so team ratings shift slightly.
- Recompute football teams.

## Files changed
- `sql/migrations/070_football_team_cards_merge.sql`

## Verification
- Clone + prod dry-run green; gate confirms one Cards composite/defense wedge, no stale
  Yellow/Red. Applied to prod (Chelsea Cards = 101). No frontend change (renders in the
  Defense pizza). NBA/NFL untouched.

## Result
Football teams carry a single Cards discipline wedge, consistent with NFL/NBA's one-metric
treatment.
