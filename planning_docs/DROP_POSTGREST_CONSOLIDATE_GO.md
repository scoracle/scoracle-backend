# Plan: Drop PostgREST — Consolidate Data Endpoints into Go API

**Status:** Draft — awaiting decision
**Author:** Claude (planning session 2026-03-21)

---

## Motivation

PostgREST v13 has unfixed CORS bugs:
- [#3027](https://github.com/PostgREST/postgrest/issues/3027) — general CORS breakage
- [#3551](https://github.com/PostgREST/postgrest/issues/3551) — hardcoded `Access-Control-Allow-Headers` doesn't include `Accept-Profile`, the header PostgREST itself needs for multi-schema mode

Browsers cannot call PostgREST directly. The Go API already proxies the PostgREST OpenAPI spec to work around this. Consolidating eliminates the workaround entirely.

Additionally, this project uses **curated SQL views and functions** — not ad-hoc filtering — so PostgREST's main value-add (automatic REST from arbitrary tables) isn't leveraged. The Go API already has the handler pattern, CORS middleware, caching, and pgx connection pool needed to serve these endpoints directly.

### Benefits
- **One service instead of two** — simpler deployment, one Dockerfile, one health check
- **CORS solved permanently** — Go's `rs/cors` middleware handles it correctly
- **Unified Swagger spec** — single `/docs/` instead of multi-spec dropdown
- **No proxy layer** — direct Postgres queries instead of Go → PostgREST → Postgres
- **Consistent caching/ETags** — all endpoints use the same in-memory cache

### Costs
- ~200 lines of new Go handler code
- ~18 new prepared statements in `db.go`
- Must keep SQL views unchanged (they are the contract)

---

## Architecture After

```
Frontend (Astro)
    └── Go API (:8000)
            ├── /api/v1/{sport}/player         → Postgres view
            ├── /api/v1/{sport}/team           → Postgres view
            ├── /api/v1/{sport}/standings      → Postgres view
            ├── /api/v1/{sport}/stat-leaders   → Postgres function
            ├── /api/v1/{sport}/autofill       → Postgres materialized view
            ├── /api/v1/{sport}/stat-definitions → Postgres view
            ├── /api/v1/football/leagues       → Postgres view
            ├── /api/v1/news/*                 → Google News / NewsAPI
            ├── /api/v1/twitter/*              → X API
            └── /docs/*                        → Swagger UI (single spec)
```

Data flow unchanged: **Python seeds → Postgres handles/manipulates → Go serves**

The `{sport}` path param replaces the PostgREST `Accept-Profile` header. Valid values: `nba`, `nfl`, `football`.

---

## Route Design

| Route | Method | Query Params | Returns | DB Source |
|-------|--------|-------------|---------|-----------|
| `/api/v1/{sport}/player` | GET | `id` (required), `season` | Single player profile + stats JSON object | `{sport}.player` view |
| `/api/v1/{sport}/team` | GET | `id` (required), `season` | Single team profile + stats JSON object | `{sport}.team` view |
| `/api/v1/{sport}/standings` | GET | `season` (required), `conference`, `division`, `league_id` | Array of team standings | `{sport}.standings` view |
| `/api/v1/{sport}/stat-leaders` | GET | `season` (required), `stat` (required), `limit`, `position`, `league_id` | Array of ranked players | `{sport}.stat_leaders()` function |
| `/api/v1/{sport}/stat-definitions` | GET | `entity_type` | Array of stat definitions | `{sport}.stat_definitions` view |
| `/api/v1/{sport}/autofill` | GET | `q` (required) | Array of matching players/teams | `{sport}.autofill_entities` materialized view |
| `/api/v1/football/leagues` | GET | `active`, `benchmark` | Array of leagues | `football.leagues` view |

### Sport Validation

A helper function validates `{sport}` against the allowed list (`nba`, `nfl`, `football`). Unknown sports return `400 Bad Request`. The sport value is used to select the correct prepared statement name (e.g., `nba_player`, `nfl_player`, `football_player`).

### Response Format

All endpoints follow the existing Postgres-as-serializer pattern: the prepared statement returns JSON bytes from `row_to_json()` / `json_agg(row_to_json())`, and the handler passes raw `[]byte` straight to the HTTP response via `respond.WriteJSON()`. No struct scanning, no marshaling.

---

## Existing SQL Views (Unchanged)

These views already exist in the per-sport schema files. They will NOT be modified — the Go handlers query them directly.

### Per-Sport Views (identical structure across nba/nfl/football)

| View | Type | Key Columns | Notes |
|------|------|-------------|-------|
| `{sport}.player` | VIEW | id, name, position, team (JSON), season, stats (JSONB), percentiles (JSONB) | Combined profile + stats |
| `{sport}.team` | VIEW | id, name, short_code, logo_url, season, stats (JSONB), percentiles (JSONB) | Combined profile + stats |
| `{sport}.standings` | VIEW | team_id, season, league_id, team_name, stats (JSONB), win_pct/sort_points | Ordered by wins/points |
| `{sport}.stat_definitions` | VIEW | id, key_name, display_name, entity_type, category | Filtered to sport |
| `{sport}.autofill_entities` | MATERIALIZED VIEW | id, type, name, position, team_abbr, team_name | Search index |
| `{sport}.stat_leaders()` | FUNCTION | p_season, p_stat_name, p_limit, p_position, p_league_id | Returns rank, player_id, name, stat_value |

### Football-Only

| View | Type | Key Columns |
|------|------|-------------|
| `football.leagues` | VIEW | id, name, country, logo_url, is_benchmark, is_active |

### Differences Between Sports

- **Football** `player` and `team` views include a `league` JSON column (nba/nfl don't)
- **Football** `standings` orders by `sort_points DESC, sort_goal_diff DESC` instead of `win_pct`
- **Football** has a `leagues` endpoint (nba/nfl don't have leagues)
- **NFL** `standings` handles ties in win_pct calculation
- All `stat_leaders()` functions have the same signature across sports

---

## Implementation Details

### Step 1: New File — `go/internal/api/handler/stats.go`

Seven handler functions, each following the established pattern from `news.go`/`twitter.go`:

```
1. Parse and validate request params
2. Build cache key, check cache (return early on hit or ETag match)
3. Call a prepared statement that returns JSON bytes
4. Store in cache, write response with respond.WriteJSON()
```

#### Handler Functions

**`GetPlayer(w, r)`**
- Path param: `{sport}`
- Query params: `id` (required int), `season` (optional int)
- Prepared stmt: `{sport}_player`
- Returns single JSON object — use `row_to_json()`
- 404 if no rows returned

**`GetTeam(w, r)`**
- Path param: `{sport}`
- Query params: `id` (required int), `season` (optional int)
- Prepared stmt: `{sport}_team`
- Returns single JSON object
- 404 if no rows returned

**`GetStandings(w, r)`**
- Path param: `{sport}`
- Query params: `season` (required int), `conference` (optional), `division` (optional), `league_id` (optional int)
- Prepared stmt: `{sport}_standings`
- Returns JSON array — use `COALESCE(json_agg(...), '[]'::json)`
- Empty array `[]` if no rows

**`GetStatLeaders(w, r)`**
- Path param: `{sport}`
- Query params: `season` (required int), `stat` (required string), `limit` (optional int, default 25), `position` (optional), `league_id` (optional int)
- Prepared stmt: `{sport}_stat_leaders`
- Calls the existing RPC function via prepared statement
- Returns JSON array

**`GetStatDefinitions(w, r)`**
- Path param: `{sport}`
- Query params: `entity_type` (optional, `player` or `team`)
- Prepared stmt: `{sport}_stat_definitions`
- Returns JSON array

**`GetAutofill(w, r)`**
- Path param: `{sport}`
- Query params: `q` (required, 1-100 chars)
- Prepared stmt: `{sport}_autofill`
- Searches by `ILIKE '%' || $1 || '%'` on `name` column
- Returns JSON array, limited to 20 results

**`GetLeagues(w, r)`**
- No sport param — football-only, hardcoded route
- Query params: `active` (optional bool), `benchmark` (optional bool)
- Prepared stmt: `football_leagues`
- Returns JSON array

#### Sport Validation Helper

```go
var validSports = map[string]bool{"nba": true, "nfl": true, "football": true}

func parseSport(w http.ResponseWriter, r *http.Request) (string, bool) {
    sport := chi.URLParam(r, "sport")
    if !validSports[sport] {
        respond.WriteError(w, http.StatusBadRequest, "INVALID_SPORT",
            "sport must be one of: nba, nfl, football")
        return "", false
    }
    return sport, true
}
```

#### Cache TTL

Add a constant in `cache/cache.go`:

```go
const TTLStats = 5 * time.Minute
```

Stats data changes less frequently than news (seeded in batches), so 5 minutes is appropriate. The existing `TTLNews` is 10 minutes.

### Step 2: Update — `go/internal/db/db.go`

Add ~18 prepared statements (6 per sport) plus 1 for football leagues.

#### Naming Convention

`{sport}_{endpoint}` — e.g., `nba_player`, `nfl_standings`, `football_stat_leaders`

#### SQL Patterns

**Single-row endpoints (player, team):**
```sql
-- "nba_player"
SELECT row_to_json(t)
FROM nba.player t
WHERE t.id = $1
  AND ($2::int IS NULL OR t.season = $2)
LIMIT 1
```

**Array endpoints (standings, stat_definitions):**
```sql
-- "nba_standings"
SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json)
FROM nba.standings t
WHERE t.season = $1
  AND ($2::text IS NULL OR t.conference = $2)
  AND ($3::text IS NULL OR t.division = $3)
```

**RPC function (stat_leaders):**
```sql
-- "nba_stat_leaders"
SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json)
FROM nba.stat_leaders($1, $2, $3, $4, $5) t
```
Parameters: `p_season`, `p_stat_name`, `p_limit`, `p_position`, `p_league_id`

**Search (autofill):**
```sql
-- "nba_autofill"
SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json)
FROM (
  SELECT * FROM nba.autofill_entities
  WHERE name ILIKE '%' || $1 || '%'
  LIMIT 20
) t
```

**Football-only (leagues):**
```sql
-- "football_leagues"
SELECT COALESCE(json_agg(row_to_json(t)), '[]'::json)
FROM football.leagues t
WHERE ($1::bool IS NULL OR t.is_active = $1)
  AND ($2::bool IS NULL OR t.is_benchmark = $2)
```

#### Full Statement List

| Name | Endpoint | Parameters |
|------|----------|------------|
| `nba_player` | player | $1 id, $2 season (nullable) |
| `nba_team` | team | $1 id, $2 season (nullable) |
| `nba_standings` | standings | $1 season, $2 conference (nullable), $3 division (nullable) |
| `nba_stat_leaders` | stat-leaders | $1 season, $2 stat, $3 limit, $4 position (nullable), $5 league_id |
| `nba_stat_definitions` | stat-definitions | $1 entity_type (nullable) |
| `nba_autofill` | autofill | $1 query string |
| `nfl_player` | player | (same as nba) |
| `nfl_team` | team | (same as nba) |
| `nfl_standings` | standings | (same as nba) |
| `nfl_stat_leaders` | stat-leaders | (same as nba) |
| `nfl_stat_definitions` | stat-definitions | (same as nba) |
| `nfl_autofill` | autofill | (same as nba) |
| `football_player` | player | (same as nba) |
| `football_team` | team | (same as nba) |
| `football_standings` | standings | $1 season, $2 league_id (nullable) |
| `football_stat_leaders` | stat-leaders | (same as nba) |
| `football_stat_definitions` | stat-definitions | (same as nba) |
| `football_autofill` | autofill | (same as nba) |
| `football_leagues` | leagues | $1 is_active (nullable), $2 is_benchmark (nullable) |

**Note on football standings:** Football standings filter by `league_id` instead of `conference`/`division`. The football standings query will differ slightly from nba/nfl. Consider whether to keep a uniform parameter list with sport-specific SQL, or have a slightly different prepared statement for football standings.

### Step 3: Update — `go/internal/api/server.go`

#### Add Routes

```go
r.Route("/api/v1", func(r chi.Router) {
    // Stats — sport-scoped
    r.Route("/{sport}", func(r chi.Router) {
        r.Get("/player", h.GetPlayer)
        r.Get("/team", h.GetTeam)
        r.Get("/standings", h.GetStandings)
        r.Get("/stat-leaders", h.GetStatLeaders)
        r.Get("/stat-definitions", h.GetStatDefinitions)
        r.Get("/autofill", h.GetAutofill)
    })

    // Football-only
    r.Get("/football/leagues", h.GetLeagues)

    // News (unchanged)
    r.Get("/news/status", h.GetNewsStatus)
    r.Get("/news/{entityType}/{entityID}", h.GetEntityNews)

    // Twitter (unchanged)
    r.Get("/twitter/journalist-feed", h.GetJournalistFeed)
    r.Get("/twitter/status", h.GetTwitterStatus)
})
```

**Routing concern:** The `/{sport}` group will match `news`, `twitter`, and `football` as sport values. Chi resolves this correctly because `/news/status` and `/twitter/journalist-feed` are registered as literal routes before the `/{sport}` wildcard, and Chi uses a radix tree that prefers exact matches. However, **verify this in tests** — if there's ambiguity, move the sport routes to a sub-path like `/api/v1/stats/{sport}/player` or register the sport routes last.

#### Update CORS

Add `Authorization` to `AllowedHeaders` for future JWT auth:

```go
AllowedHeaders: []string{
    "Accept", "Accept-Encoding", "Content-Type",
    "If-None-Match", "Cache-Control", "Authorization",
},
```

#### Remove PostgREST Proxy Code

Delete:
- `postgrestURL` and `postgrestPublicURL` variable declarations (lines 67-74)
- The `/docs/postgrest.json` handler (lines 88-134)
- The multi-spec Swagger UI config — simplify to single spec

Replace the Swagger UI setup with:

```go
r.Get("/docs/go.json", func(w http.ResponseWriter, r *http.Request) {
    data, err := rewriteSwaggerServer([]byte(apidocs.SwaggerInfo.ReadDoc()), requestBaseURL(r), true)
    if err != nil {
        respond.WriteError(w, http.StatusBadGateway, "proxy_error", "failed to rewrite spec")
        return
    }
    w.Header().Set("Content-Type", "application/json")
    w.WriteHeader(http.StatusOK)
    w.Write(data)
})

r.Get("/docs/*", httpSwagger.Handler(
    httpSwagger.URL("/docs/go.json"),
))
```

The `rewriteSwaggerServer` function stays (it's still used to set the host/scheme from the request), but the `preserveBasePath` parameter and the PostgREST-specific error message in it can be cleaned up. The `io` and `fmt` imports related to the proxy can be removed if unused after cleanup.

#### Remove Unused Imports

After removing the PostgREST proxy code, these imports in `server.go` may become unused:
- `"io"` — only used by PostgREST proxy body read
- `"time"` — only used by PostgREST proxy cache TTL
- `"fmt"` — check if still used elsewhere in the file
- `"net/url"` — still used by `rewriteSwaggerServer`

### Step 4: Update — `go/internal/config/config.go`

Remove two fields and their env var loading:

```go
// Remove from Config struct:
PostgRESTURL       string
PostgRESTPublicURL string

// Remove from Load():
PostgRESTURL:       envOr("POSTGREST_URL", ""),
PostgRESTPublicURL: envOr("POSTGREST_PUBLIC_URL", ""),
```

### Step 5: Remove PostgREST Service

| Action | File | Reason |
|--------|------|--------|
| Delete | `postgrest/Dockerfile` | No longer needed |
| Delete | `postgrest/entrypoint.sh` | No longer needed |
| Update | `docker-compose.yml` | Remove `postgrest` service, remove `depends_on: postgrest` from `api`, remove `POSTGREST_URL`/`POSTGREST_PUBLIC_URL` env vars from `api` |

#### docker-compose.yml After

```yaml
services:
  api:
    build: go/
    ports:
      - "8000:8000"
    env_file: .env

  seed:
    build: seed/
    env_file: .env
    environment:
      NEON_DATABASE_URL_V2: ${NEON_DATABASE_URL_V2:-}
      DATABASE_URL: ${DATABASE_URL:-}
      BALLDONTLIE_API_KEY: ${BALLDONTLIE_API_KEY:-}
      SPORTMONKS_API_TOKEN: ${SPORTMONKS_API_TOKEN:-}
    profiles: ["seed"]
```

### Step 6: Update — `go/internal/api/server_test.go`

#### Remove

- `TestRouteOwnershipSplit` — the "profile moved to postgrest" / "stats moved to postgrest" / "autofill moved to postgrest" test cases are no longer valid. The whole test becomes obsolete since the split no longer exists.
- `TestRewriteSwaggerServerUsesPublicURL` — the PostgREST public URL rewrite test is no longer needed. (The `rewriteSwaggerServer` function is still used for Go spec rewriting, but the test for PostgREST-specific behavior can go.)

#### Add

- **Sport param validation test** — verify that `/api/v1/basketball/player?id=1` returns 400
- **Valid sport routing test** — verify that `/api/v1/nba/player`, `/api/v1/nfl/team`, `/api/v1/football/standings` all route correctly (will return 400/500 without DB, but should not 404)
- **Required param tests** — verify that `/api/v1/nba/player` without `id` returns 400
- **Football leagues route test** — verify `/api/v1/football/leagues` routes correctly
- **Keep** `TestGoSpecProxyUsesRequestHost` — still valid since Go spec rewriting remains

**Note:** These tests don't need a real database — they test routing and param validation using `httptest`. The handler will fail at the DB query step, but the test can verify it gets past routing (i.e., doesn't 404) and past param validation (i.e., doesn't 400 on valid params).

### Step 7: Swagger Annotations

Add swaggo annotations to each handler in `stats.go`. Example for `GetPlayer`:

```go
// GetPlayer returns a single player profile with stats.
// @Summary Get player profile
// @Description Returns player profile, team context, and season stats from the sport-specific schema view.
// @Tags stats
// @Produce json
// @Param sport path string true "Sport" Enums(nba, nfl, football)
// @Param id query int true "Player ID"
// @Param season query int false "Season year (e.g. 2025)"
// @Success 200 {object} map[string]interface{}
// @Failure 400 {object} respond.ErrorResponse
// @Failure 404 {object} respond.ErrorResponse
// @Router /api/v1/{sport}/player [get]
```

After implementing, regenerate the spec: `cd go && swag init -g cmd/api/main.go -o docs/`

### Step 8: Update — `.env.example`

Remove the PostgREST configuration section (lines 102-124):

```
# PostgREST Configuration
POSTGREST_URL=http://localhost:3000
POSTGREST_PUBLIC_URL=http://localhost:3000
```

Also update the CORS default origins comment — `http://localhost:3000` was listed because PostgREST ran there. If the frontend doesn't use port 3000, remove it from the defaults.

### Step 9: Update — `CLAUDE.md`

- Update architecture diagram to show single Go API service
- Update "Where New Endpoints Go" table — all endpoints go in Go now
- Remove all PostgREST references
- Update build/run commands (remove PostgREST from Docker section)
- Add note about `{sport}` path param pattern for new endpoints

### Step 10: Progress Doc

Create `progress_docs/2026-03-21_drop-postgrest-consolidate-go.md` with the standard format.

---

## File Change Summary

| Action | File | Description |
|--------|------|-------------|
| **Create** | `go/internal/api/handler/stats.go` | 7 handlers + sport validation helper |
| **Modify** | `go/internal/db/db.go` | Add ~19 prepared statements |
| **Modify** | `go/internal/api/server.go` | Add routes, remove PostgREST proxy, simplify Swagger |
| **Modify** | `go/internal/api/server_test.go` | Remove PostgREST tests, add sport routing tests |
| **Modify** | `go/internal/config/config.go` | Remove PostgREST fields |
| **Modify** | `go/internal/cache/cache.go` | Add `TTLStats` constant |
| **Modify** | `docker-compose.yml` | Remove postgrest service |
| **Modify** | `.env.example` | Remove PostgREST section |
| **Modify** | `CLAUDE.md` | Update architecture docs |
| **Delete** | `postgrest/Dockerfile` | No longer needed |
| **Delete** | `postgrest/entrypoint.sh` | No longer needed |
| **Create** | `progress_docs/2026-03-21_drop-postgrest-consolidate-go.md` | Session summary |

## What Stays Unchanged

- **SQL views** in `sql/nba.sql`, `sql/nfl.sql`, `sql/football.sql` — still the query layer
- **Triggers and derived stats** — all Postgres, untouched
- **Python seeder** — unchanged
- **News/Twitter handlers** — unchanged
- **Cache, rate limiting, middleware** — unchanged (except adding `TTLStats`)
- **Database roles** (`web_anon`, `web_user`) — still exist but PostgREST no longer uses them

---

## Open Questions

1. **Football standings parameters** — Football standings filter by `league_id`, not `conference`/`division`. Should the handler accept all three params and ignore sport-irrelevant ones? Or should football standings have a slightly different parameter validation?

2. **CORS default origins** — `http://localhost:3000` is in the default CORS origins because PostgREST ran there. Should it be removed, or does the frontend dev server also use port 3000?

3. **Autofill search** — The current `ILIKE '%' || $1 || '%'` pattern is simple but doesn't use the materialized view's potential for indexing. Should we add a `GIN` trigram index on `autofill_entities.name` for performance? (This would be a SQL change, out of scope for this plan.)

4. **Database roles cleanup** — After removing PostgREST, the `web_anon` and `web_user` roles are no longer needed for API access (the Go API connects as the pool user). Should they be dropped, or kept for potential future use?

5. **Chi routing precedence** — Need to verify that `/{sport}` doesn't shadow `/news` and `/twitter` routes. If it does, alternatives:
   - Move sport routes to `/api/v1/data/{sport}/...`
   - Register literal routes first (Chi's radix tree should handle this, but test it)

---

## Verification Plan

After implementation:

1. **Unit tests pass:** `cd go && go test ./...`
2. **Docker builds:** `docker compose up --build` — single API service starts
3. **Endpoint smoke tests:**
   ```bash
   # Player profile
   curl http://localhost:8000/api/v1/nba/player?id=1&season=2025

   # Standings
   curl http://localhost:8000/api/v1/nba/standings?season=2025

   # Stat leaders
   curl http://localhost:8000/api/v1/nba/stat-leaders?season=2025&stat=pts

   # Autofill
   curl http://localhost:8000/api/v1/nba/autofill?q=lebron

   # Football leagues
   curl http://localhost:8000/api/v1/football/leagues

   # Invalid sport
   curl http://localhost:8000/api/v1/basketball/player?id=1  # expect 400
   ```
4. **CORS works:** `curl -H "Origin: http://localhost:4321" -sI http://localhost:8000/api/v1/nba/standings?season=2025` — has `Access-Control-Allow-Origin`
5. **Swagger UI:** `http://localhost:8000/docs/` — single spec, all endpoints documented
6. **News/Twitter still work:** existing endpoints unaffected

---

## Rollback

If issues arise after deployment:
- The SQL views are unchanged, so re-deploying with PostgREST is trivial
- Git revert the commit
- Re-add PostgREST to docker-compose.yml
- No database migration to undo
