# Optimization Ledger O16 — remove the bundled profile route

**Date:** 2026-06-19 · Backend (Go; route/handler/statement removal; **service restarted**, prepared-statement validation passed). No migration.

## Goal
Remove the legacy bundled profile route now that the eager / owned-data model has each card fetch its own
product. The ledger gated this on "confirm Astro standby / OG / mobile don't consume it."

## Consumer verification (the gate)
Enumerated every backend URL the **live** frontend builds (its `*.server.ts` fetchers + `data-sources.ts`):
only `/{sport}/{type}/{id}/{product}`, `/{sport}/leaderboard[/{board}]`, `/{sport}/team/{id}/roster`, and
the dead `newsUrl` (`/news/{type}/{id}`, the disconnected CoMentionsCard). **No bare `/{sport}/{type}/{id}`
fetch and no bundled `/leagues/.../{id}` fetch.** The page-shell data the bundled payload carried is sourced
elsewhere now: `ContentShell` reads `stats().available_seasons` ("available_seasons rides the stats
payload"), `stat_definitions` ride `/stats`, entity identity is `/meta`, season score/ranks are `/stats`+`/rating`.
iOS: no `profile_page`/league-profile consumer. OG: none. The only ever-consumer was the Astro-era whole-page
frontend; the standby's 72h soak (cutover 2026-05-03) expired ~6 weeks ago.

Also corrected a ledger inaccuracy: there is **no separate "league SQL fn"** — the `{sport}_profile_page`
entries are inline SQL, and `GetLeagueProfilePage` SHARES them with `GetProfilePage`. So both routes go
together (both unused). The per-product **league** routes (`/leagues/.../momentum`, `/leagues/.../meta`,
`/leagues/.../health`, `/leagues/.../results`) are untouched and keep working.

## What Was Done
- **server.go** — removed `GET /{sport}/{entityType}/{id}` (`GetProfilePage`) and
  `GET /{sport}/leagues/{leagueId}/{entityType}/{id}` (`GetLeagueProfilePage`).
- **data.go** — deleted both handlers (90 lines). No private helpers were unique to them.
- **db.go** — deleted the 3 inline statements `nba_profile_page` / `nfl_profile_page` / `football_profile_page` (263 lines).
- **server_test.go** — the two profile-route assertions now expect 404 (was 503).
- **ENDPOINTS.md** — both route sections replaced with REMOVED-(O16) redirect notes pointing at the per-product endpoints.

## Files Changed
- `go/internal/api/server.go`, `server_test.go`, `handler/data.go`, `internal/db/db.go`, `ENDPOINTS.md`

## Verification
- `go vet` clean; `go test ./internal/api/...` PASS.
- Boot on :8001 clean (3 statements gone, no degraded mode).
- Both removed routes → **404**; per-product routes (`/stats`, `/meta`, `/momentum`) and per-product **league**
  routes (`/leagues/8/meta`, `/leagues/8/player/175/momentum`) → **200**.
- Deployed; prod health 200, bundled profile 404, `/stats` 200, no degraded/missing-statement errors.

## Result
O16 ✅ shipped + deployed. The bundled profile surface is fully decommissioned; every client already consumes
the per-product endpoints. Residual (unverifiable from Archbox, but long-expired): the parked Astro standby worker.
