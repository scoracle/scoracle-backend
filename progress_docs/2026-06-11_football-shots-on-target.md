# 073 — Football: Shooting = shots on target only (drop raw shots)

**Date:** 2026-06-11
## Goal
Reversed the 072 blend. Shots-on-target ⊆ total shots, so blending (shots+SoT) just buries
the signal under off-target attempts that have no outcome value. On-target predicts goals
better everywhere (player .90 vs .83). Use on-target only.
## What was done
- PLAYER Shooting → `shots_on_target` (rate_base, per-90/per-game siblings).
- TEAM offense Shooting → `shots_on_target`.
- TEAM defense → `SoT Allowed` = `shots_on_target_allowed` (the blended Shots Allowed reverts).
- Recompute football players + teams (+ starline).
## Impact
Cools speculative long-shooters (midfielders −0.07), rewards clinical finishers. Backend-only.
## Verification
Clone + prod dry-run green; per-mode exact; gate confirms Shooting=shots_on_target + SoT
Allowed. Applied to prod. NBA/NFL untouched.
