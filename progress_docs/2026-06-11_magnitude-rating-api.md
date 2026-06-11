# 068 (API) — expose the magnitude score in the rating payloads

**Date:** 2026-06-11

## Goal

067 computed + stored the magnitude `rating_composite_score` (+ specialist + scoped). This
exposes it through the API so the frontend can display it instead of the percentile.

## What was done

- **`go/internal/db/db.go`** — added `rating_composite_score` / `rating_specialist_score`
  (and `rating_scoped_scores` for the sparkline) to three prepared statements, all of which
  serialize via `row_to_json`, so adding the columns to the CTEs is sufficient:
  - `leaderboard` — player + team `ranked` branches (column-aligned for the `SELECT * … UNION`).
  - `sparkline` — `season_rating` outer SELECT + both inner UNION branches (player + team),
    keeping the column lists aligned. Per-mode scores already ride in `rating_modes` (067).
  - `roster` — `ranked` branch.

## Deploy / safety

- Built a throwaway binary and booted it on **:8099** against prod first — `db.New` prepares
  every statement at boot, so a clean boot = all prepared statements valid. Confirmed the
  leaderboard payload carried `rating_composite_score` (Yamal 99.0), then stopped the test
  instance **by its own PID** (never pkill-by-pattern — the 2026-06-10 outage lesson).
- Then `go build -o go/bin/scoracle-api` + `systemctl --user restart scoracle-api`. Prod
  API active, `/health` 200, leaderboard serving the score.

## Files changed

- `go/internal/db/db.go`

## Result

The magnitude score is live in the leaderboard / sparkline / roster payloads (alongside the
retained percentile). Frontend switch is the companion change in scoracle-frontend.
