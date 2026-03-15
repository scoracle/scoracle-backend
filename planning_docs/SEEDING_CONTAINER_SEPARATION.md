# Seeding Container Separation Plan

**Date:** 2026-03-15
**Status:** Proposed

## Problem

The Go service currently handles two distinct responsibilities:

1. **Third-party integrations + event listening** — News (Google RSS + NewsAPI), curated Twitter/X feeds, LISTEN/NOTIFY for push notifications, maintenance tickers
2. **Data seeding/ingestion** — CLI commands to seed NBA/NFL/Football data from external provider APIs (BallDontLie, SportMonks) into Postgres, fixture processing, percentile recalculation

This conflation causes:

1. **Role confusion** — It's unclear which parts of Go are "the API" vs "the seeder." New contributors (human or AI agent) get bogged down reading seed orchestration when they only need to touch news endpoints, and vice versa.
2. **Bloated context** — AI models working on Go carry the full weight of provider clients, canonical structs, upsert logic, fixture scheduling, and seed orchestration alongside the API handler code they actually need.
3. **Deployment coupling** — Seeding is a batch/scheduled job that runs infrequently. The API is a long-running server. Packaging both in one binary means the API binary ships provider clients and seed logic it never uses.
4. **Unnatural fit** — Go is ideal for the concurrent API server and LISTEN/NOTIFY listener. It is unnecessarily verbose for what seeding actually does: HTTP calls → JSON normalization → parameterized SQL inserts.

## Architecture

**Three containers, one database. Each container has exactly one job.**

```
                    Provider APIs
                   (BDL, SportMonks)
                         │
                         ▼
              ┌─────────────────────┐
              │   Python Seeder     │  ← NEW: seeds data on schedule + CLI
              │   (seed/)           │
              └────────┬────────────┘
                       │ INSERT/UPSERT
                       ▼
              ┌─────────────────────┐
              │  Neon PostgreSQL    │  ← Computes derived stats, percentiles,
              │                     │    fires LISTEN/NOTIFY events
              └──┬──────────────┬───┘
                 │              │
                 ▼              ▼
    ┌────────────────┐  ┌──────────────────┐
    │   PostgREST    │  │     Go API       │  ← SLIMMED: news, twitter,
    │   (:3000)      │  │     (:8000)      │    LISTEN/NOTIFY, notifications
    │   Stats API    │  │   Integrations   │
    └────────────────┘  └──────────────────┘
                 │              │
                 └──────┬───────┘
                        ▼
                    Frontend (Astro)
```

### Data Flow

```
Python seeds data → Postgres triggers compute derived stats →
  Postgres recalculate_percentiles() runs →
    Postgres emits NOTIFY on milestone channels →
      Go listener picks up events → sends push notifications
```

### Responsibility Boundaries

| Concern | Owner |
|---------|-------|
| Fetching data from BDL/SportMonks | Python seeder |
| Normalizing API responses to canonical format | Python seeder |
| Upserting teams, players, stats into Postgres | Python seeder |
| Calling recalculate_percentiles() | Python seeder |
| Processing pending fixtures (re-seeding after games) | Python seeder |
| Scheduling seed jobs (cron-style) | Python seeder |
| Computing derived stats (per-36, TS%, win_pct) | Postgres triggers |
| Computing percentile rankings | Postgres function |
| Emitting milestone events | Postgres NOTIFY |
| Listening for DB events | Go API |
| Sending push notifications (FCM) | Go API |
| Serving news + twitter feeds | Go API |
| Maintenance tickers (cleanup, digest, catch-up) | Go API |
| Serving stats, profiles, standings, search | PostgREST |

## Why Python

| Consideration | Python | Go (status quo) | Zig |
|--------------|--------|-----------------|-----|
| HTTP + JSON ergonomics | Excellent (httpx, native dicts) | Verbose (struct tags, manual parsing) | Immature ecosystem |
| SQL parameterization | Clean (psycopg, `%s` params) | Clean (pgx) | No mature Postgres driver |
| Scheduler libraries | APScheduler, mature | Limited options | None |
| Container size | ~150MB (slim) | ~8MB (static) | ~5MB |
| Developer experience | Author has experience | Already in use | Learning curve |
| Maintenance burden | Low — seeding is simple code | Overkill for the task | High — low-level plumbing |

Python wins on ergonomics for what is essentially "call API, transform JSON, run SQL." The container size difference (150MB vs 8MB) is irrelevant for a batch job that runs a few times per day.

## Python Tech Stack

| Purpose | Library | Rationale |
|---------|---------|-----------|
| HTTP client | `httpx` | Async-capable, timeout/retry, rate limiting via custom transport |
| DB driver | `psycopg[binary]` v3 | Modern async Postgres, auto-prepares queries, no ORM |
| Scheduler | `APScheduler` v4 | In-process cron triggers, no Redis/Celery dependency |
| CLI | `click` | Composable commands, similar to Cobra |
| Config | `pydantic-settings` | Typed env var loading with validation |
| Schedule configs | `PyYAML` | Declarative per-sport schedule files |
| Logging | `structlog` | Structured JSON output, matches Go's slog style |

## Directory Structure

```
seed/
├── pyproject.toml              # Dependencies, entry points, metadata
├── Dockerfile                  # python:3.13-slim based
├── railway.toml                # Railway deployment config
├── schedules/                  # Per-sport schedule configs (YAML)
│   ├── nba.yaml
│   ├── nfl.yaml
│   └── football.yaml
├── src/
│   └── scoracle_seed/
│       ├── __init__.py
│       ├── cli.py              # Click CLI: seed, percentiles, fixtures, run
│       ├── config.py           # pydantic-settings, DB URL resolution chain
│       ├── db.py               # psycopg connection pool, raw parameterized queries
│       ├── scheduler.py        # APScheduler setup, loads YAML configs
│       ├── models.py           # Canonical dataclasses (Team, Player, PlayerStats, etc.)
│       ├── upsert.py           # All INSERT ON CONFLICT DO UPDATE functions
│       ├── percentiles.py      # recalculate_percentiles() + archive calls
│       ├── fixtures.py         # Pending fixture query, grouping, processing
│       ├── providers/
│       │   ├── __init__.py
│       │   ├── bdl/
│       │   │   ├── __init__.py
│       │   │   ├── client.py   # Rate-limited httpx (600 req/min, cursor pagination)
│       │   │   ├── nba.py      # NBA handler: teams, player stats, team stats
│       │   │   └── nfl.py      # NFL handler: teams, player stats, standings
│       │   └── sportmonks/
│       │       ├── __init__.py
│       │       ├── client.py   # Rate-limited httpx (300 req/min, page pagination, 429 backoff)
│       │       └── football.py # Football: season discovery, squads, players, standings
│       └── seeds/
│           ├── __init__.py
│           ├── nba.py          # Three-phase NBA seed orchestration
│           ├── nfl.py          # Three-phase NFL seed orchestration
│           └── football.py     # Three-phase Football seed orchestration (per-league)
└── tests/
    ├── test_models.py
    └── test_providers/
        ├── test_bdl_normalize.py
        └── test_sportmonks_normalize.py
```

## CLI Commands

```bash
# Manual seeding (ad-hoc)
scoracle-seed seed nba --season 2025
scoracle-seed seed nfl --season 2025
scoracle-seed seed football --season 2025 --league 8

# Percentile recalculation
scoracle-seed percentiles --sport NBA --season 2025

# Fixture processing
scoracle-seed fixtures process --sport NBA --max 50
scoracle-seed fixtures seed --id 42

# Scheduler daemon (container default entrypoint)
scoracle-seed run
```

## Schedule Config Format

Each sport gets a declarative YAML file. The scheduler loads all files at startup, checks current date against `season_dates`, and registers the appropriate cron triggers.

```yaml
# schedules/nba.yaml
sport: NBA
current_season: 2025
provider: bdl

in_season:
  full_seed:
    cron: "0 6 * * *"           # Daily at 6am UTC
  fixtures:
    cron: "*/30 * * * *"        # Every 30 minutes
    max_fixtures: 50
  percentiles:
    cron: "30 6 * * *"          # Daily at 6:30am UTC (after full seed)

off_season:
  full_seed:
    cron: "0 6 * * 1"           # Weekly on Monday

season_dates:
  start: "2024-10-22"
  end: "2025-06-22"
```

```yaml
# schedules/football.yaml
sport: FOOTBALL
current_season: 2025
provider: sportmonks

leagues:
  - id: 8
    name: Premier League
    sm_league_id: 8
  - id: 82
    name: Bundesliga
    sm_league_id: 82
  # ... more leagues

in_season:
  full_seed:
    cron: "0 5 * * *"
  fixtures:
    cron: "*/30 * * * *"
    max_fixtures: 50
  percentiles:
    cron: "30 5 * * *"

off_season:
  full_seed:
    cron: "0 5 * * 1"

season_dates:
  start: "2024-08-15"
  end: "2025-05-25"
```

The `season_dates` boundaries determine which schedule block (in_season vs off_season) is active. This gets "loaded once a year" — update the YAML when a new season starts.

## Key Module Designs

### Canonical Models (`models.py`)

Direct translation of Go's `provider/canonical.go` to Python dataclasses:

```python
@dataclass
class Team:
    id: int
    name: str
    short_code: str | None = None
    city: str | None = None
    country: str | None = None
    conference: str | None = None
    division: str | None = None
    logo_url: str | None = None
    venue_name: str | None = None
    venue_capacity: int | None = None
    founded: int | None = None
    meta: dict[str, Any] = field(default_factory=dict)

@dataclass
class Player:
    id: int
    name: str
    first_name: str | None = None
    last_name: str | None = None
    position: str | None = None
    nationality: str | None = None
    height: str | None = None
    weight: str | None = None
    date_of_birth: str | None = None
    photo_url: str | None = None
    team_id: int | None = None
    meta: dict[str, Any] = field(default_factory=dict)

@dataclass
class PlayerStats:
    player_id: int
    team_id: int | None = None
    player: Player | None = None
    stats: dict[str, float]
    raw: dict[str, Any] | None = None

@dataclass
class TeamStats:
    team_id: int
    team: Team | None = None
    stats: dict[str, float]
    raw: dict[str, Any] | None = None

@dataclass
class SeedResult:
    teams_upserted: int = 0
    players_upserted: int = 0
    player_stats_upserted: int = 0
    team_stats_upserted: int = 0
    errors: list[str] = field(default_factory=list)
```

### Config (`config.py`)

Matches Go's env var resolution chain:

```python
class Settings(BaseSettings):
    neon_database_url_v2: str = ""
    database_url: str = ""
    neon_database_url: str = ""
    balldontlie_api_key: str = ""
    sportmonks_api_token: str = ""
    db_pool_min_conns: int = 2
    db_pool_max_conns: int = 10

    @property
    def resolved_db_url(self) -> str:
        return self.neon_database_url_v2 or self.database_url or self.neon_database_url
```

### Upsert Functions (`upsert.py`)

Direct port of SQL from `go/internal/seed/upsert.go`. All use parameterized queries with `INSERT ... ON CONFLICT DO UPDATE`:

- `upsert_team(conn, sport, team)` — conflict on `(id, sport)`
- `upsert_player(conn, sport, player)` — conflict on `(id, sport)`, COALESCE to preserve existing
- `upsert_player_stats(conn, sport, season, league_id, stats)` — conflict on `(player_id, sport, season, league_id)`
- `upsert_team_stats(conn, sport, season, league_id, stats)` — conflict on `(team_id, sport, season, league_id)`
- `recalculate_percentiles(conn, sport, season)` — calls Postgres function
- `archive_percentiles(conn, sport, season)` — calls Postgres function

### Provider Clients

**BDL (`providers/bdl/client.py`):**
- Base URLs: `https://api.balldontlie.io/v1` (NBA), `https://api.balldontlie.io/nfl/v1` (NFL)
- Auth: `Authorization` header with API key
- Rate limit: 600 req/min (token bucket via asyncio semaphore)
- Pagination: cursor-based (`meta.next_cursor`)

**SportMonks (`providers/sportmonks/client.py`):**
- Base URL: `https://api.sportmonks.com/v3/football`
- Auth: `api_token` query parameter
- Rate limit: 300 req/min
- Pagination: page-based (`pagination.has_more`)
- 429 retry: exponential backoff (2s, 4s, 8s, 16s, 32s), max 5 retries

### Seed Orchestration (per sport)

Each sport follows the same three-phase pattern, ported from Go:

1. **Phase 1: Teams** — Fetch all teams from provider → `upsert_team()` each
2. **Phase 2: Players + Player Stats** — Fetch player stats (includes player profiles) → `upsert_player()` + `upsert_player_stats()` each. Postgres triggers auto-compute derived stats.
3. **Phase 3: Team Stats** — Fetch team stats/standings → `upsert_team_stats()` each

Progress logged every 50 records. Final summary with counts and duration.

## Notification Handoff

**Problem:** Currently Go's `SeedFixture()` calls `notifications.Run()` directly after seeding. With seeding in Python, Go won't know when seeding completes.

**Solution: Postgres NOTIFY bridge.** Modify the `recalculate_percentiles()` Postgres function to emit:
```sql
PERFORM pg_notify('percentile_recalculated', json_build_object(
    'sport', sport_param,
    'season', season_param
)::text);
```

Go's existing LISTEN/NOTIFY listener (in `go/internal/listener/`) adds a handler for the `percentile_recalculated` channel that triggers the notification pipeline. This:
- Matches the project's existing event-driven pattern
- Requires no HTTP endpoints between containers
- Is invisible to the Python seeder (it just calls `recalculate_percentiles()` as before)

The existing catch-up sweep in `go/internal/maintenance/` already handles missed events, so the notification pipeline is resilient to listener downtime.

## Docker Integration

### Dockerfile (`seed/Dockerfile`)

```dockerfile
FROM python:3.13-slim AS base

WORKDIR /app
COPY pyproject.toml ./
RUN pip install --no-cache-dir .

COPY src/ src/
COPY schedules/ schedules/
RUN pip install --no-cache-dir -e .

RUN adduser --disabled-password --uid 1000 scoracle
USER scoracle

ENTRYPOINT ["scoracle-seed"]
CMD ["run"]
```

Uses `python:3.13-slim` (not alpine) because psycopg binary wheels require glibc. Target image ~150MB.

### docker-compose.yml Addition

```yaml
seed:
  build: seed/
  env_file: .env
  environment:
    NEON_DATABASE_URL_V2: ${NEON_DATABASE_URL_V2:-}
    DATABASE_URL: ${DATABASE_URL:-}
    NEON_DATABASE_URL: ${NEON_DATABASE_URL:-}
    BALLDONTLIE_API_KEY: ${BALLDONTLIE_API_KEY:-}
    SPORTMONKS_API_TOKEN: ${SPORTMONKS_API_TOKEN:-}
  volumes:
    - ./seed/schedules:/app/schedules:ro
```

Ad-hoc CLI usage: `docker compose run --rm seed seed nba --season 2025`

### Railway Deployment (`seed/railway.toml`)

```toml
[build]
builder = "DOCKERFILE"
watchPatterns = ["src/**", "schedules/**", "pyproject.toml", "Dockerfile", "railway.toml"]

[deploy]
restartPolicyType = "ON_FAILURE"
restartPolicyMaxRetries = 10
```

No health check needed — it's a scheduler, not a server.

## What Changes in Go

### Delete Entirely

| Directory | Contents | Why |
|-----------|----------|-----|
| `go/cmd/ingest/` | Cobra CLI entry point | Seeding CLI moves to Python |
| `go/internal/seed/` | nba.go, nfl.go, football.go, upsert.go, result.go | Seed orchestration + upserts move to Python |
| `go/internal/provider/` | canonical.go, extract.go, bdl/, sportmonks/ | Provider clients + normalization move to Python |
| `go/internal/fixture/` | fixture.go, query.go, scheduler.go, seed.go | Fixture processing moves to Python |

### Modify

| File | Change |
|------|--------|
| `go/internal/db/db.go` | Remove seeding-only prepared statements: `recalculate_percentiles`, `resolve_provider_season`, `league_lookup`, `get_pending_fixtures`, `fixture_by_id`, `archive_current_percentiles`, `check_player_stats_season`, `check_team_stats_season`. Keep notification statements. |
| `go/internal/config/config.go` | Remove `SportRegistry` and table name constants no longer referenced by Go code. Keep API-related config. |
| `go/internal/listener/listener.go` | Add handler for `percentile_recalculated` NOTIFY channel to trigger notification pipeline. |
| `go/cmd/api/main.go` | Remove any imports of `fixture`, `seed`, or `provider` packages. |

### Keep As-Is

- `go/internal/notifications/` — triggered by LISTEN/NOTIFY
- `go/internal/maintenance/` — cleanup, digest, catch-up sweep
- `go/internal/listener/` — milestone detection (+ new percentile channel)
- `go/internal/api/handler/` — news.go, twitter.go
- `go/internal/thirdparty/` — news + twitter HTTP clients
- `go/internal/cache/` — API response caching

## Implementation Phases

### Phase 1: Python scaffold
Create `seed/` directory, `pyproject.toml`, `config.py`, `models.py`, `db.py`. Verify DB connectivity with a simple query.

### Phase 2: Provider clients
Port BDL client + NBA/NFL handlers, then SportMonks client + Football handler. Replicate rate limiting, pagination, stat key normalization maps. Write normalization tests.

### Phase 3: Upsert + seed orchestration
Port all upsert functions with parameterized SQL (directly from `go/internal/seed/upsert.go`). Port the three-phase seed orchestrators per sport.

### Phase 4: CLI
Wire Click commands matching the current `go/cmd/ingest` interface. Test manual seeding against the real database.

### Phase 5: Fixtures + percentiles
Port fixture processing: pending query, grouping by (sport, season, league_id), per-sport re-seeding, mark as seeded. Port percentile recalculation and archiving calls.

### Phase 6: Scheduler + YAML configs
Implement APScheduler loading per-sport YAML configs. Create initial schedule files for NBA, NFL, Football.

### Phase 7: Docker + compose
Build Dockerfile, add seed service to docker-compose.yml, add railway.toml.

### Phase 8: Notification bridge
Modify `recalculate_percentiles()` Postgres function to emit `NOTIFY percentile_recalculated`. Add handler in Go's listener. Test end-to-end: seed → percentiles → NOTIFY → Go → push notification.

### Phase 9: Go cleanup
Delete seeding code from Go. Update db.go prepared statements. Remove unused config. Verify `go build ./cmd/api` and `go test ./...` pass cleanly.

## Known Challenges

1. **SportMonks N+1 player fetch** — The current Go code fetches each player individually after getting the squad list. This is inherently slow (hundreds of requests per team). Port faithfully first, optimize later.

2. **Fixture notification handoff** — The transition from direct `notifications.Run()` calls to NOTIFY-based triggering requires careful testing to avoid missed notifications during the cutover. The catch-up sweep provides a safety net.

3. **Rate limiter fidelity** — Go uses `golang.org/x/time/rate` (token bucket). Python equivalent: asyncio semaphore with time-windowed release. Must match provider rate limits exactly to avoid 429s.

4. **psycopg auto-preparation** — Unlike Go's explicit prepared statement registration, psycopg v3 auto-prepares on the third execution. First two runs of each query are slightly slower — irrelevant for batch jobs.

## Go Source Files to Port From

| Go Source | Python Target | What to Port |
|-----------|--------------|--------------|
| `go/internal/seed/upsert.go` | `upsert.py` | All INSERT ON CONFLICT SQL |
| `go/internal/provider/bdl/client.go` | `providers/bdl/client.py` | Rate limiting, cursor pagination, auth |
| `go/internal/provider/bdl/nba.go` | `providers/bdl/nba.py` | Response parsing, stat key normalization |
| `go/internal/provider/bdl/nfl.go` | `providers/bdl/nfl.py` | Flat stat extraction, code overrides |
| `go/internal/provider/sportmonks/client.go` | `providers/sportmonks/client.py` | Rate limiting, page pagination, 429 backoff |
| `go/internal/provider/sportmonks/football.go` | `providers/sportmonks/football.py` | Season discovery, squad iteration, stat extraction |
| `go/internal/provider/canonical.go` | `models.py` | Canonical dataclasses |
| `go/internal/provider/extract.go` | `providers/sportmonks/football.py` | `extract_value()` helper |
| `go/internal/fixture/scheduler.go` | `fixtures.py` | Pending fixture grouping + processing |
| `go/internal/fixture/seed.go` | `fixtures.py` | Per-sport fixture seeding dispatch |
| `go/internal/fixture/query.go` | `fixtures.py` | DB queries for fixture state management |
| `go/internal/config/config.go` | `config.py` | Env var resolution, sport registry |
