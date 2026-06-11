# 071 — Football team: drop Penalties Conceded (noise)

**Date:** 2026-06-11
## Goal
Penalties Conceded (penalties_committed) is rare/luck-driven — right value sign (+0.33 vs
goals_against) but reliability 0.12 (same noise family as Penalties Won 0.149, Red Cards
0.15, both already dropped). Remove from the football team composite.
## What was done
- `rating_datapoints_team` (FOOTBALL): remove the `Penalties Conceded` datapoint. Recompute
  football teams.
## Verification
- Clone + prod dry-run green; gate confirms no Penalties Conceded wedge remains. Applied to
  prod. NBA/NFL untouched.
## Result
The football team defense composite drops a noisy wedge; cleaner, more reliable set.
