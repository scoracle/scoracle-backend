# 061 — Football team: penalties_won → display; add Fouls Won / Fouls Committed

**Date:** 2026-06-10

## Goal

Resolve the penalties-won problem surfaced by the Aston Villa / Chelsea investigation:
`penalties_won` was distorting the team composite — propping up penalty-spike seasons
and deflating controlled, low-penalty ones — without a quality justification.

## Decisions (data-grounded)

- **`penalties_won` → display tier (in_comp=FALSE).** It was the *least* repeatable
  composite signal (year-over-year **0.149** vs ~0.70 for shots-on-target / key-passes /
  big-chances), triple-counted a won penalty (already in Goals For + Shooting), and its
  swing tracked coaching/approach more than quality. No clean swap target exists in our
  data: `fouls_drawn` has ~0 value (**−0.10** vs goals — a style fingerprint, repeatability
  0.656) and `shots_insidebox` is a **0.89 duplicate** of Shooting. So it moves to the
  display tier (still shown, out of the rating math) rather than being replaced.
- **Add `Fouls Won` (offense) + `Fouls Committed` (defense) as display datapoints**
  (in_comp=FALSE) for team-side parity with the player model. Player `Drawing Fouls` *is*
  a composite datapoint (0.44 vs goals+assists — earns its spot per-player), but the
  signal washes out to −0.10 at team level, so on the team side these belong in the
  display tier alongside Possession % / Accurate Passes / Big Chances Created — visible,
  characterful, but not quality signals. (`Fouls Committed` sign −1 = fewer-is-cleaner for
  the displayed percentile; display-only, trivially flippable.)

## What was done

- `rating_datapoints_team` (FOOTBALL branch only): `Penalties Won` in_comp TRUE→FALSE;
  added `Fouls Won` (fouls_drawn, offense, display) and `Fouls Committed` (fouls_committed,
  defense, display). Recompute football team-seasons. No API restart, no frontend change.

## Files changed

- `sql/migrations/061_football_team_fouls.sql` (new; migration-canonical)

## Verification

Local clone (at 060): 061 applies clean — gate confirms Penalties Won is out of the
composite and Fouls Won/Committed are present (582 rows each). 060→061 board:

| Team | penalties | rank 060 → 061 |
|---|---|---|
| Villa 2025 | 0 | 56.8 → **62.1** (+5.3) — controlled season no longer punished |
| Villa 2021 | 2 | 32.0 → 38.1 (+6.1) |
| **Chelsea 2023** | **11** | 77.9 → **69.5** (−8.4) — penalty-spike prop removed |
| Chelsea 2020 | 10 | 99.0 → 96.9 (−2.1) |
| Chelsea 2022 | 3 | 64.9 → 67.0 (+2.1) |

## Rollout (pending authorization)

Prod dry-run (COMMIT→ROLLBACK) → `migrate.sh` apply. No API restart, no cf:deploy.
