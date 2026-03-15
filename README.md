# Scoracle Data

Backend data pipeline and API for Scoracle. Seeds sports statistics from external APIs into Neon PostgreSQL, computes derived stats via Postgres triggers, calculates percentile rankings, and serves everything through two co-equal services: **PostgREST** for data endpoints and **Go** for third-party integrations.

**Database:** PostgreSQL (Neon) only.

## Sports Covered

- **NBA** — Basketball statistics with per-36 minute normalization and True Shooting %
- **NFL** — American football with position-specific stat groupings
- **Football (Soccer)** — Top 5 European leagues with per-90 minute normalization

## Architecture Overview

Two API services sit in front of Postgres, each with a distinct role. PostgREST handles the core data endpoints (stats, profiles, rankings). Go handles third-party integrations (news, Twitter) and ingestion. A separate Go CLI seeds data from external providers.

```
┌─────────────────────────────────────────────────────────────────┐
│                        Frontend (Astro)                         │
└──────────────┬──────────────────────────────────┬───────────────┘
               │ Stats, profiles, rankings        │ News, tweets
┌──────────────▼──────────────────┐  ┌────────────▼───────────────┐
│   PostgREST — Data Provider     │  │   Go API — Integrations    │
│                                 │  │                             │
│   Auto-generated REST from      │  │   Third-party APIs:        │
│   Postgres views & functions.   │  │   Google News, NewsAPI,    │
│   JWT auth, row-level security. │  │   X/Twitter journalist     │
│   Zero application code.        │  │   feed. In-memory cache,   │
│   Port 3000                     │  │   ETag, gzip, rate limit.  │
│                                 │  │   Port 8000                │
│   Swagger UI: /docs/ serves     │  │                             │
│   both specs via multi-spec     │  │   Also serves health,      │
│   dropdown.                     │  │   Swagger UI, and stats    │
│                                 │  │   passthrough endpoints.   │
└──────────────┬──────────────────┘  └────────────┬───────────────┘
               │                                  │
               │         pgxpool + prepared        │
               │           statements              │
┌──────────────▼──────────────────────────────────▼───────────────┐
│                   PostgreSQL (Neon)                              │
│   All data logic: derived stat triggers, percentile functions,  │
│   views (v_player_profile, v_team_profile), API functions       │
│   (api_player_profile, api_entity_stats), materialized views.   │
└──────────────▲──────────────────────────────────────────────────┘
               │ pgxpool + prepared statements
┌──────────────┴──────────────────────────────────────────────────┐
│                   Go Ingestion CLI (Cobra)                       │
│   Provider-specific handlers fetch from external APIs,          │
│   normalize to canonical structs, upsert to Postgres.           │
└─────────────────────────────────────────────────────────────────┘
```

### Service Roles

| Service | Role | Port | Dockerfile |
|---------|------|------|------------|
| **PostgREST** | Data provider — stats, profiles, rankings, autofill. Auto-generated REST API from Postgres views and functions. | 3000 | `postgrest/Dockerfile` |
| **Go API** | Integrations provider — news, Twitter, health checks, Swagger UI. Also proxies stats endpoints via Postgres JSON passthrough. | 8000 | `go/Dockerfile` |
| **Go CLI** | Data ingestion — seeds stats from BallDontLie (NBA/NFL) and SportMonks (Football). | N/A | N/A (ad-hoc tool) |

### Multi-Spec Swagger UI

The Go API serves a unified Swagger UI at `/docs/` with a spec-selector dropdown. Both the Go API spec (auto-generated by swaggo) and the PostgREST OpenAPI spec are browsable from one interface. This gives frontend developers a single entry point to explore all available endpoints across both services.

### Key Design Decisions

- **Postgres-as-serializer** — SQL functions (`api_player_profile`, `api_entity_stats`, etc.) return complete JSON responses. Go is a pure transport layer.
- **No service layer in the API** — handlers call pgxpool directly because Postgres functions ARE the service layer.
- **No shared Provider interface** — provider-agnosticism comes from canonical output structs, not input interfaces. Adding a new provider means adding a new handler package; nothing else changes.
- **Separate seed runner per sport** — NBA, NFL, and Football each have their own orchestration file since their data flows differ.

## Docker

Both services are containerized with lean, production-ready Dockerfiles and orchestrated via Docker Compose.

### Quick Start

```bash
# Copy env vars and fill in your values
cp .env.example .env

# Start both services
docker compose up --build

# PostgREST → http://localhost:3000
# Go API    → http://localhost:8000
# Swagger   → http://localhost:8000/docs/
```

### Images

| Service | Base | Final Size | Details |
|---------|------|------------|---------|
| Go API | `golang:1.25-alpine` → `alpine:3.21` | ~15MB | Multi-stage build, static binary, stripped symbols, non-root user (UID 1000) |
| PostgREST | `postgrest/postgrest:v13.0.8` | ~30MB | Health check, non-root user (UID 1000) |

### Compose Services

```yaml
postgrest:  # PostgREST on :3000 — data provider
api:        # Go API on :8000 — integrations, depends on postgrest health
```

Compose reads your `.env` file directly. It maps `DATABASE_URL` to `PGRST_DB_URI` and overrides `POSTGREST_URL` for inter-container networking (service name resolution). No additional configuration needed.

### Without Docker

```bash
go mod tidy
go build -o bin/scoracle-api ./cmd/api
go build -o bin/scoracle-ingest ./cmd/ingest
./bin/scoracle-api
```

## Data Sources

| Sport | Provider | Go Handler |
|-------|----------|------------|
| NBA | [BallDontLie](https://balldontlie.io) | `internal/provider/bdl/nba.go` |
| NFL | [BallDontLie](https://balldontlie.io) | `internal/provider/bdl/nfl.go` |
| Football | [SportMonks](https://sportmonks.com) | `internal/provider/sportmonks/football.go` |

## How Seeding Works

1. **Provider handlers** (`internal/provider/`) — Fetch data from external APIs and normalize into canonical Go structs (`Team`, `Player`, `PlayerStats`, `TeamStats`). Each provider has its own HTTP client with rate limiting and pagination.

2. **Seed runners** (`internal/seed/`) — Provider-agnostic orchestration that takes canonical structs and upserts into Postgres. Each sport has its own runner: `SeedNBA`, `SeedNFL`, `SeedFootballSeason`.

3. **Derived stats** — Postgres triggers auto-compute per-36, per-90, TS%, win_pct, and other derived metrics on INSERT/UPDATE.

4. **Percentiles** — `recalculate_percentiles()` computes per-position percentile rankings.

## Database Schema

Modular SQL files in the `sql/` directory, organized by sport. Shared tables live in `public`, sport-specific views and functions live in Postgres schemas (`nba`, `nfl`, `football`). PostgREST uses multi-schema mode — the frontend selects the sport via the `Accept-Profile` header.

```bash
# Apply to a fresh database
psql -f sql/shared.sql     # tables, roles, shared functions
psql -f sql/nba.sql         # NBA schema: views, triggers, stat defs, grants
psql -f sql/nfl.sql         # NFL schema
psql -f sql/football.sql    # Football schema
```

### Core Tables

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

### Per-Sport Schemas

Each sport has its own Postgres schema with views, functions, and materialized views:

| Schema | Views | Functions | Materialized Views |
|--------|-------|-----------|-------------------|
| `nba` | `players`, `player_stats`, `teams`, `team_stats`, `standings`, `stat_definitions` | `stat_leaders()`, `health()` | `autofill_entities` |
| `nfl` | `players`, `player_stats`, `teams`, `team_stats`, `standings`, `stat_definitions` | `stat_leaders()`, `health()` | `autofill_entities` |
| `football` | `players`, `player_stats`, `teams`, `team_stats`, `standings`, `stat_definitions`, `leagues` | `stat_leaders()`, `health()` | `autofill_entities` |

## API Endpoints

### Go API (`:8000`)

| Endpoint | Description |
|----------|-------------|
| `GET /` | API info (name, version, status) |
| `GET /health` | Basic health check |
| `GET /health/db` | Database connectivity |
| `GET /health/cache` | Cache statistics |
| `GET /docs/` | Multi-spec Swagger UI (Go + PostgREST) |
| `GET /api/v1/news/{type}/{id}?sport=&source=` | News articles (Google News RSS + NewsAPI) |
| `GET /api/v1/twitter/journalist-feed?q=&sport=` | Curated journalist tweets from X List |

### PostgREST (`:3000`)

Auto-generated REST endpoints from per-sport Postgres schemas (`nba`, `nfl`, `football`). The frontend selects the sport via the `Accept-Profile` header. Supports filtering, ordering, pagination, and JWT authentication out of the box. See the Swagger UI at `/docs/` for the full spec.

## CLI Commands

```bash
# Build
go build -o bin/scoracle-api ./cmd/api
go build -o bin/scoracle-ingest ./cmd/ingest

# Data seeding
./bin/scoracle-ingest seed nba [--season 2025]
./bin/scoracle-ingest seed nfl [--season 2025]
./bin/scoracle-ingest seed football [--season 2025] [--league 8]

# Percentile recalculation
./bin/scoracle-ingest percentiles [--sport NBA] [--season 2025]

# Fixture processing
./bin/scoracle-ingest fixtures process [--sport NBA] [--max 10] [--workers 2]
```

## Codebase Structure

```
scoracle-data/
├── sql/                               # Database schema (modular, per-sport)
│   ├── shared.sql                     # Public tables, roles, shared functions
│   ├── nba.sql                        # NBA schema: views, triggers, stat defs
│   ├── nfl.sql                        # NFL schema: views, triggers, stat defs
│   ├── football.sql                   # Football schema: views, triggers, leagues
│   └── platform.sql                   # Stub for future user/follows surface
├── go.mod / go.sum                    # Go module definition
├── docker-compose.yml                 # Orchestrates Go API + PostgREST
├── .dockerignore                      # Build context filter
├── .env.example                       # Environment variable template
├── railway.toml                       # Railway deployment config
│
├── cmd/                               # Go entry points
│   ├── api/main.go                    # API server (graceful shutdown, maintenance tickers)
│   └── ingest/main.go                 # Cobra CLI (seed, percentiles, fixtures)
│
├── internal/                          # Go internal packages
│   ├── api/                           # Chi router, middleware, response helpers, handlers
│   ├── cache/                         # In-memory TTL cache with ETag
│   ├── config/                        # Env var loading, sport registry
│   ├── db/                            # pgxpool wrapper, prepared statements
│   ├── thirdparty/                    # News (RSS + NewsAPI) and Twitter clients
│   ├── provider/                      # External API handlers (BDL, SportMonks)
│   ├── seed/                          # Per-sport seed orchestration and upsert logic
│   ├── fixture/                       # Fixture processing and scheduling
│   ├── listener/                      # LISTEN/NOTIFY consumer for milestones
│   ├── maintenance/                   # Periodic tickers (cleanup, digest, catch-up)
│   └── notifications/                 # Milestone detection, FCM dispatch pipeline
│
├── docs/                              # Auto-generated Swagger specs (swaggo)
│
├── go/                                # Go service Docker config
│   └── Dockerfile                     # Multi-stage: golang:1.25-alpine → alpine:3.21
│
├── postgrest/                         # PostgREST service Docker config
│   ├── Dockerfile                     # postgrest/postgrest:v13.0.8 + healthcheck
│   └── passwd                         # Non-root user definition
│
├── legacy_fastapi/                    # Python reference (FastAPI, seeders, tests)
├── planning_docs/                     # Architecture and design documents
└── progress_docs/                     # Session progress tracking
```

## Deployment

### Railway (Production)

The Go API deploys to [Railway](https://railway.app) using its Dockerfile (`go/Dockerfile`). Railway builds the multi-stage image, runs the health check, and manages restarts.

PostgREST deploys as a separate Railway service using `postgrest/Dockerfile`.

Both services connect to the same Neon PostgreSQL instance.

### Railway Configuration (`railway.toml`)

```toml
[build]
builder = "DOCKERFILE"
dockerfilePath = "go/Dockerfile"
watchPatterns = ["cmd/**", "internal/**", "go.mod", "go.sum", "go/Dockerfile"]

[deploy]
healthcheckPath = "/health"
healthcheckTimeout = 100
restartPolicyType = "ON_FAILURE"
restartPolicyMaxRetries = 10
```

## Environment Variables

See `.env.example` for the full documented template.

**Required:**

| Variable | Description |
|----------|-------------|
| `DATABASE_URL` | PostgreSQL connection string (Neon) |
| `BALLDONTLIE_API_KEY` | BallDontLie API key (NBA + NFL ingestion) |
| `SPORTMONKS_API_TOKEN` | SportMonks API token (Football ingestion) |

**Optional:**

| Variable | Default | Description |
|----------|---------|-------------|
| `API_PORT` | `8000` | Go API port |
| `ENVIRONMENT` | `development` | `development`, `staging`, `production` |
| `CORS_ALLOW_ORIGINS` | `localhost:3000,4321,5173` | Comma-separated allowed origins |
| `RATE_LIMIT_ENABLED` | `true` | Enable IP-based rate limiting |
| `CACHE_ENABLED` | `true` | Enable in-memory cache |
| `NEWS_API_KEY` | _(empty)_ | NewsAPI.org key |
| `TWITTER_BEARER_TOKEN` | _(empty)_ | X/Twitter API v2 bearer token |
| `POSTGREST_URL` | `http://localhost:3000` | PostgREST URL (overridden by Compose) |

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
