# 074 — Football team offense: Creation = Big Chances; add Progression

**Date:** 2026-06-11
## Goal
Round out team offense to a principled six.
## What was done
- **Creation**: key_passes → `big_chances_created`. Big chances are a strict quality-subset
  of key passes (0 teams exceed; ~25%; 0.81 collinear) — same as SoT⊆shots — and higher
  value (.90 vs .83). Use it, don't blend. The display 'Big Chances Created' folds in.
- **Progression** (NEW) = `passes_final_third + successful_dribbles`. The ball-progression /
  "possession + intent" dimension that was missing. Components only 0.40 collinear (distinct),
  combined value 0.58, reliability 0.85, 0.69 collinear with Creation (additive). The display
  'Successful Dribbles' folds in.
- **Injuries** kept — measured real (value −0.24/−0.28, reliability 0.36), a distinct
  squad-availability signal.
- Recompute football teams.
## Result
Offense = Goals For · Creation · Shooting · Progression · Possession Lost · Injuries (6),
symmetric with Defense's 6. NBA/NFL untouched.
## Verification
Clone green; gate confirms Creation=big chances + Progression; Chelsea offense = 6 wedges.
