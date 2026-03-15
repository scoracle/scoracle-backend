# Sport Schema Separation Plan

**Date:** 2026-03-14
**Status:** Approved, implementation in progress

## Problem

The monolithic `schema.sql` (1,602 lines) mixes all three sports into shared tables, shared triggers, shared functions, and shared API views. This creates:

1. **Blast radius** — editing an NFL derived stat means opening a file with NBA triggers, Football standings, and cross-sport materialized views.
2. **Ownership boundaries** — each sport will have its own product owner. They need a bounded file they can own without reading or risking other sports.
3. **Branching logic** — `fn_standings` has `CASE WHEN sport = 'FOOTBALL'` branches. `mv_autofill_entities` has 4 UNION ALL branches. These grow with each sport.
4. **Dead code** — 3 Go handler files and 8 prepared statements back endpoints that migrated to PostgREST but were never cleaned up.

## Architecture

**One Neon database. Shared tables in `public`. Sport-specific views and functions in Postgres schemas (`nba`, `nfl`, `football`). PostgREST multi-schema with `Accept-Profile` header.**

```
One Neon PostgreSQL Database
│
├── public schema (shared tables — unchanged)
│   ├── players, player_stats, teams, team_stats
│   ├── stat_definitions, fixtures, leagues, provider_seasons
│   ├── percentile_archive, meta, sports
│   ├── users, user_follows, user_devices, notifications
│   ├── recalculate_percentiles(), get_pending_fixtures(), ...
│   └── notify_milestone_reached() + triggers
│
├── nba schema (PostgREST surface + sport-specific logic)
│   ├── views: players, player_stats, teams, team_stats, standings,
│   │          stat_definitions, autofill_entities (materialized)
│   ├── functions: stat_leaders(), health()
│   ├── trigger functions: compute_derived_player_stats/team_stats()
│   └── triggers on public.player_stats/team_stats WHEN sport='NBA'
│
├── nfl schema (same pattern)
│
└── football schema (same pattern + leagues view)
```

### Why this approach

Tables stay shared in `public` — the `sport` column and JSONB design already handle sport-specific data. No Go code changes needed for the seed pipeline, fixtures, or upserts.

Sport-specific **views** in each schema read from shared tables with `WHERE sport = 'NBA'` baked in. PostgREST exposes each schema directly via `PGRST_DB_SCHEMAS=nba,nfl,football`. The frontend selects the sport with an `Accept-Profile` header.

Sport-specific **functions** (standings sort order, stat leaders, derived-stat triggers) live in the sport's schema and file. No branching, no `CASE WHEN`.

### Why not separate tables or databases

Separate tables per sport (via Postgres schemas) was considered but rejected:
- Requires rewriting every Go upsert, prepared statement, and seed orchestrator
- Changes every PK, index, and ON CONFLICT clause
- 4-5 sessions of high-risk changes across every layer of the stack
- Solves a theoretical problem (table isolation) that JSONB already handles

Separate databases were rejected for container proliferation (one PostgREST per DB).

## File Layout

```
sql/
├── shared.sql       ← public tables, indexes, roles, shared functions, notification infra
├── nba.sql          ← nba schema: views, stat defs, triggers, functions, grants
├── nfl.sql          ← nfl schema: views, stat defs, triggers, functions, grants
├── football.sql     ← football schema: views, stat defs, triggers, functions, grants, leagues
└── platform.sql     ← stub: future user/follows PostgREST surface
```

### Ownership model

| File | Owner | When to edit |
|---|---|---|
| `sql/nba.sql` | NBA product owner | Adding stats, changing derived metrics, adjusting standings sort |
| `sql/nfl.sql` | NFL product owner | Adding stats, changing derived metrics, adjusting standings sort |
| `sql/football.sql` | Football product owner | Adding stats, changing per-90 metrics, adding leagues |
| `sql/shared.sql` | Platform owner | Table structure changes, shared functions, roles |
| `sql/platform.sql` | Platform owner | Future: user follows, notifications PostgREST surface |

## PostgREST Configuration

```dockerfile
# Before
ENV PGRST_DB_SCHEMAS=api

# After
ENV PGRST_DB_SCHEMAS=nba,nfl,football
```

Each sport schema exposes views directly. No `api.*` wrapper layer. The first schema (`nba`) is the default when no `Accept-Profile` header is sent.

### Frontend migration

```
# Before
GET /player_stats?sport=eq.NBA&player_id=eq.123&season=eq.2025

# After
GET /player_stats?player_id=eq.123&season=eq.2025
Accept-Profile: nba
```

Same endpoint names (`/players`, `/player_stats`, `/standings`), but scoped by the `Accept-Profile` header. The `sport` filter parameter is no longer needed.

For RPC functions:
```
# Before
POST /rpc/stat_leaders
Body: { "p_sport": "NBA", "p_season": 2025, "p_stat_name": "pts" }

# After
POST /rpc/stat_leaders
Accept-Profile: nba
Content-Profile: nba
Body: { "p_season": 2025, "p_stat_name": "pts" }
```

## What Gets Removed

### SQL objects removed

| Object | Reason |
|---|---|
| `api` schema + all `api.*` views | Replaced by sport schemas via PostgREST multi-schema |
| `api.stat_leaders()`, `api.health()` | Replaced by sport-specific functions |
| `api.my_follows` + RLS grants on api | Deferred to platform schema |
| `api.sports` view | Frontend handles sport list |
| `fn_standings()` (shared, branching) | Replaced by `nba.standings`, `nfl.standings`, `football.standings` views |
| `fn_stat_leaders()` (shared) | Replaced by `nba.stat_leaders()`, etc. |
| `mv_autofill_entities` (84 lines) | Replaced by per-sport materialized views (~15 lines each) |
| `v_player_profile`, `v_team_profile` | Replaced by per-sport player/team views |
| `api_player_profile()`, `api_team_profile()` | Dead — backed unrouted Go handlers |
| `api_entity_stats()`, `api_available_seasons()` | Dead — backed unrouted Go handlers |

### Go dead code removed

| File | Reason |
|---|---|
| `handler/profile.go` | No route in `server.go` — PostgREST serves profiles |
| `handler/stats.go` | No route in `server.go` — PostgREST serves stats |
| `handler/bootstrap.go` | No route in `server.go` — PostgREST serves autofill |
| 8 prepared statements in `db.go` | Backed the dead handlers above |

### What stays unchanged

- All `public.*` tables — same structure, same PKs, same `sport` column, same indexes
- `go/internal/seed/upsert.go` — same SQL, same sport parameter
- `go/internal/seed/nba.go`, `nfl.go`, `football.go` — unchanged
- `go/internal/fixture/seed.go` — unchanged
- `go/internal/config/config.go` — unchanged
- `go/internal/db/db.go` — 16 active prepared statements unchanged (8 dead ones removed)
- `docker-compose.yml` — unchanged
- Go API server, handlers for news/twitter — unchanged

## Sport File Structure

Each sport file (e.g., `sql/nba.sql`) contains:

```
1.  CREATE SCHEMA IF NOT EXISTS nba;
2.  Stat definition INSERTs (into public.stat_definitions)
3.  Derived-stat trigger functions (in nba schema)
4.  Triggers on public.player_stats/team_stats WHEN sport='NBA'
5.  Views: players, player_stats, teams, team_stats, standings, stat_definitions
6.  Materialized view: autofill_entities
7.  RPC functions: stat_leaders(), health()
8.  Grants to web_anon, web_user
```

### Key view examples

**`nba.standings`** — no branching, hardcoded NBA sort:
```sql
CREATE OR REPLACE VIEW nba.standings AS
SELECT ts.team_id, ts.season, ts.league_id,
       t.name AS team_name, t.short_code AS team_abbr, t.logo_url,
       t.conference, t.division, ts.stats,
       ROUND((ts.stats->>'wins')::numeric /
           NULLIF((ts.stats->>'wins')::integer + (ts.stats->>'losses')::integer, 0), 3
       ) AS win_pct
FROM public.team_stats ts
JOIN public.teams t ON t.id = ts.team_id AND t.sport = ts.sport
WHERE ts.sport = 'NBA';
```

**`football.standings`** — no branching, hardcoded Football sort:
```sql
CREATE OR REPLACE VIEW football.standings AS
SELECT ts.team_id, ts.season, ts.league_id,
       t.name AS team_name, t.short_code AS team_abbr, t.logo_url,
       l.name AS league_name, ts.stats,
       (ts.stats->>'points')::integer AS sort_points,
       (ts.stats->>'goal_difference')::integer AS sort_goal_diff
FROM public.team_stats ts
JOIN public.teams t ON t.id = ts.team_id AND t.sport = ts.sport
LEFT JOIN public.leagues l ON l.id = ts.league_id
WHERE ts.sport = 'FOOTBALL';
```

**`nba.autofill_entities`** — 15 lines vs 84-line UNION ALL:
```sql
CREATE MATERIALIZED VIEW nba.autofill_entities AS
SELECT p.id, 'player'::text AS type, p.name, p.position, p.detailed_position,
       t.short_code AS team_abbr, t.name AS team_name,
       NULL::int AS league_id, NULL::text AS league_name, p.meta
FROM public.players p
LEFT JOIN public.teams t ON t.id = p.team_id AND t.sport = p.sport
WHERE p.sport = 'NBA'
UNION ALL
SELECT t.id, 'team'::text AS type, t.name, t.conference, t.division,
       t.short_code AS team_abbr, NULL, NULL::int, NULL::text, t.meta
FROM public.teams t
WHERE t.sport = 'NBA'
WITH DATA;
```

## Adding a New Sport

### SQL (sport product owner)
1. Copy `sql/nba.sql` as template to `sql/mlb.sql`
2. Replace `nba`/`NBA` with `mlb`/`MLB`
3. Customize stat definitions, derived stats, standings sort
4. Apply to database: `psql -f sql/mlb.sql`

### PostgREST
5. Update `PGRST_DB_SCHEMAS=nba,nfl,football,mlb`, reload PostgREST

### Go (platform owner, one-time wiring)
6. Create provider package: `go/internal/provider/mlbprovider/`
7. Create seed orchestrator: `go/internal/seed/mlb.go`
8. Add `SportRegistry` entry in `config.go`
9. Add `seedMLBCmd()` in `cmd/ingest/main.go`
10. Add fixture case in `fixture/seed.go`
11. Add news config in `thirdparty/news.go`

### What the MLB product owner never touches
- `sql/nba.sql`, `sql/nfl.sql`, `sql/football.sql`
- Any existing sport's Go provider or seed code

## Database Application

### Fresh database
```bash
psql -f sql/shared.sql
psql -f sql/nba.sql
psql -f sql/nfl.sql
psql -f sql/football.sql
```

### Migrating from schema.sql
```bash
# 1. Apply new files (CREATE IF NOT EXISTS / CREATE OR REPLACE — safe)
psql -f sql/shared.sql
psql -f sql/nba.sql
psql -f sql/nfl.sql
psql -f sql/football.sql

# 2. Drop old objects (after verifying new setup works)
psql -c "DROP SCHEMA IF EXISTS api CASCADE;"
psql -c "DROP FUNCTION IF EXISTS fn_standings;"
psql -c "DROP FUNCTION IF EXISTS fn_stat_leaders;"
psql -c "DROP FUNCTION IF EXISTS api_player_profile;"
psql -c "DROP FUNCTION IF EXISTS api_team_profile;"
psql -c "DROP FUNCTION IF EXISTS api_entity_stats;"
psql -c "DROP FUNCTION IF EXISTS api_available_seasons;"
psql -c "DROP VIEW IF EXISTS v_player_profile;"
psql -c "DROP VIEW IF EXISTS v_team_profile;"
psql -c "DROP MATERIALIZED VIEW IF EXISTS mv_autofill_entities;"
```

## Success Metrics

- A sport product owner adds a new stat by editing only their sport's SQL file
- A new sport is added by creating one SQL file + one Go provider + one seed file
- No `CASE WHEN sport = ...` branching in any SQL function or view
- PostgREST serves all sports from one instance via `Accept-Profile` header
- Go connects with one database pool, zero pipeline code changes
- Each sport's standings view is < 15 lines with zero branching
- Each sport's autofill materialized view is < 20 lines with zero UNION branches
