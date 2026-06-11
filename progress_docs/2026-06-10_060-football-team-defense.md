# 060 — Football team defense: outcome metrics + possession-adjusted volumes

**Date:** 2026-06-10

## Goal

The football team **defense** facet was measuring defensive *workload*, not *quality* —
ball-dominant sides (PSG, Arsenal) were under-rated defensively and leaky low-block
sides (Hellas Verona) over-rated, while a mediocre-but-busy defense (Chelsea 2025) was
inflated by raw interception volume. Fix the defense composite to reflect what teams
actually concede, plus correct the possession bias in the volume stats.

## Decisions (all data-grounded; see correlations below)

1. **Possession-adjust the volume stats (PAdj).** Raw tackles/interceptions correlate
   *positively* with goals conceded (you rack them up *because* you defend a lot), so
   they mildly reward bad defenses. PAdj = `raw × 50 / opponent_possession` — the per-90
   idea for defenders (divide by how much you had to defend). Opponent possession is
   measured data; the league average is structurally **50%** (possession is zero-sum,
   measured 49.98%), and the `×50` constant washes out of the z-score entirely → fully
   data-driven, no arbitrary weight. Applied **inline** (no materialization; recomputes
   each rating run).
   - **Tackling → PAdj**, **Interceptions → PAdj** (outcome corr flips: tackles +0.18→
     **−0.36**, interceptions +0.24→**−0.18** — perverse to correct).
   - **Clearances dropped from the composite.** PAdj clearances only reaches ~0 vs
     outcome (neutral noise, not a quality signal); kept as a *display* datapoint.
2. **Add the outcome metrics to the composite.** `Shots Allowed` + `Big Chances Allowed`
   were display-only (in_comp=FALSE), and there was **no Goals Against datapoint at all**
   (asymmetric with Goals For). Promote both + **add Goals Against** (in_comp, sign −1).
   These are the no-estimation defensive truth.

## What was done

- `rating_datapoints_team` (FOOTBALL branch only — NBA/NFL untouched): Tackling +
  Interceptions use inline PAdj; Clearances → display-only; Shots Allowed + Big Chances
  Allowed → composite; new Goals Against datapoint.
- Recompute every football team-season (`compute_team_rating`). No API restart
  (`rating_datapoints_team` is not a prepared statement; `compute_team_rating` updates
  rating columns, not percentiles → notify trigger uninvolved). No frontend change.

## Files changed

- `sql/migrations/060_football_team_defense.sql` (new; the function is migration-canonical
  — no separate `sql/*.sql` sync)

## Verification

Local throwaway clone (at 059): 060 applies clean from scratch — gate confirms Goals
Against is in the composite, Clearances is display-only, and PAdj is applied (128
high-possession Tackling rows where the datapoint value exceeds raw tackles).
Before→after composite rank (validated):

| Team | rank before → after |
|---|---|
| Arsenal 2024 (34 GA) | 62.1 → **88.4** ↑↑ |
| Arsenal 2025 (27 GA) | 92.6 → **94.7** ↑ |
| PSG 2025 (29 GA) | 91.6 → **97.9** ↑ |
| **Chelsea 2025 (52 GA)** | 90.5 → **86.3** ↓ |
| Hellas Verona 2025 (61 GA) | 57.9 → **17.9** ↓↓ |
| Pisa 2025 (71 GA) | 23.2 → **7.4** ↓ |

## Rollout (pending authorization)

Prod dry-run (COMMIT→ROLLBACK) → `migrate.sh` apply. No API restart, no cf:deploy.
