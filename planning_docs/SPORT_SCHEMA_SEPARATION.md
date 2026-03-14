# Sport Schema Separation Plan

**Date:** 2026-03-14
**Status:** Approved plan, not yet implemented
**Supersedes:** `progress_docs/2026-03-13_multi-sport-schema-architecture-plan.md` (which recommended modular files within one DB; this plan goes further with Postgres schema isolation)

## Problem

The monolithic `schema.sql` (1,602 lines) mixes all three sports into shared tables, shared triggers, shared functions, and shared API views. This creates several problems:

1. **Blast radius** — Changing an NFL derived stat means editing a file that also contains NBA triggers, Football standings logic, and cross-sport materialized views.
2. **Ownership boundaries** — As the product grows, each sport will have its own product owner. An NFL product owner should never need to read, understand, or risk breaking NBA or Football code.
3. **Organic growth** — Each sport needs to evolve independently. Football may need per-90 metrics for different positions. NBA may need advanced analytics. NFL may need weekly game-log breakdowns. These expansions should be bounded to the sport that needs them.
4. **`sport` column pollution** — Every table, index, primary key, and query carries a `sport` column that adds filtering overhead and makes every SQL statement longer than it needs to be.
5. **Branching logic** — Functions like `fn_standings` contain `CASE WHEN sport = 'FOOTBALL'` branches that grow with each sport. Triggers use `WHEN (NEW.sport = 'NBA')` guards. The materialized view `mv_autofill_entities` has four `UNION ALL` branches.

## Goal

Enable each sport's database schema, seed pipeline, and API surface to be **owned and evolved independently** by a single product owner, without affecting other sports.

## Architecture Decision

**One Neon database. Separate Postgres schemas per sport. One PostgREST instance.**

```
One Neon PostgreSQL Database
├── nba schema        ← NBA product owner
│   ├── nba.players
│   ├── nba.player_stats
│   ├── nba.teams
│   ├── nba.team_stats
│   ├── nba.stat_definitions
│   ├── nba.fixtures
│   ├── nba.percentile_archive
│   ├── nba.meta
│   ├── nba.mv_autofill_entities
│   ├── nba.compute_derived_player_stats()
│   ├── nba.compute_derived_team_stats()
│   ├── nba.fn_standings()
│   ├── nba.fn_stat_leaders()
│   └── nba.recalculate_percentiles()
│
├── nfl schema        ← NFL product owner
│   ├── nfl.players
│   ├── nfl.player_stats
│   ├── nfl.teams
│   ├── nfl.team_stats
│   ├── nfl.stat_definitions
│   ├── nfl.fixtures
│   ├── nfl.percentile_archive
│   ├── nfl.meta
│   ├── nfl.mv_autofill_entities
│   ├── nfl.compute_derived_player_stats()
│   ├── nfl.compute_derived_team_stats()
│   ├── nfl.fn_standings()
│   ├── nfl.fn_stat_leaders()
│   └── nfl.recalculate_percentiles()
│
├── football schema   ← Football product owner
│   ├── football.players
│   ├── football.player_stats
│   ├── football.teams
│   ├── football.team_stats
│   ├── football.leagues
│   ├── football.provider_seasons
│   ├── football.stat_definitions
│   ├── football.fixtures
│   ├── football.percentile_archive
│   ├── football.meta
│   ├── football.mv_autofill_entities
│   ├── football.compute_derived_player_stats()
│   ├── football.fn_standings()
│   ├── football.fn_stat_leaders()
│   └── football.recalculate_percentiles()
│
├── platform schema   ← Shared (future)
│   ├── platform.users
│   ├── platform.user_follows
│   ├── platform.user_devices
│   └── platform.notifications
│
└── api schema        ← PostgREST public surface
    ├── api.nba_players          → SELECT * FROM nba.players ...
    ├── api.nba_player_stats     → SELECT * FROM nba.player_stats ...
    ├── api.nba_teams            → SELECT * FROM nba.teams ...
    ├── api.nba_team_stats       → SELECT * FROM nba.team_stats ...
    ├── api.nba_standings        → nba.fn_standings(...)
    ├── api.nba_stat_definitions → SELECT * FROM nba.stat_definitions
    ├── api.nba_autofill         → SELECT * FROM nba.mv_autofill_entities
    ├── api.nfl_players          → ...
    ├── api.nfl_player_stats     → ...
    ├── api.nfl_teams            → ...
    ├── api.nfl_team_stats       → ...
    ├── api.nfl_standings        → ...
    ├── api.nfl_stat_definitions → ...
    ├── api.nfl_autofill         → ...
    ├── api.football_players     → ...
    ├── api.football_player_stats→ ...
    ├── api.football_teams       → ...
    ├── api.football_team_stats  → ...
    ├── api.football_standings   → ...
    ├── api.football_stat_definitions → ...
    ├── api.football_leagues     → ...
    ├── api.football_autofill    → ...
    ├── api.sports               → hardcoded or shared catalog view
    ├── api.nba_stat_leaders()   → nba.fn_stat_leaders(...)
    ├── api.nfl_stat_leaders()   → nfl.fn_stat_leaders(...)
    ├── api.football_stat_leaders() → football.fn_stat_leaders(...)
    └── api.health()
```

## Why Not Separate Databases?

Separate Neon databases (one per sport) were considered. The tradeoffs:

| Concern | Postgres Schemas (chosen) | Separate DBs |
|---|---|---|
| PostgREST instances | 1 | 3+ (one per DB) |
| Railway services | 2 (PostgREST + Go API) | 4+ (3 PostgREST + Go API) |
| Neon projects/cost | 1 | 3+ |
| Go connection pools | 1 | 3+ |
| Cross-sport queries | Possible (same DB) | Impossible |
| Future user follows | Same DB, easy joins | Cross-DB coordination |
| Ownership isolation | Strong (schema boundary) | Strongest (DB boundary) |
| Operational overhead | Low | High |

Postgres schemas provide strong isolation (separate tables, separate functions, no accidental cross-contamination) without the operational cost of multiple databases and containers. Each sport's tables are physically separate — `nba.players` and `nfl.players` are different tables, not filtered views of a shared table.

## Why Not Neon Branches?

Neon branches are copy-on-write forks of the same database for dev/test/preview workflows. They are not designed for running parallel production workloads with different schemas. Each branch would start as a full clone of the monolith, still containing all sports, still having the `sport` column everywhere. Branches solve a different problem.

## What Changes in Each Sport Schema

### Tables: The `sport` column disappears

Every sport-specific table drops the `sport` column. It is implied by the schema.

| Table | Current PK | New PK |
|---|---|---|
| `players` | `(id, sport)` | `(id)` |
| `teams` | `(id, sport)` | `(id)` |
| `player_stats` | `(player_id, sport, season, league_id)` | `(player_id, season, league_id)` |
| `team_stats` | `(team_id, sport, season, league_id)` | `(team_id, season, league_id)` |

All `idx_*_sport` indexes are eliminated. All `WHERE sport = $1` conditions disappear.

### NBA/NFL: `league_id` handling

NBA and NFL currently use `league_id = 0` as a sentinel for "single league." Two options:

- **Option A (recommended for Phase 1):** Keep `league_id` column with `DEFAULT 0` to minimize Go code changes. Remove the column in a future cleanup pass.
- **Option B:** Drop `league_id` entirely from NBA/NFL schemas. Requires Go upsert changes.

### Football: `leagues` and `provider_seasons`

Football keeps these tables in `football.*`. They don't exist in the NBA or NFL schemas. This is the organic growth in action — Football has multi-league complexity that NBA/NFL don't need.

### Functions: Sport-specific logic is hardcoded

Functions no longer need `p_sport` parameters or `CASE WHEN` branches.

**`fn_standings` — current (shared, branching):**
```sql
ORDER BY
    CASE WHEN p_sport = 'FOOTBALL' THEN (s.stats->>'points')::INTEGER END DESC NULLS LAST,
    CASE WHEN p_sport = 'FOOTBALL' THEN (s.stats->>'goal_difference')::INTEGER END DESC NULLS LAST,
    CASE WHEN p_sport != 'FOOTBALL' THEN (s.stats->>'wins')::INTEGER END DESC NULLS LAST
```

**`nba.fn_standings` — after (NBA-specific, no branching):**
```sql
ORDER BY (s.stats->>'wins')::INTEGER DESC NULLS LAST
```

**`football.fn_standings` — after (Football-specific, no branching):**
```sql
ORDER BY
    (s.stats->>'points')::INTEGER DESC NULLS LAST,
    (s.stats->>'goal_difference')::INTEGER DESC NULLS LAST,
    (s.stats->>'goals_for')::INTEGER DESC NULLS LAST
```

**`recalculate_percentiles` — current:**
```sql
CREATE FUNCTION recalculate_percentiles(p_sport TEXT, p_season INTEGER, ...)
-- Reads from player_stats WHERE sport = p_sport AND season = p_season
```

**`nba.recalculate_percentiles` — after:**
```sql
CREATE FUNCTION nba.recalculate_percentiles(p_season INTEGER, ...)
-- Reads from nba.player_stats WHERE season = p_season
-- No sport param needed
```

### Triggers: Simplified

**Current:** Each trigger has a `WHEN (NEW.sport = 'NBA')` guard on shared tables.

**After:** Each sport schema has its own trigger on its own table. No guards needed — `nba.player_stats` only contains NBA data.

```sql
-- In sql/nba.sql
CREATE TRIGGER trg_derived_player_stats
    BEFORE INSERT OR UPDATE ON nba.player_stats
    FOR EACH ROW
    EXECUTE FUNCTION nba.compute_derived_player_stats();
```

### Materialized Views: Simplified

**Current `mv_autofill_entities`:** 84 lines, 4 `UNION ALL` branches, conditional logic for Football vs NBA/NFL league resolution.

**After `nba.mv_autofill_entities`:** ~15 lines, single query, no branching:
```sql
CREATE MATERIALIZED VIEW nba.mv_autofill_entities AS
    SELECT p.id, 'player'::text AS type, p.name, p.position,
           t.short_code AS team_abbr, t.name AS team_name, p.meta
    FROM nba.players p
    LEFT JOIN nba.teams t ON t.id = p.team_id
UNION ALL
    SELECT t.id, 'team'::text AS type, t.name, t.conference AS position,
           t.short_code AS team_abbr, NULL, t.meta
    FROM nba.teams t
WITH DATA;
```

## SQL Source File Layout

```
sql/
├── 00_extensions_and_roles.sql   ← shared: extensions, roles, api schema creation
├── nba.sql                       ← complete NBA schema (self-contained)
├── nfl.sql                       ← complete NFL schema (self-contained)
├── football.sql                  ← complete Football schema (self-contained)
├── platform.sql                  ← future: users, follows, devices, notifications
├── api_views.sql                 ← api.* views (sport-prefixed, delegates to sport schemas)
├── api_functions.sql             ← api.* RPC functions (sport-prefixed wrappers)
├── grants.sql                    ← all GRANT statements for PostgREST roles
└── assemble.sh                   ← concatenates all files into schema.sql (generated artifact)
```

### Sport file contents (e.g., `sql/nba.sql`)

Each sport file is fully self-contained and creates all objects in its schema:

```
1. CREATE SCHEMA IF NOT EXISTS nba;
2. Tables: players, player_stats, teams, team_stats, stat_definitions,
   fixtures, percentile_archive, meta
3. Indexes
4. Seed data: stat_definitions INSERTs, sports catalog entry
5. Derived-stat trigger functions + triggers
6. Helper functions: fn_standings, fn_stat_leaders, recalculate_percentiles,
   get_pending_fixtures, mark_fixture_seeded
7. Views: v_player_profile, v_team_profile
8. Materialized view: mv_autofill_entities
9. API response functions: api_player_profile, api_team_profile,
   api_entity_stats, api_available_seasons
10. Notification functions: archive_current_percentiles,
    detect_percentile_changes, notify_milestone_reached + triggers
```

### Ownership model

| File | Owner | When to edit |
|---|---|---|
| `sql/nba.sql` | NBA product owner | Adding NBA stats, changing NBA derived metrics, adjusting NBA standings logic |
| `sql/nfl.sql` | NFL product owner | Adding NFL stats, changing NFL derived metrics, adjusting NFL standings logic |
| `sql/football.sql` | Football product owner | Adding Football stats, changing per-90 metrics, adding new leagues |
| `sql/api_views.sql` | Platform owner | Only when adding/removing a sport or changing the PostgREST contract |
| `sql/grants.sql` | Platform owner | Only when changing auth or adding a new sport's views |
| `sql/platform.sql` | Platform owner | User features, notifications, follows |

## PostgREST Configuration

One PostgREST instance, exposing the `api` schema:

```dockerfile
# postgrest/Dockerfile — no change to instance count
ENV PGRST_DB_SCHEMAS=api
```

The `api` schema contains sport-prefixed views that delegate to sport schemas:

```sql
-- In sql/api_views.sql
CREATE OR REPLACE VIEW api.nba_players AS
SELECT id, name, first_name, last_name, position, ... FROM nba.v_player_profile;

CREATE OR REPLACE VIEW api.nfl_players AS
SELECT id, name, first_name, last_name, position, ... FROM nfl.v_player_profile;

CREATE OR REPLACE VIEW api.football_players AS
SELECT id, name, first_name, last_name, position, ... FROM football.v_player_profile;
```

### Frontend migration

PostgREST URLs change from:
```
GET /player_stats?sport=eq.NBA&player_id=eq.123&season=eq.2025
GET /standings?sport=eq.FOOTBALL&league_id=eq.8&season=eq.2025
```

To:
```
GET /nba_player_stats?player_id=eq.123&season=eq.2025
GET /football_standings?league_id=eq.8&season=eq.2025
```

The `sport` filter is no longer needed — it's embedded in the view name.

### Backward compatibility

During migration, the old generic views can coexist as compatibility wrappers:
```sql
-- Temporary compatibility view (remove after frontend migration)
CREATE OR REPLACE VIEW api.player_stats AS
    SELECT *, 'NBA'::text AS sport FROM nba.player_stats
    UNION ALL
    SELECT *, 'NFL'::text AS sport FROM nfl.player_stats
    UNION ALL
    SELECT *, 'FOOTBALL'::text AS sport FROM football.player_stats;
```

## Go Code Changes

### 1. Config (`go/internal/config/config.go`)

The single `DatabaseURL` remains — it's still one database. No env var changes needed.

The `search_path` on the connection determines which schema is the default. For the API server (which queries all sports), the `search_path` stays as `public`. For ingestion, the sport-specific schema can be set per operation.

### 2. DB layer (`go/internal/db/db.go`)

Prepared statements change to use schema-qualified table/function names:

```go
// Current
"recalculate_percentiles": "SELECT * FROM recalculate_percentiles($1, $2)",

// After — sport-specific statements
"nba_recalculate_percentiles": "SELECT * FROM nba.recalculate_percentiles($1)",
"nfl_recalculate_percentiles": "SELECT * FROM nfl.recalculate_percentiles($1)",
"football_recalculate_percentiles": "SELECT * FROM football.recalculate_percentiles($1)",
```

The `sport` parameter disappears from many functions since the schema implies the sport. The prepared statement name includes the sport prefix.

**Alternative approach:** Use `SET search_path` per operation instead of sport-prefixed statement names. This keeps the Go code closer to its current shape:

```go
// Set search_path before operations
_, _ = pool.Exec(ctx, "SET search_path TO nba, public")
// Then use unqualified names — they resolve to nba.* tables
pool.QueryRow(ctx, "recalculate_percentiles", season)
```

This approach has tradeoffs (search_path is connection-scoped, not transaction-scoped by default) and needs careful handling with connection pools. Schema-qualified names are safer for production.

### 3. Seed upsert (`go/internal/seed/upsert.go`)

Currently uses `config.TeamsTable` ("teams") and passes `sport` as a parameter. After the change:

**Option A — Schema-qualified table names:**
```go
func UpsertTeam(ctx context.Context, pool *pgxpool.Pool, schema string, team provider.Team) error {
    table := schema + ".teams"
    _, err := pool.Exec(ctx, `
        INSERT INTO `+table+` (id, name, short_code, ...)
        VALUES ($1,$2,$3,...)
        ON CONFLICT (id) DO UPDATE SET ...`,
        team.ID, team.Name, ...)
}
```

The `sport` parameter becomes a `schema` parameter. The `ON CONFLICT` clause simplifies because the PK is now just `(id)` instead of `(id, sport)`.

**Option B — Separate upsert registrations per sport:**
Each sport registers its own prepared INSERT statements. More explicit but more boilerplate.

**Recommendation:** Option A. The upsert functions stay generic, just parameterized by schema name instead of sport string.

### 4. Seed orchestrators (`go/internal/seed/nba.go`, etc.)

Minimal changes. The sport string `"NBA"` becomes the schema string `"nba"`:

```go
// Current
UpsertTeam(ctx, pool, sportNBA, team)

// After
UpsertTeam(ctx, pool, "nba", team)
```

### 5. Fixture seeding (`go/internal/fixture/seed.go`)

The `switch f.Sport` block stays structurally the same. The sport strings map to schema names:

```go
case "NBA":
    seedResult = seedNBAFixture(ctx, pool, deps.NBAHandler, f, logger)
```

Inside `seedNBAFixture`, upsert calls change from `"NBA"` to `"nba"` (schema name).

### 6. Handler/news entity lookups

Currently uses prepared statements like `player_name_lookup` with `WHERE sport = $2`. These become schema-qualified:

```go
// Current
"player_name_lookup": "SELECT name FROM players WHERE id = $1 AND sport = $2"

// After — one per sport, or dynamically qualified
"nba_player_name":      "SELECT name FROM nba.players WHERE id = $1",
"nfl_player_name":      "SELECT name FROM nfl.players WHERE id = $1",
"football_player_name": "SELECT name FROM football.players WHERE id = $1",
```

Or, since these lookups are rare (news enrichment), use dynamic SQL:

```go
func playerNameLookup(ctx context.Context, pool *pgxpool.Pool, sport string, id int) (string, error) {
    schema := strings.ToLower(sport)
    var name string
    err := pool.QueryRow(ctx, "SELECT name FROM "+schema+".players WHERE id = $1", id).Scan(&name)
    return name, err
}
```

## Adding a New Sport: The Checklist

After this refactor, adding a new sport (e.g., MLB) requires:

### SQL (owned by MLB product owner)
1. Create `sql/mlb.sql` — copy the structure from `sql/nba.sql` as a template
2. Define `mlb.stat_definitions` with MLB-specific stats
3. Write `mlb.compute_derived_player_stats()` with MLB-specific derived metrics
4. Write `mlb.fn_standings()` with MLB-specific sort order
5. Apply to database: `psql -f sql/mlb.sql`

### API surface (platform owner, one-time wiring)
6. Add MLB views to `sql/api_views.sql` (`api.mlb_players`, `api.mlb_player_stats`, etc.)
7. Add MLB RPC to `sql/api_functions.sql` (`api.mlb_stat_leaders()`)
8. Add grants in `sql/grants.sql`

### Go (one-time wiring)
9. Create provider package: `go/internal/provider/mlbprovider/`
10. Create seed orchestrator: `go/internal/seed/mlb.go`
11. Add `SportRegistry` entry in `go/internal/config/config.go`
12. Add `seedMLBCmd()` in `go/cmd/ingest/main.go`
13. Add case to fixture `switch` in `go/internal/fixture/seed.go`
14. Add news config in `go/internal/thirdparty/news.go`

### What the MLB product owner never touches
- `sql/nba.sql`, `sql/nfl.sql`, `sql/football.sql`
- `go/internal/seed/nba.go`, `nfl.go`, `football.go`
- `go/internal/provider/bdl/`, `sportmonks/`
- Any existing sport's derived stats, standings, or stat definitions

## Phased Rollout

### Phase 1: Create sport schema files (no runtime changes)

- Write `sql/nba.sql`, `sql/nfl.sql`, `sql/football.sql` by extracting and simplifying from `schema.sql`
- Write `sql/00_extensions_and_roles.sql`, `sql/api_views.sql`, `sql/api_functions.sql`, `sql/grants.sql`
- Write `sql/platform.sql` as a stub
- Write `sql/assemble.sh` to produce a combined `schema.sql`
- Validate: assembled output produces identical database objects as current `schema.sql`

**Exit criteria:** New SQL files exist. No runtime changes. Current `schema.sql` still works.

### Phase 2: Apply schemas to database

- Apply the new schema files to a test Neon branch
- Verify all PostgREST endpoints return identical results
- Verify all seed operations work
- Create compatibility views for old generic endpoints during transition

**Exit criteria:** Test environment runs on new schemas. API responses unchanged.

### Phase 3: Update Go code

- Update `go/internal/seed/upsert.go` — schema parameter instead of sport string
- Update `go/internal/db/db.go` — schema-qualified prepared statements
- Update seed orchestrators — `"nba"` schema instead of `"NBA"` sport
- Update fixture seeding
- Update news entity lookups

**Exit criteria:** Go code works against new schema layout. All tests pass.

### Phase 4: Update frontend

- Migrate PostgREST calls from generic views (`/player_stats?sport=eq.NBA`) to sport-prefixed views (`/nba_player_stats`)
- Remove `sport` filter params from API calls
- Update autofill/bootstrap to use sport-specific views

**Exit criteria:** Frontend uses new API surface. Old compatibility views can be removed.

### Phase 5: Cleanup

- Remove compatibility views
- Remove old `schema.sql` (or keep as generated artifact only)
- Update `CLAUDE.md` and documentation
- Remove `sport` column references from all Go code

**Exit criteria:** No legacy references remain. Clean codebase.

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Breaking PostgREST during migration | Compatibility views maintain old endpoints until frontend migrates |
| Schema-qualified SQL breaks prepared statements | Test all statements against new schema in Phase 2 before Go changes |
| Duplicated table definitions across sport files | Acceptable — each sport file is small (~200-300 lines). Could template if it becomes a problem. |
| `search_path` confusion with connection pools | Use explicit schema-qualified names, not `SET search_path` |
| Platform tables (users, follows) need cross-sport access | `platform` schema lives in same DB — can JOIN or query any sport schema |
| Grants become verbose (3x views to grant) | Automate with `GRANT SELECT ON ALL TABLES IN SCHEMA api TO web_anon` |

## Estimated Effort

| Phase | Effort | Risk |
|---|---|---|
| Phase 1: SQL files | 1 session | None (no runtime change) |
| Phase 2: Apply + validate | 1 session | Low (test branch) |
| Phase 3: Go code | 1-2 sessions | Medium (prepared statement changes) |
| Phase 4: Frontend | 1 session | Low (additive, then remove old) |
| Phase 5: Cleanup | 0.5 session | None |
| **Total** | **4-5 sessions** | |

## Success Metrics

- A sport product owner can add a new stat by editing only their sport's SQL file
- A new sport can be added by creating one SQL file + one provider package + one seed file
- No `CASE WHEN sport = ...` branching remains in SQL
- No `sport` column exists in sport-specific tables
- PostgREST serves all sports from one instance
- Go connects with one database pool
- Each sport's `fn_standings` is < 15 lines with zero branching
- Each sport's `mv_autofill_entities` is < 20 lines with zero UNION branches
