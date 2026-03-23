# Session: Consolidate Go API and Remove PostgREST Runtime
**Date:** 2026-03-22

## Goals
- Consolidate the public API under Go with simple sport-scoped resource routes.
- Keep Go thin and keep Postgres as the response-shaping engine.
- Remove PostgREST runtime and config from the repository.
- Align tests and docs with the new architecture.

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| Use `/api/v1/{sport}/players/{id}` and `/api/v1/{sport}/teams/{id}` as canonical page-shaped routes | Keeps URLs intuitive while returning curated page payloads. |
| Keep page assembly in SQL prepared statements | Preserves Postgres-as-serializer architecture and avoids logic creep in Go handlers. |
| Keep `go/internal/thirdparty/` unchanged | Clear existing boundary for external integrations. |
| Remove PostgREST runtime files and compose service | Clean break to a single public API runtime. |

## Accomplishments
### Created
- `go/internal/api/handler/data.go` — Added thin handlers for sport data routes:
  - players, teams, standings, leaders, search, stat-definitions
  - football leagues endpoint
  - shared validation/caching/etag helpers for data responses
- `progress_docs/2026-03-22_consolidate-go-remove-postgrest.md` — Session summary.

### Updated
- `go/internal/db/db.go` — Added prepared statements for all new sport data page endpoints.
- `go/internal/cache/cache.go` — Added `TTLData` constant for data endpoint caching.
- `go/internal/api/server.go` — Removed PostgREST spec proxy; added sport-scoped data routes.
- `go/internal/api/server_test.go` — Updated routing tests for unified API ownership.
- `go/internal/config/config.go` — Removed PostgREST-specific config fields.
- `go/internal/config/config_test.go` — Removed obsolete PostgREST config assertions.
- `go/cmd/api/main.go` — Updated API description annotation to unified API scope.
- `go/docs/docs.go`, `go/docs/swagger.json`, `go/docs/swagger.yaml` — Regenerated Swagger docs for new endpoint surface.
- `.env.example` — Rewritten to match active Go/Python configuration only.
- `docker-compose.yml` — Removed PostgREST service; kept API + seed services.
- `README.md` — Rewritten for unified Go API architecture.
- `ENDPOINTS.md` — Rewritten to single-origin, sport-scoped route contracts.
- `AGENTS.md` — Updated architecture and route guidance.
- `CLAUDE.md` — Updated architecture rules to unified Go API runtime.

### Cleaned Up
- Deleted `postgrest/Dockerfile`.
- Deleted `postgrest/entrypoint.sh`.
- Deleted `postgrest/railway.toml`.

## Verification
- `cd go && go test ./...` passed.
- `cd go && go build ./cmd/api` passed.

## Quick Reference
- Core routes now live under:
  - `/api/v1/{sport}/players/{id}`
  - `/api/v1/{sport}/teams/{id}`
  - `/api/v1/{sport}/standings`
  - `/api/v1/{sport}/leaders`
  - `/api/v1/{sport}/search`
  - `/api/v1/{sport}/stat-definitions`
  - `/api/v1/football/leagues`
