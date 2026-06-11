# 064 + 065 — Football composite refinements (PAdj, drops, GK rework)

**Date:** 2026-06-11

## Goal

Act on the incremental-validity audit (does each datapoint actually improve the
composite's tie to scoring/prevention, or is it noise?). Two migrations.

## 064 — outfield: PAdj + drops

- **Tackling / Interceptions → possession-adjusted** (`raw × 50 / opp-possession`,
  floored at 30, on the rate-resolved base, 2-decimal). Raw volume was mildly perverse
  (rewards being camped in your own half); PAdj flips the outcome sign decisively —
  player vs goal-diff −.07→+.15 / −.07→+.11; team vs goal-diff −.24→+.53 / −.33→+.53.
  A swap, not a drop.
  - **Opp-possession source.** `rating_datapoints` reads `team_opp_possession` from the
    player's stats, but nothing in the committed pipeline produces it (it only existed in
    an ad-hoc clone experiment — prod had 0, and the 064 gate caught it on the prod
    dry-run). The team-level `team_stats.opp_possession_pct` IS populated on prod and
    equals the per-player value exactly, so **`_compute_rating_bundle` now injects it**
    per football player from `team_stats` at rating time (one extra LEFT JOIN) — no new
    derived field, no backfill, auto-covers future data.
- **Duels, Ball Recovery, Drawing Fouls → display** (in_comp/in_spec FALSE). Duels:
  highest-influence wedge yet no outcome signal (leave-one-out −.032). Ball Recovery:
  redundant with the passing/creation cluster (−.020). Drawing Fouls: weak value, drags
  (−.024).

## 065 — goalkeepers: bimodal value

Old GK composite (saves, Penalty Saves, Punching, High Claims) was 3/4 noise; `save%`
(the textbook skill metric) is a season-to-season coin flip (rel 0.08) and there's no xG
in-feed. Keeper value is **bimodal** (the eye test): a keeper on a bad team provides
value stopping the barrage; a keeper on a good team provides value with distribution.
New GK composite:

| | metric | rel | value vs conceding |
|---|---|---|---|
| Shot-Stopping | saves | 0.14 (context, not luck) | credits the barrage-stopper |
| Distribution | pass accuracy | 0.72 | −0.41 |
| Long-Ball Accuracy | long-ball accuracy | 0.66 | −0.25 |
| High Claims | good_high_claim | 0.45 | +0.13 |

Saves-z and distribution-z are anti-correlated (−0.28), so each keeper is credited for
their mode — a keeper who faces few shots earns ~0 on shot-stopping and earns on
distribution instead. Dropped Penalty Saves + Punching (noise); no goals-conceded (it
would penalise the barrage-stopper).

## Files changed

- `sql/migrations/064_football_player_padj_drops.sql` (new)
- `sql/migrations/065_football_gk_rework.sql` (new)

## Verification

- Clone + **prod dry-run** both green; both gates pass (064: PAdj used real
  opp-possession on 1563 rows; 065: 105 keepers on the 4 bimodal labels).
- Outfield composite → 9 z-datapoints, full rank spread retained. GK rank avg 12.7
  (pre-063) → 54.6, spread to 89.
- No API restart (rating_datapoints isn't a prepared statement); no frontend change
  (pizza filters in_comp; new GK labels render generically). NBA/NFL untouched.
- Applied to prod via `./sql/migrate.sh`.

## Result

The football composite now rates outfielders on possession-fair defending + the
attacking spine, and keepers on their actual two modes of value — shot-stopping and
distribution. Noise (Duels, Ball Recovery, Drawing Fouls, Penalty Saves, Punching) is
out of the math.
