# Session: Remove Legacy PostgREST Endpoints & Fix CORS
**Date:** 2026-03-21

## Goals
- Remove legacy `player_stats`, `team_stats`, `players`, `teams` views that PostgREST was still exposing
- Fix CORS errors when the frontend hits PostgREST endpoints
- Fix Swagger UI cross-origin fetch of PostgREST OpenAPI spec

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| Add `DROP VIEW IF EXISTS` for all 4 legacy views in each sport schema | The March 16 consolidation replaced them with `player`/`team` views but never dropped the old ones — PostgREST auto-exposes all views in a schema, so they were still accessible |
| Set `PGRST_SERVER_CORS_ALLOWED_ORIGINS=*` in PostgREST Dockerfile | PostgREST had no CORS origin config, so browsers blocked cross-origin requests entirely. Default `*` works for dev; production can override via env var |
| Proxy PostgREST OpenAPI spec through Go API at `/docs/postgrest.json` | Swagger UI was fetching the PostgREST spec cross-origin (different Railway service URL), causing CORS errors. Proxying makes it same-origin, eliminating the issue entirely |

## Accomplishments
### Updated
- `sql/nba.sql` — Added `DROP VIEW IF EXISTS` for `nba.players`, `nba.player_stats`, `nba.teams`, `nba.team_stats`
- `sql/nfl.sql` — Added `DROP VIEW IF EXISTS` for `nfl.players`, `nfl.player_stats`, `nfl.teams`, `nfl.team_stats`
- `sql/football.sql` — Added `DROP VIEW IF EXISTS` for `football.players`, `football.player_stats`, `football.teams`, `football.team_stats`
- `postgrest/Dockerfile` — Added `PGRST_SERVER_CORS_ALLOWED_ORIGINS="*"` to enable CORS
- `go/internal/api/server.go` — Added `GET /docs/postgrest.json` proxy route (fetches PostgREST spec server-side, cached 30min); updated Swagger UI config to use local proxy path instead of external URL

## Quick Reference
After deploying, re-run the sport schema SQL files against the database to drop the legacy views:
```sql
\i sql/nba.sql
\i sql/nfl.sql
\i sql/football.sql
```

PostgREST CORS origin can be restricted in production by setting:
```
PGRST_SERVER_CORS_ALLOWED_ORIGINS=https://yourdomain.com
```
