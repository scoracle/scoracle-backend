# 2026-06-02 — Per-datapoint rating breakdown (migration 030) + starline payload

## Goal

Phase 1 (backend foundation) of the profile reframe: persist the per-datapoint z
that the rating engine computes-and-discards, so the upcoming Composite tab
(pizza of the composite datapoints) and Specialist tab (peak skill + scarcity)
can render it. **Core principle: store the composite as raw z, serve it to the
frontend as a percentile.**

## What Was Done

**Migration `030_rating_breakdown.sql`** — strictly additive.
- New `rating_breakdown JSONB` on `player_stats` + `team_stats`: an array, one
  object per datapoint — `{label, z, pct, in_comp, in_spec, sign, facet, is_specialty}`.
- `pct` = `percent_rank() OVER (PARTITION BY label ORDER BY sign*z) * 100` — the
  0–100 the UI draws. `sign*z` makes negative datapoints (Ball Security,
  Giveaways, Possession Lost) read correctly: low raw value → high pct.
- `is_specialty` flags the single peak `in_spec` datapoint (matches the engine's
  stored `rating_specialty`).
- `compute_rating` / `compute_team_rating` re-declared **verbatim** with one new
  pass appended after the existing rank UPDATE, reusing the still-live
  `_rating_dp` / `_team_dp` temp tables. The composite/specialist/specialty/rank
  math is untouched. `finalize_fixture` left alone — it already PERFORMs the two
  functions, so the breakdown rides the existing in-season recompute. Backfilled
  every (sport, season).

**`go/internal/db/db.go`** — added `rating_breakdown` to the `starline`
statement's `season_rating` CTE (both UNION arms + outer SELECT); flows through
`row_to_json` into the `rating` object automatically. No route/handler change.

**`ENDPOINTS.md`** — documented `rating.rating_breakdown` on the starline endpoint.

## Files Changed

```
sql/migrations/030_rating_breakdown.sql   (NEW)
go/internal/db/db.go                       (starline season_rating CTE)
ENDPOINTS.md                               (starline rating_breakdown)
```

## Verification

- **Frozen-math proof**: snapshotted composite/specialist/ranks for all 20,413
  rated players + 1,078 teams before/after — **diff = 0** (math byte-identical).
- Breakdown covers all rated entities; array lengths exact: **NBA 9 / NFL 12 /
  FOOTBALL 18** (teams 7–8).
- **Exactly one `is_specialty` per row** (all 20,413); the flagged label matches
  the stored `rating_specialty` (5/5 sample).
- Wembanyama: Rim Protection `pct 100, z 6.1058, is_specialty:true`; negative
  Ball Security `pct 15.1` (high turnovers → low pct ✓).
- `go build` clean; API restarted on `:8000`; `curl …/starline | jq .rating.rating_breakdown`
  returns the array for players (9) + teams (8).

## Result

The plumbing is in place. Tonight's UI phases (Composite card, Specialist card,
meta 3-score row) all read `getStarline().rating.rating_breakdown` — no further
backend work needed for them. See plan `~/.claude/plans/zany-dazzling-hamster.md`.
