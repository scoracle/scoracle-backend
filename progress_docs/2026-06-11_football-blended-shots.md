# 072 — Football: blend shots + shots-on-target into one Shooting metric

**Date:** 2026-06-11
## Goal
Shots and shots-on-target are 0.89–0.94 collinear — where both appear they double-count
shooting. On-target predicts the outcome better everywhere (player goals .90 vs .83, team
scored .91 vs .81, conceded .78 vs .70). But don't discard speculative volume — FOLD into
one blended metric that credits both: shots_total + shots_on_target (on-target counts twice).
## What was done
- **PLAYER Shooting**: shots_total → `shots_total + shots_on_target` (rate-aware: per_90/
  per_game blend the matching siblings).
- **TEAM offense Shooting**: shots_on_target → `shots_total + shots_on_target`.
- **TEAM defense**: `Shots Allowed` = `shots_allowed + shots_on_target_allowed` (ONE wedge —
  the separate `SoT Allowed` is folded in and removed; defense double-count gone).
- Recompute football players + teams (+ event starline).
## Impact
Blend is volume-dominated (0.995 corr with raw shots) so ratings barely move, but it's
principled and de-double-counts. Free nudge vs midfielder inflation: switching toward
on-target cools speculative shooters (midfielders avg −0.02 in the blend, more under pure
on-target) and warms clinical finishers.
## Verification
- Clone + prod dry-run green; gate confirms player Shooting = shots+on-target and no stale
  SoT Allowed wedge. Per-mode blend exact. Applied to prod. NBA/NFL untouched. No frontend
  change (blended wedges render automatically).
## Result
One principled Shooting metric across player/team/offense/defense — credits the attempt and
rewards hitting the target, counted once.
