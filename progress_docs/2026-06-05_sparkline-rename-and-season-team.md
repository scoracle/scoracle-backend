# 2026-06-05 — Rename starline → sparkline + add the season's team to the rating

## Goals

(1) Finish the long-flagged `starline` → `sparkline` rename so the endpoint name matches
the frontend's vocabulary. (2) Add the entity's team FOR THE SELECTED SEASON to the rating
payload, so the meta card can show a player's most-recent team by default and switch to the
correct team when a season is picked (fixes the stale "last-seeded team" glitch).

## Decisions / What Was Done

- **Rename** (`/starline` → `/sparkline`): prepared-statement key `starline`→`sparkline`,
  handler `GetStarline`→`GetSparkline`, `'page'` value `starline`→`sparkline`, route
  `/{entityType}/{id}/sparkline`. Kept `/{entityType}/{id}/starline` as a **deprecated
  alias** → same handler, so the frontend rollout has zero downtime. ENDPOINTS.md + README
  updated.
- **Season team**: the `sparkline` statement's `season_rating` now carries a `team` object
  (`id, name, short_code, logo_url`). Player branch LEFT JOINs `public.teams` on that
  season's `ps.team_id`; team branch uses itself. Season-aware by construction (the rating
  is already rolled to `season_pick`, which defaults to the latest rated season).
- No schema migration — `player_stats.team_id` already exists.

## Accomplishments

- `db.go`, `data.go`, `server.go`, `ENDPOINTS.md`, `README.md`.
- `gofmt`/`go build`/`go vet` clean. Pre-flight (throwaway `cmd/spcheck`, removed) ran the
  full `db.New` registration + executed the renamed statement — clean (no degraded-mode
  risk). Restarted via `systemctl --user restart scoracle-api.service`; health 200.

## Verification

`/sparkline` returns the season `team`; `/starline` alias still 200. Season-awareness
confirmed on Luis Díaz (241036): default (2025) → **FC Bayern München**; `?season=2024..2022`
→ **Liverpool**. NBA Wembanyama default → San Antonio Spurs.

## Quick reference

```
GET /api/v1/{sport}/{entityType}/{id}/sparkline   # rating + season team + event series
GET /api/v1/{sport}/{entityType}/{id}/starline     # deprecated alias (remove post-rollout)
```

## Follow-ups

- `swag init` to regenerate `go/docs/*` (still show `/starline`; swag not installed here).
- Remove the `/starline` alias once the new frontend is confirmed live everywhere.
