# Go API Migration Guide

Overview of migrating Scoracle's API layer from Python/FastAPI to Go, enabled by the Postgres-centric architecture established in migrations `003` through `006`.

## Architecture

```
Python (seeders/CLI)  --write-->  Postgres (views/functions)  <--read--  Go (API)
```

- **Python stays**: Seeders, CLI, external API clients (BallDontLie, SportMonks). These write to Postgres and don't need to change.
- **Postgres owns the data**: All data shaping, ranking, and derived computations live in SQL views and functions.
- **Go replaces FastAPI**: The API server connects to the same Postgres database and queries the same views/functions.

Neither Python nor Go depend on each other. Both talk to Postgres independently.

## Endpoint-to-SQL Mapping

Every API endpoint maps directly to a Postgres view or function. Go handlers execute these queries and serialize the result to JSON.

| Endpoint | HTTP | Postgres Object | Go Query |
|---|---|---|---|
| Player profile | `GET /profile/player/{id}?sport=` | `v_player_profile` | `SELECT * FROM v_player_profile WHERE id = $1 AND sport_id = $2` |
| Team profile | `GET /profile/team/{id}?sport=` | `v_team_profile` | `SELECT * FROM v_team_profile WHERE id = $1 AND sport_id = $2` |
| Entity stats | `GET /stats/{type}/{id}?sport=&season=` | `player_stats` / `team_stats` | Direct SELECT with percentile metadata extraction (see below) |
| Stat leaders | `GET /stats/leaders?sport=&stat=` | `fn_stat_leaders()` | `SELECT * FROM fn_stat_leaders($1, $2, $3, $4, $5, $6)` |
| Standings | `GET /standings?sport=&season=` | `fn_standings()` | `SELECT * FROM fn_standings($1, $2, $3, $4)` |
| Available seasons | `GET /stats/{type}/{id}/seasons` | `player_stats` / `team_stats` | `SELECT DISTINCT season FROM {table} WHERE ... ORDER BY season DESC` |
| Stat definitions | `GET /stats/definitions?sport=` | `stat_definitions` | `SELECT * FROM stat_definitions WHERE sport = $1 ORDER BY sort_order` |
| Provider season ID | (internal) | `resolve_provider_season_id()` | `SELECT resolve_provider_season_id($1, $2)` |

### Entity Stats Query (not a function — direct query)

```sql
SELECT
    stats,
    percentiles - '_position_group' - '_sample_size' AS percentiles,
    percentiles->>'_position_group' AS position_group,
    (percentiles->>'_sample_size')::int AS sample_size
FROM player_stats  -- or team_stats
WHERE player_id = $1 AND sport = $2 AND season = $3
```

## What Go Needs

### Driver
Use [`pgx`](https://github.com/jackc/pgx) with `pgxpool` for connection pooling. It handles JSONB natively via `json.RawMessage` or typed structs.

### Patterns

```go
// Profile — query view, scan into struct, serialize
row := pool.QueryRow(ctx,
    "SELECT * FROM v_player_profile WHERE id = $1 AND sport_id = $2",
    playerID, sport)

// Stat leaders — call function, iterate rows
rows, _ := pool.Query(ctx,
    "SELECT * FROM fn_stat_leaders($1, $2, $3, $4, $5, $6)",
    sport, season, statName, limit, position, leagueID)

// Standings — call function
rows, _ := pool.Query(ctx,
    "SELECT * FROM fn_standings($1, $2, $3, $4)",
    sport, season, leagueID, conference)
```

### JSONB Handling

Profile views return `team` and `league` as `json` columns. In Go:

```go
type PlayerProfile struct {
    ID       int              `json:"id"`
    Name     string           `json:"name"`
    // ...
    Team     json.RawMessage  `json:"team"`    // pre-built JSON from Postgres
    League   json.RawMessage  `json:"league"`  // pre-built JSON from Postgres
}
```

The `json.RawMessage` fields pass through Postgres-built JSON directly — no struct unpacking and re-marshaling needed.

For `stats` and `percentiles` (JSONB columns), same approach:

```go
type EntityStats struct {
    Stats       json.RawMessage `json:"stats"`
    Percentiles json.RawMessage `json:"percentiles"`
}
```

## What Go Must Reimplement

These are framework-specific concerns that don't live in Postgres:

| Concern | Python (current) | Go (reimplement) |
|---|---|---|
| HTTP routing | FastAPI `APIRouter` | `net/http`, chi, or gin |
| In-memory cache | Custom `TTLCache` | `groupcache`, `bigcache`, or built-in `sync.Map` |
| ETag generation | MD5 hash of JSON response | Same algorithm, `crypto/md5` |
| Cache headers | `Cache-Control`, `X-Cache` | Set response headers manually |
| Rate limiting | Not currently implemented | `golang.org/x/time/rate` |
| Error responses | Custom `NotFoundError`, `ValidationError` | HTTP status codes + JSON error body |
| Season validation | `validate_season()` in `_utils.py` | Simple year-range check |

## Additional Postgres Objects (migrations 004-006)

| Object | Purpose | Go Usage |
|---|---|---|
| `stat_definitions` table | Canonical stat name registry | Query for display names, categories, inverse flags |
| `recalculate_percentiles()` | Self-contained percentile computation (reads inverse stats from `stat_definitions`) | Call from Go CLI/cron, no config needed |
| `resolve_provider_season_id()` | Maps (league_id, season_year) -> provider season ID | Internal use if Go handles fixture management |
| `provider_seasons` table | Season year -> provider season ID mappings | Reference data for scheduling |
| Derived stats triggers | NBA per-36/TS%/efficiency, NFL td_int_ratio/catch_pct, Football per-90/accuracy | Transparent — fires on INSERT/UPDATE, Go just reads the computed values |

## What Stays in Python

These components are not part of the API server and don't move to Go:

- **Seeders** (`seeders/seed_nba.py`, `seed_nfl.py`, `seed_football.py`): Parse external API responses, normalize data, write to Postgres
- **CLI** (`cli.py`): Database init, seeding orchestration, fixture management, exports
- **External API clients** (`providers/`): BallDontLie, SportMonks HTTP clients
- **Percentile calculator** (`percentiles/`): Orchestrates the `recalculate_percentiles()` SQL function
- **Fixture scheduler** (`fixtures/`): Manages fixture lifecycle and post-match seeding

## Migration Checklist

1. Set up Go project with `pgxpool` connection to the same Postgres database
2. Implement handlers for each endpoint using the SQL mapping above
3. Add cache layer (ETag + in-memory)
4. Add error handling (404 for missing entities, 400 for bad params)
5. Test against the same database the Python API currently uses
6. Swap the API deployment target from Python to Go
7. Python seeders/CLI continue running unchanged against the same database
