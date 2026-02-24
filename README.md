# Scoracle Data

Backend data pipeline and API for Scoracle. Seeds sports statistics from external APIs into Neon PostgreSQL, computes derived stats via Postgres triggers, calculates percentile rankings, and serves everything through a high-performance Go API.

**Database:** PostgreSQL (Neon) only.

## Sports Covered

- **NBA** — Basketball statistics with per-36 minute normalization and True Shooting %
- **NFL** — American football with position-specific stat groupings
- **Football (Soccer)** — Top 5 European leagues with per-90 minute normalization

## Architecture Overview

The system is split into three independent layers. Each layer can be swapped out without affecting the others.

```
┌─────────────────────────────────────────────────────────────────┐
│                        Frontend (Astro)                         │
└──────────────────────────────┬──────────────────────────────────┘
                               │ JSON over HTTP
┌──────────────────────────────▼──────────────────────────────────┐
│                    Layer 3: Go API (Chi)                        │
│   Pure transport — Postgres functions return complete JSON.     │
│   No struct scanning, no marshaling. Raw bytes to HTTP.         │
│   In-memory cache, ETag, gzip, rate limiting, Swagger UI.      │
└──────────────────────────────┬──────────────────────────────────┘
                               │ pgxpool + prepared statements
┌──────────────────────────────▼──────────────────────────────────┐
│                 Layer 2: PostgreSQL (Neon)                      │
│   All data logic lives here: derived stat triggers,            │
│   recalculate_percentiles(), views (v_player_profile,          │
│   v_team_profile), functions (fn_stat_leaders, fn_standings),  │
│   API functions (api_player_profile, api_entity_stats, etc.)   │
└──────────────────────────────▲──────────────────────────────────┘
                               │ pgxpool + prepared statements
┌──────────────────────────────┴──────────────────────────────────┐
│                 Layer 1: Go Ingestion CLI (Cobra)               │
│   Provider-specific handlers fetch JSON from external APIs,     │
│   normalize to canonical Go structs, upsert to Postgres.        │
│   Provider-agnosticism via canonical output types.              │
└─────────────────────────────────────────────────────────────────┘
```

### Key Design Decisions

- **Postgres-as-serializer** — SQL functions (`api_player_profile`, `api_entity_stats`, etc.) return complete JSON responses. Go is a pure transport layer.
- **No service layer in the API** — handlers call pgxpool directly because Postgres functions ARE the service layer.
- **No shared Provider interface** — provider-agnosticism comes from canonical output structs, not input interfaces. Adding a new provider means adding a new handler package; nothing else changes.
- **Separate seed runner per sport** — NBA, NFL, and Football each have their own orchestration file since their data flows differ.

## Data Sources

| Sport | Provider | Go Handler |
|-------|----------|------------|
| NBA | [BallDontLie](https://balldontlie.io) | `internal/provider/bdl/nba.go` — `NBAHandler` |
| NFL | [BallDontLie](https://balldontlie.io) | `internal/provider/bdl/nfl.go` — `NFLHandler` |
| Football | [SportMonks](https://sportmonks.com) | `internal/provider/sportmonks/football.go` — `FootballHandler` |

## How Seeding Works

The pipeline follows a **handler + seed runner** pattern:

1. **Provider handlers** (`internal/provider/`) — Fetch data from external APIs and normalize responses into canonical Go structs (`Team`, `Player`, `PlayerStats`, `TeamStats`). Each provider package has its own HTTP client with rate limiting and pagination.

2. **Seed runners** (`internal/seed/`) — Provider-agnostic orchestration that takes canonical structs and upserts them into Postgres. Each sport has its own runner:
   - `SeedNBA` — teams, player stats (with auto-player upsert), team stats
   - `SeedNFL` — same flow, NFL-specific fields
   - `SeedFootballSeason` — per-league/team squad iteration via SportMonks

3. **Derived stats** — Postgres triggers automatically compute per-36, per-90, TS%, win_pct, and other derived metrics on INSERT/UPDATE to `player_stats` and `team_stats`.

4. **Percentiles** — The `recalculate_percentiles()` Postgres function computes per-position percentile rankings.

Player profiles are derived from stats responses (BallDontLie embeds full player data in each stats record), so there is no separate player-fetching step.

## Database Schema

Single consolidated schema in `schema.sql` at the repo root (v7.0). No incremental migrations — the schema file is the complete database definition including API functions and materialized views.

### Core Tables (11)

| Table | Purpose |
|-------|---------|
| `meta` | Key-value store for schema version and metadata |
| `sports` | Sport definitions (NBA, NFL, FOOTBALL) |
| `leagues` | League definitions with SportMonks IDs |
| `players` | Player profiles (all sports, unified) |
| `teams` | Team profiles (all sports, unified) |
| `player_stats` | Player statistics with JSONB `stats` column |
| `team_stats` | Team statistics with JSONB `stats` column |
| `stat_definitions` | Stat registry (display names, categories, inverse flags) |
| `provider_seasons` | Maps provider season IDs to year strings |
| `fixtures` | Match schedule for post-match seeding |
| `percentile_archive` | Stored percentile rankings by position group |

### Views

- `v_player_profile` — Joins players with their latest stats
- `v_team_profile` — Joins teams with their latest stats

### API Functions (in `schema.sql`)

| Function | Returns | Description |
|----------|---------|-------------|
| `api_player_profile(id, sport)` | JSON | Complete player profile from `v_player_profile` |
| `api_team_profile(id, sport)` | JSON | Complete team profile from `v_team_profile` |
| `api_entity_stats(type, id, sport, season, league_id)` | JSON | Stats + percentiles for a player or team |
| `api_available_seasons(type, id, sport)` | JSON | List of seasons with stats for an entity |

### Materialized View

- `mv_autofill_entities` — Pre-computed entity list (players + teams, all sports) for frontend search/autocomplete. Unique index enables `REFRESH MATERIALIZED VIEW CONCURRENTLY`.

### Triggers & Functions

- **Derived stats triggers** on `player_stats` and `team_stats` — auto-compute per-36, per-90, TS%, win_pct, etc.
- `resolve_provider_season_id()` — Maps provider season IDs
- `fn_stat_leaders()` / `fn_standings()` — Query helpers
- `recalculate_percentiles()` — Percentile recalculation entry point

## API

Go Chi server with Postgres JSON passthrough. All data-heavy responses are raw JSON bytes from Postgres functions — no struct scanning, no marshaling overhead.

### Features

- **pgxpool** connection pooling with 17 prepared statements
- **In-memory TTL cache** with ETag support and periodic eviction
- **Gzip compression** via Chi middleware
- **IP-based rate limiting** with token bucket (golang.org/x/time/rate)
- **CORS** with configurable origins
- **Swagger UI** at `/docs/` (swaggo auto-generated)
- **Graceful shutdown** on SIGINT

### Service Responsibilities

- **Go API**: external integrations (`/api/v1/news/*`, `/api/v1/twitter/*`) plus service health/docs.
- **PostgREST API**: DB-backed stats/profile/autofill endpoints generated from Postgres views/functions.
- **Swagger UI**: `/docs/` serves a multi-spec dropdown so both APIs are browsable in one place.

### Go Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /` | API info (name, version, status, optimizations) |
| `GET /health` | Basic health check |
| `GET /health/db` | Database connectivity |
| `GET /health/cache` | Cache statistics |
| `GET /docs/` | Swagger UI (interactive) |
| `GET /api/v1/news/{type}/{id}?sport=&source=` | News articles (Google News RSS + NewsAPI) |
| `GET /api/v1/news/status` | News service configuration status |
| `GET /api/v1/twitter/journalist-feed?q=&sport=` | Curated journalist tweets from X List |
| `GET /api/v1/twitter/status` | Twitter service configuration status |

### Cache TTLs

| Data Type | TTL |
|-----------|-----|
| News articles | 10 minutes |
| Twitter journalist feed | 1 hour (in-memory, separate from main cache) |

## CLI Commands

### Go CLI (production)

```bash
# Build both binaries
go build -o bin/scoracle-api ./cmd/api
go build -o bin/scoracle-ingest ./cmd/ingest

# Data seeding
./bin/scoracle-ingest seed nba
./bin/scoracle-ingest seed nfl
./bin/scoracle-ingest seed football

# Percentile recalculation
./bin/scoracle-ingest percentiles

# Run API server
./bin/scoracle-api
```

### Python CLI (legacy)

```bash
pip install -e .
scoracle-data seed --sport nba
scoracle-data percentiles --sport nba
uvicorn scoracle_data.api.main:app --reload
```

## Codebase Structure

```
scoracle-data/
├── schema.sql                         # THE complete database definition (v7.0)
├── go.mod / go.sum                    # Module: github.com/albapepper/scoracle-data
├── pyproject.toml                     # Python package config (legacy)
├── railway.toml                       # Railway deployment (Railpack, no Docker)
│
├── cmd/                               # Go entry points
│   ├── api/main.go                    # API server entry point (graceful shutdown)
│   └── ingest/main.go                 # Cobra CLI: seed nba|nfl|football, percentiles
│
├── docs/                              # Auto-generated Swagger (swaggo)
│   ├── docs.go
│   ├── swagger.json
│   └── swagger.yaml
│
├── internal/                          # Go internal packages
│   ├── api/
│   │   ├── server.go                  # Chi router, middleware stack, route registration
│   │   ├── middleware.go              # TimingMiddleware, RateLimitMiddleware
│   │   ├── respond/
│   │   │   └── respond.go            # WriteJSON, WriteError, WriteJSONObject helpers
│   │   └── handler/
│   │       ├── handler.go            # Handler struct (pool, cache, config, news, twitter)
│   │       ├── profile.go            # GET /profile — Postgres JSON passthrough
│   │       ├── stats.go              # GET /stats, /seasons, /definitions
│   │       ├── bootstrap.go          # GET /autofill_databases — materialized view
│   │       ├── news.go               # GET /news — entity lookup + RSS/NewsAPI
│   │       └── twitter.go            # GET /twitter — cached journalist feed
│   │
│   ├── cache/
│   │   └── cache.go                   # TTL cache, ETag (MD5), eviction loop
│   │
│   ├── config/
│   │   └── config.go                  # Env var loading, SportRegistry, table constants
│   │
│   ├── db/
│   │   └── db.go                      # pgxpool wrapper, 17 prepared statements
│   │
│   ├── thirdparty/
│   │   ├── news.go                    # Google News RSS + NewsAPI unified client
│   │   └── twitter.go                 # X API v2 List tweets with 1h feed cache
│   │
│   ├── provider/
│   │   ├── canonical.go               # Team, Player, PlayerStats, TeamStats structs
│   │   ├── extract.go                 # ExtractValue() for normalizing API fields
│   │   ├── bdl/
│   │   │   ├── client.go             # Shared BDL HTTP client (cursor pagination)
│   │   │   ├── nba.go                # NBAHandler: teams, players, stats
│   │   │   └── nfl.go                # NFLHandler: teams, players, stats
│   │   └── sportmonks/
│   │       ├── client.go             # SportMonks HTTP client (page pagination)
│   │       └── football.go           # FootballHandler: squads, standings
│   │
│   └── seed/
│       ├── result.go                  # SeedResult tracking (counts + errors)
│       ├── upsert.go                  # UpsertTeam/Player/Stats, RecalculatePercentiles
│       ├── nba.go                     # SeedNBA orchestration
│       ├── nfl.go                     # SeedNFL orchestration
│       └── football.go               # SeedFootballSeason orchestration
│
├── python/                            # Python codebase (legacy, being phased out)
│   ├── scoracle_data/                 # Python package
│   │   ├── cli.py                     # Full-featured CLI (seed, percentiles, export, fixtures)
│   │   ├── api/                       # FastAPI server + routers
│   │   ├── core/                      # Config, types, models
│   │   ├── handlers/                  # BDL, SportMonks API clients
│   │   ├── seeders/                   # Seed orchestration
│   │   ├── external/                  # News, Twitter clients
│   │   ├── fixtures/                  # Fixture scheduling
│   │   ├── services/                  # Business logic
│   │   └── percentiles/               # Percentile calculation
│   └── tests/                         # Python test suite
│
└── planning_docs/                     # Planning & design documents
```

## Python Codebase (legacy, being phased out)

The Python codebase under `python/scoracle_data/` contains the original FastAPI server, CLI, and data seeders. It is no longer the production system but remains in the repo until the Go migration is fully validated.

## Go Dependencies

| Package | Version | Purpose |
|---------|---------|---------|
| `go-chi/chi/v5` | v5.2.5 | HTTP router |
| `jackc/pgx/v5` | v5.8.0 | PostgreSQL driver + connection pool |
| `joho/godotenv` | v1.5.1 | `.env` file loading |
| `rs/cors` | v1.11.1 | CORS middleware |
| `spf13/cobra` | v1.10.2 | CLI framework |
| `swaggo/http-swagger/v2` | v2.0.2 | Swagger UI serving |
| `golang.org/x/time` | v0.14.0 | Token-bucket rate limiter |

## Environment Variables

**Required:**

| Variable | Description |
|----------|-------------|
| `NEON_DATABASE_URL_V2` | PostgreSQL connection string (Neon). Fallback: `DATABASE_URL` |
| `BALLDONTLIE_API_KEY` | BallDontLie API key (NBA + NFL ingestion) |
| `SPORTMONKS_API_TOKEN` | SportMonks API token (Football ingestion) |

**Optional:**

| Variable | Default | Description |
|----------|---------|-------------|
| `API_HOST` | `0.0.0.0` | API bind address |
| `API_PORT` | `8000` | API port |
| `ENVIRONMENT` | `development` | `development`, `staging`, `production` |
| `DEBUG` | `false` | Enable debug logging |
| `CORS_ALLOW_ORIGINS` | `localhost:3000,4321,5173` | Comma-separated allowed origins |
| `RATE_LIMIT_ENABLED` | `true` | Enable IP-based rate limiting |
| `RATE_LIMIT_REQUESTS` | `100` | Requests per window |
| `RATE_LIMIT_WINDOW` | `60` | Window in seconds |
| `CACHE_ENABLED` | `true` | Enable in-memory cache |
| `DB_POOL_MIN_CONNS` | `2` | Minimum pool connections |
| `DB_POOL_MAX_CONNS` | `10` | Maximum pool connections |
| `NEWS_API_KEY` | _(empty)_ | NewsAPI.org key (fallback news source) |
| `TWITTER_BEARER_TOKEN` | _(empty)_ | X/Twitter API v2 bearer token |
| `TWITTER_JOURNALIST_LIST_ID` | _(empty)_ | Curated journalist X List ID |

## Deployment

The Go API is deployed on [Railway](https://railway.app) using Railpack (Railway's native Go builder). No Docker.

### Railway Configuration (`railway.toml`)

```toml
[build]
builder = "RAILPACK"
watchPatterns = ["cmd/**", "internal/**", "go.mod", "go.sum"]

[deploy]
startCommand = "./out"
healthcheckPath = "/health"
healthcheckTimeout = 100
restartPolicyType = "ON_FAILURE"
restartPolicyMaxRetries = 10
```

Railpack detects `go.mod` at the repo root, reads Go 1.25 from it, compiles a static binary (`CGO_ENABLED=0`, `-ldflags="-w -s"`), and outputs it as `./out`.

### Railway Environment Variables

Set `RAILPACK_GO_BIN=api` to tell Railpack to build `cmd/api` (since `cmd/ingest` also exists).

### Pre-deployment Checklist

1. Apply `schema.sql` to the Neon database (if schema has changed)
2. Set all required env vars on Railway (including `RAILPACK_GO_BIN=api`)
3. Push to `main` — Railway auto-deploys
4. Verify `/health/db` returns `"database": "connected"`

## Quick Start

```bash
cd scoracle-data

# Install dependencies
go mod tidy

# Copy env vars
cp .env.example .env
# Edit .env with your API keys and database URL

# Build
go build -o bin/scoracle-api ./cmd/api
go build -o bin/scoracle-ingest ./cmd/ingest

# Seed data
./bin/scoracle-ingest seed nba
./bin/scoracle-ingest percentiles

# Run the API
./bin/scoracle-api
# -> http://localhost:8000
# -> http://localhost:8000/docs/ (Swagger UI)
```
