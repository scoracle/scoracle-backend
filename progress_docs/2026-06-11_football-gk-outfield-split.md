# 063 — Football: clean Goalkeeper / outfield datapoint split

**Date:** 2026-06-11

## Goal

`rating_datapoints` emitted every football datapoint for every player regardless of
position. Two dishonest consequences:

1. **Keepers were rated on outfield play they never do.** A GK posts near-zero
   Goalscoring / Shooting / Dribbling / Tackling, and because those labels' z-score
   population was dominated by outfielders, the keeper earned a big *negative* z on
   each — dragging an otherwise-fine keeper down toward the bottom of the pack for the
   crime of not scoring goals. Every PL 2025 keeper sat **below the 45th percentile**
   (GK composite_rank: 0.0–44.5, **avg 12.7**).
2. **Outfielders carried four dead GK wedges** (Shot-Stopping / Penalty Saves /
   Punching / High Claims) at z=0 — breakdown clutter. And GK stats were z-scored
   against a sea of outfield zeros, inflating them (High Claims = +4).

## What was done

- **`rating_datapoints(p_sport, p_stats, p_rate_mode, p_position)`** — gains a
  position parameter. The FOOTBALL branch tags each row `pos_class` `'gk'`/`'out'` and
  gates: a `Goalkeeper` emits only the four keeping datapoints; everyone else
  (Defender / Midfielder / Attacker + unknown/NULL) emits only the outfield
  datapoints. Because each label is now emitted by exactly one position class, its
  z-score *population is that cohort for free* — keepers measured against keepers,
  outfielders against outfielders. NBA / NFL branches unchanged (ignore `p_position`).
  DROP+CREATE (a new param can't be added via REPLACE); trailing defaults keep the
  2-/3-arg call shapes resolving.
- **`_compute_rating_bundle`** and **`compute_event_starline`** — pass the player's /
  event's `position` into `rating_datapoints` (the only change to each body).
- **Recompute** — FOOTBALL only (NBA/NFL datapoints unchanged, so their rows are not
  touched): `compute_rating` + `compute_event_starline` per football season. The FCM
  notify trigger (`AFTER UPDATE OF percentiles`) is disabled for the window as a
  belt-and-suspenders — neither recompute touches `percentiles`.
- **In-migration gate** — asserts every football breakdown is a clean split: a
  keeper's holds only the four GK labels, an outfielder's holds none of them.

No API restart (`rating_datapoints` is not a prepared statement; rating column shapes
unchanged). The frontend z-pizza reflects the new breakdowns automatically.

## Files changed

- `sql/migrations/063_football_gk_outfield_split.sql` (new)

## Verification

- Clone (prod-faithful) + **prod dry-run** (rolled back) both green; gate passed.
- GK composite_rank **avg 12.7 → 53.9** (range 0–44.5 → 18.8–82.2); outfield ranks
  essentially unchanged (avg ~50, full 0–100).
- GK breakdown **4** datapoints, outfield **15** (was 19) — across the default
  breakdown *and* every `rating_modes` rate sibling (per_90 / per_game): 105 GK × 2
  modes all = 4; 1680 outfield × 2 modes all = 15.
- Applied to prod via `./sql/migrate.sh`.

## Result

Keepers are rated on keeping, outfielders on outfield play; neither pizza shows the
other's stats. The keeper-drag bug is gone — an average keeper is an average player,
an elite one rises. Keepers compress toward mid-pack in the *overall* pool (a 4-stat
composite has less spread than the outfield 12-stat one); `scoped_ranks.position`
already carries true GK-among-GK ranking for a position-scoped view.
