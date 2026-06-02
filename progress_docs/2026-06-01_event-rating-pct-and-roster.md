# 2026-06-01 — Per-event rating percentiles (migration 029) + roster endpoint

## Goal

Two backend additions so the frontend can (a) plot the rating engine's Composite
+ Specialist as **0–100 lines** on the Trends sparkline alongside the 0–100 vibe
line, and (b) show a team's **roster ranked by rating**.

## What Was Done

**Migration 029 — per-event 0–100 percentiles.** The rating engine (027/028)
stores per-event Composite/Specialist only as z-scores; the *season* ranks are
0–100 but there was no per-event 0–100. Added `rating_composite_pct` /
`rating_specialist_pct` (NUMERIC) to `event_box_scores` + `event_team_stats`,
derived by a positionless `percent_rank()*100` over the per-event z within each
`(sport, season)` population — the same normalization migration 018 uses for
`composite_score`, and matching the positionless season ranks in 027. New
`recalculate_event_rating_pct(sport, season)` does the derivation; `finalize_fixture`
gets one `PERFORM` after the starline z recompute so the pct stays fresh
in-season. Backfilled every `(sport, season)`. The z columns are untouched.

**Starline endpoint** now surfaces the two pct fields in each `events[]` row
(`db.go` `event_series` CTE) — `row_to_json` picks them up automatically.

**Roster endpoint — `GET /{sport}/team/{id}/roster`.** New `roster` statement
(`db.go`): every player on the team's season roster (`player_stats` ⋈ `players`)
with season Composite/Specialist (+ ranks + specialty + name/image/position),
ordered by the `(Composite + Specialist)` sum. New `GetRoster` handler + route.
No migration — `player_stats` already carries the rating columns.

## Files Changed

```
sql/migrations/029_event_rating_percentiles.sql   (NEW)
go/internal/db/db.go                               (starline pct fields + roster statement)
go/internal/api/handler/data.go                    (GetRoster)
go/internal/api/server.go                          (roster route)
```

## Verification

- Migration applied clean; NBA 2025 player events: 27,816 rows, pct uniform
  `[0,100]` (min 0 / avg 50 / max 100), monotonic vs z (Wemby 17.56→100, −3.23→19).
- `/api/v1/nba/player/56677822/starline` events now carry `rating_composite_pct`
  / `rating_specialist_pct` (opener 99.4 / 96.2).
- `/api/v1/nba/team/21/roster` → 9 rated players ordered by sum (SGA 11.69 #1,
  Holmgren 9.30, Wallace 6.55 …); ordered-by-sum-desc = true.
- `go build` clean; API restarted on `:8000`.

## Result

Plumbing in place for the frontend 3-line 0–100 sparkline + the roster card.
**Base quality** — endpoint shapes + the freshness hook are solid; polish
(scope toggles, league-scoped roster variant, swagger regen) is follow-on.
