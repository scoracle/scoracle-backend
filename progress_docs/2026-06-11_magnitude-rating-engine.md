# 067 — Magnitude rating: percentile → score (the rating-model revelation)

**Date:** 2026-06-11

## The revelation

The leaderboard/profile "RATING" had always been the **percentile rank**
(`percent_rank × 100`). Percentile is **rank-based**, so the top 1% of *any*
population is — by definition — ≥99, no matter what you rank by. We proved this: ranking
1,785 players by **goals alone**, by **total passes alone**, or by a **14-metric sum**
(the trimmed metrics added back) each yields ~15–19 players at ≥99. A wall of 99s with no
differentiating power — Yamal (composite 25.1) and the #12 player (16.2) both read ~99.x
despite a chasm between them. The number of 99s is set by **population size**, not the
model. Adding metrics back does not move it (disproved the "incomplete model" hypothesis);
the cause was the **display scale**.

## What was done

The headline rating becomes a **magnitude score** — a transform of the composite itself,
which preserves the gaps percentile destroys:

```
score = 50 + 10 × (composite − cohort_mean) / cohort_sd ,  clamped [1, 99]
```

A standard T-score: average player = **50**, SD = **10**. The ×10 slope is a single
tunable constant (`public.rating_score(value, mean, sd)` helper). Result on football 2025:
the **99-club drops 19 → 4**, and the field spreads by real value — Mbappé 94, Kane 89,
Pedri 85. Distribution: 4 at ≥99, 7 ≥90, 17 ≥80, 706 mid-pack (45–55), floor 32.5.

Computed wherever the rank is computed, so the whole product stays consistent:
- **Player** (`_compute_rating_bundle`): `composite_score` + `specialist_score` (T-score
  over the season cohort), and **per-cohort** `scoped_scores` (position / conference /
  division / league) so the scope selector reads the same scale. Emitted per **rate-mode**
  (rating_modes) too.
- **Team** (`compute_team_rating`): `composite_score` + `specialist_score`.
- New columns (non-destructive — percentile columns retained for "top X%" context):
  `rating_composite_score`, `rating_specialist_score`, `rating_scoped_scores` on
  player_stats + team_stats.
- `_compute_rating_bundle` return type changed (DROP+CREATE) to add the score columns;
  `compute_rating` stores them; **all sports recomputed** (NBA / NFL / Football, players +
  teams).

This is the rating model's display backbone — proven on football (our test subject), it's
now the formula for NBA / NFL / future sports.

## Files changed

- `sql/migrations/067_magnitude_rating_engine.sql` (new) — `rating_score` helper,
  `_compute_rating_bundle`, `compute_rating`, `compute_team_rating`, columns, recompute.

## Verification

- Clone + prod dry-run green; gate asserts scores populated, avg ≈ 50, score-99-club <
  percentile-99-club. Result: **1,785 players (avg 50.0), 99-club 19 → 4, 1,078 teams**.
- All three sports scored and centered at 50 (NFL 2,034 · Football 3,556 · NBA 495).
- No API restart for the columns; recompute touches rating_* only (not `percentiles`), so
  no FCM notify.

## Result

The rating finally *represents value* instead of just rank. **Next (068):** flip the API
+ all 11 frontend surfaces from percentile → score, and recalibrate the tier-color
thresholds (81/61/41/21 on the percentile → magnitude bands ~65/55/45/35). The percentile
stays available as a "Top X%" context badge.
