# 2026-07-05 - DB-First Leaderboard/Profile Sync

## Goal

Move roster-style discovery out of team profile and into the leaderboard cohort model while keeping backend product endpoints sport-scoped.

## What Changed

- Extended `/api/v1/{sport}/leaderboard` with shared cohort filters: `team_id`, `position_group`, `league_id`, `conference`, `division`, `position`, `season`, `scope`, and `limit`.
- Added `/leaderboard/momentum` as the product-facing alias for the existing risers board; `/leaderboard/trending` remains wired for compatibility.
- Reworked the Rating leaderboard SQL so `entity_type=player&team_id=...` reads `team_rosters` and includes active roster members even when rating/fantasy product data is null.
- Applied cohort filtering to Vibe, Sigil, News, Transfers, and Momentum board statements.
- Marked `/team/{id}/roster` as legacy compatibility in docs.

## Files Changed

- `go/internal/api/handler/data.go`
- `go/internal/api/server.go`
- `go/internal/db/db.go`
- `go/docs/docs.go`
- `go/docs/swagger.json`
- `go/docs/swagger.yaml`
- `ENDPOINTS.md`
- `README.md`

## Verification

- `GOCACHE=/tmp/scoracle-go-cache go test ./internal/api ./internal/api/handler ./internal/api/respond ./internal/db`
- `GOCACHE=/tmp/scoracle-go-cache go build -o bin/scoracle-api ./cmd/api`
- `cargo test --lib`
- `cargo build --bin scoracle-cognition --bin statcommentary`

`go test ./internal/api/... ./internal/db/...` was also attempted with a writable `GOCACHE`; it compiled but the existing `internal/api/opencodeproxy` test could not open a local `httptest` socket in the sandbox.

## Result

Backend leaderboard contracts now support DB-first scoped discovery, including full team roster inclusion through the leaderboard.

## Follow-Up

- Regenerate Swagger with `swag` when the local binary is available. Networked `go run github.com/swaggo/swag/cmd/swag@v1.16.6` was blocked by policy because it downloads and executes third-party code.
