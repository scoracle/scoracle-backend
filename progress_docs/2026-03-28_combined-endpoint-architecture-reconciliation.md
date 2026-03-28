# Session: Combined Endpoint Architecture Reconciliation
**Date:** 2026-03-28

## Goals
- Reconcile parallel local changes into one coherent endpoint architecture.
- Ensure the codebase, Swagger, and documentation all reflect the same contract.
- Prepare a single clean push so the team is aligned.

## Decisions Made
| Decision | Rationale |
|---|---|
| Adopt a clean-break endpoint surface (canonical only) | Removes ambiguity and prevents drift between route families. |
| Keep canonical routes as `profile`, `meta`, `health` per sport | Matches current product direction and sport-autonomy goals. |
| Keep league-scoped route family for explicit league context | Supports football and future multi-league sports growth. |
| Remove legacy-route/deprecation bridge from the final merged state | Current local state had already moved to clean break and docs/tests reflected this direction. |
| Keep integration routes (`news`, `twitter`) unchanged | Preserves non-data endpoint stability while data surface evolves. |

## Final Endpoint Surface

### Canonical per-sport routes
- `GET /api/v1/{sport}/{entityType}/{id}`
- `GET /api/v1/{sport}/meta`
- `GET /api/v1/{sport}/health`

### League-scoped routes
- `GET /api/v1/{sport}/leagues/{leagueId}/{entityType}/{id}`
- `GET /api/v1/{sport}/leagues/{leagueId}/meta`
- `GET /api/v1/{sport}/leagues/{leagueId}/health`

### Integrations
- `GET /api/v1/news/status`
- `GET /api/v1/news/{entityType}/{entityID}`
- `GET /api/v1/twitter/status`
- `GET /api/v1/twitter/journalist-feed`

## Accomplishments
### Router and handlers
- Confirmed canonical + league-scoped routes in:
  - `go/internal/api/server.go`
  - `go/internal/api/handler/data.go`
- Confirmed clean-break route tests in:
  - `go/internal/api/server_test.go`

### SQL contracts
- Confirmed prepared statements are canonical-only in:
  - `go/internal/db/db.go`
  - `*_profile_page`
  - `*_meta_page`
  - `*_health_page`

### Docs and Swagger consistency
- Confirmed and aligned:
  - `ENDPOINTS.md`
  - `README.md`
  - `go/docs/docs.go`
  - `go/docs/swagger.json`
  - `go/docs/swagger.yaml`
- Updated architecture language in `CLAUDE.md` to remove outdated legacy endpoint wording.

### Cleanup performed during reconciliation
- Removed previous progress note that described a different transitional state:
  - `progress_docs/2026-03-28_sport-autonomous-endpoint-architecture.md`
- Replaced it with this combined reconciliation summary so the history reflects the final merged direction.

## Verification
- `cd go && go test ./...` ✅
- `cd go && go build -o bin/scoracle-api ./cmd/api` ✅

## Notes
- An untracked local binary exists at `go/api` (not committed).
- This session consolidated work from overlapping local edits so one push can represent the final contract.
