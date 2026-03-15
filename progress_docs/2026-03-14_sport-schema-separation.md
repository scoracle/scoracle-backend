# Session: Sport Schema Separation
**Date:** 2026-03-14

## Goals
- Split the monolithic `schema.sql` (1,602 lines) into per-sport SQL files
- Enable independent ownership per sport via Postgres schemas and PostgREST multi-schema
- Eliminate branching functions (`CASE WHEN sport = 'FOOTBALL'`)
- Remove dead Go handler code left over from PostgREST migration
- Configure PostgREST to use `Accept-Profile` header for sport selection

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| Keep shared tables in `public` schema | JSONB design already handles sport-specific data. Avoids rewriting all Go upserts, PKs, and indexes. |
| Use Postgres schemas for views/functions only | Each sport schema holds PostgREST-facing views and sport-specific functions. Tables stay shared. |
| Use PostgREST multi-schema with `Accept-Profile` header | Eliminates the `api.*` wrapper layer entirely. Each sport schema is self-contained. |
| Remove `api` schema | No longer needed. Sport schemas are exposed directly by PostgREST. |
| Remove dead Go handlers (profile.go, stats.go, bootstrap.go) | These backed endpoints that migrated to PostgREST but were never cleaned up. No routes reference them. |
| Sport-specific standings/autofill with zero branching | Each sport has its own standings view with hardcoded sort order and its own autofill materialized view. |

## Accomplishments
### Created
- `sql/shared.sql` — public tables, indexes, roles, shared functions (percentiles, fixtures, notifications)
- `sql/nba.sql` — NBA schema: views, stat definitions, derived-stat triggers, standings, stat_leaders(), autofill, grants
- `sql/nfl.sql` — NFL schema: same pattern with NFL-specific derived stats (td_int_ratio, catch_pct) and ties in standings
- `sql/football.sql` — Football schema: same pattern with per-90 metrics, league context in views, league-aware autofill
- `sql/platform.sql` — stub documenting future user/follows PostgREST surface
- `progress_docs/2026-03-14_sport-schema-separation.md`

### Updated
- `planning_docs/SPORT_SCHEMA_SEPARATION.md` — rewritten to reflect the simpler approach (shared tables + sport schemas for views/functions, not full table separation)
- `CLAUDE.md` — PostgREST section updated for multi-schema mode, design rule #4 updated for per-sport schema separation
- `postgrest/Dockerfile` — `PGRST_DB_SCHEMAS=api` changed to `PGRST_DB_SCHEMAS=nba,nfl,football`
- `go/internal/db/db.go` — removed 8 dead prepared statements (api_player_profile, api_team_profile, api_entity_stats, api_available_seasons, stat_definitions, fn_stat_leaders, fn_standings, autofill_entities)

### Cleaned Up
- Removed `go/internal/api/handler/profile.go` (dead — no route in server.go)
- Removed `go/internal/api/handler/stats.go` (dead — no route in server.go)
- Removed `go/internal/api/handler/bootstrap.go` (dead — no route in server.go)
- Removed `schema.sql` (1,602-line monolith, fully superseded by `sql/` directory)
- Dropped old `api` schema from production database (replaced by sport schemas)
- Updated `README.md` — removed dead endpoint docs, updated schema section and codebase tree
- Updated `go/internal/config/config.go` comment to reference `sql/shared.sql`

## Quick Reference
```bash
# Apply schema to fresh database
psql -f sql/shared.sql
psql -f sql/nba.sql
psql -f sql/nfl.sql
psql -f sql/football.sql

# PostgREST multi-schema config
PGRST_DB_SCHEMAS=nba,nfl,football

# Frontend sport selection (via header, not query param)
GET /player_stats?player_id=eq.123&season=eq.2025
Accept-Profile: nba
```

## File Layout After This Session
```
sql/
├── shared.sql          ← public tables, indexes, roles, shared functions
├── nba.sql             ← nba schema: views, stat defs, triggers, functions, grants
├── nfl.sql             ← nfl schema: views, stat defs, triggers, functions, grants
├── football.sql        ← football schema: views, stat defs, triggers, functions, grants, leagues
└── platform.sql        ← stub for future user/follows PostgREST surface

go/internal/api/handler/
├── handler.go          ← shared handler struct (unchanged)
├── news.go             ← news endpoints (unchanged)
└── twitter.go          ← twitter endpoints (unchanged)
    (profile.go, stats.go, bootstrap.go removed — dead code)
```
