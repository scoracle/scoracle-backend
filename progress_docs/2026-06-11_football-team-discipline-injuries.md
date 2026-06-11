# 068 — Football team composite: Yellow Cards + Injuries; team breakdown rebuild fix

**Date:** 2026-06-11

## Goal

Cards + injuries were orphaned display-only team metrics (no surface since the
Discipline/Squad card was removed). They impact results — promote the ones that earn it.

## What was done

- **`rating_datapoints_team` (FOOTBALL)** — Yellow Cards → composite ('discipline' facet),
  Injuries → composite ('offense' facet). Both pass the gate vs goal difference:
  Yellow Cards value −0.35 / reliability 0.71 (strong); Injuries −0.24 / 0.36 (real).
  **Red Cards stays display** — value −0.24 but reliability 0.15 (too rare/noisy).
- **`compute_team_rating` — now rebuilds `rating_breakdown`** every run. This was a latent
  gap: the player bundle builds its breakdown inline, but the team function only wrote
  composite/specialist/ranks/scores — the stored team `rating_breakdown` (what the pizza
  reads) was only refreshed by `finalize_fixture`, so a datapoint change applied via a
  migration left it stale. Now the team breakdown is rebuilt with the composite, so new
  datapoints actually surface. (Also recomputes the 067 magnitude score.)
- Recompute football teams (all seasons).

## Files changed

- `sql/migrations/068_football_team_discipline_injuries.sql` (new)

## Verification

- Clone + prod dry-run green; gate asserts Yellow Cards + Injuries are composite in the
  function AND present in the rebuilt breakdown (in_comp). Applied to prod.
- NBA/NFL team datapoints untouched.

## Result

Football teams are now rated on discipline (yellow cards) and squad availability
(injuries) too, and the team breakdown rebuilds on every recompute. Frontend renders the
new Discipline facet pizza (companion change in scoracle-frontend).
