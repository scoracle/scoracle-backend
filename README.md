# Scoracle Data

Backend data pipeline and API for the Scoracle sports platform.

The repository is organized around a clear split of responsibilities:

- **Python (`seed/`)** ingests schedules, fixtures, teams, and stats from external providers and upserts raw data into PostgreSQL.
- **PostgREST (`:3000`)** exposes the core data API directly from Postgres views and functions.
- **Go (`:8000`)** handles third-party integrations, health/docs endpoints, and listens to PostgreSQL events for notifications and background work.

**Database:** Neon PostgreSQL

## Sports Covered

- **NBA** — Basketball statistics with per-36 minute normalization and True Shooting %
- **NFL** — American football with position-specific stat groupings
- **Football (Soccer)** — Top 5 European leagues with per-90 minute normalization

## Architecture

Scoracle Data has **two runtime APIs and one seeding process** connected to the same PostgreSQL database.

```
┌──────────────────────────────────────────────────────────────────────┐
│                           Frontend (Astro)                          │
└───────────────────────┬──────────────────────────────┬──────────────┘
                        │                              │
                        │ stats, profiles, standings   │ news, tweets,
                        │ search, rankings             │ docs, health
                        │                              │
            ┌───────────▼───────────┐      ┌───────────▼──────────────┐
            │      PostgREST        │      │          Go API          │
            │     Data API :3000    │      │   Integrations API :8000 │
            │                       │      │                          │
            │ Reads Postgres views  │      │ News + social feeds      │
            │ and SQL functions     │      │ Swagger UI, health,      │
            │ directly              │      │ LISTEN/NOTIFY workers    │
            └───────────┬───────────┘      └───────────┬──────────────┘
                        │                              ▲
                        │                              │ NOTIFY / LISTEN
                        ▼                              │
                  ┌───────────────────────────────────────────────┐
                  │             PostgreSQL (Neon)                │
                  │  Raw stats, normalized stats, derived stats, │
                  │  percentiles, standings, views, RPCs         │
                  └───────────────────▲───────────────────────────┘
                                      │
                                      │ upserts raw data
                         ┌────────────┴────────────┐
                         │      Python Seeder      │
                         │   Provider ingestion    │
                         │   fixtures + stats      │
                         └─────────────────────────┘
```

### Service Responsibilities

| Component | Responsibility | Lives in |
|-----------|----------------|----------|
| **PostgREST** | Core data endpoints for stats, profiles, standings, stat definitions, search, and rankings | `postgrest/`, `sql/` |
| **Go API** | Third-party integrations only: news, curated tweets, health checks, Swagger UI, PostgreSQL listeners, notifications, and maintenance workers | `go/` |
| **Python Seeder** | Thin ingestion layer for provider APIs; loads schedules and fixtures, seeds stats, and calls `finalize_fixture()` | `seed/` |
| **PostgreSQL** | Source of truth for stat normalization, derived metrics, percentiles, materialized views, and API-shaping SQL | `sql/` |

### Boundary Rules

- **New data endpoints** go in Postgres and are exposed by **PostgREST**, not by the Go API.
- **New third-party integrations** go in **Go** under `go/internal/api/handler/`.
- **New provider ingestion logic** goes in **Python** under `seed/scoracle_seed/`.
- **Derived stats, percentiles, and stat-key normalization** stay in **PostgreSQL**, not in Go or Python.

## Data Flow

1. The Python seeder loads fixtures and calls external provider APIs.
2. Raw provider stats are upserted into PostgreSQL.
3. PostgreSQL triggers normalize stat keys and compute derived stats.
4. The seeder calls `finalize_fixture()` to refresh downstream derived data.
5. PostgreSQL emits `NOTIFY` events for important changes.
6. The Go service listens for those events and runs follow-up work such as notifications.
7. The frontend reads core sports data from PostgREST and third-party content from the Go API.

## API Surface

### PostgREST (`:3000`) — Core Data API

PostgREST auto-generates a REST API from the sport schemas in PostgreSQL. The frontend selects the sport schema with the `Accept-Profile` header:

- `Accept-Profile: nba`
- `Accept-Profile: nfl`
- `Accept-Profile: football`

Common data endpoints include:

- `GET /player`
- `GET /team`
- `GET /standings`
- `GET /stat_definitions`
- `GET /autofill_entities`

PostgREST is the home for **stats, profiles, standings, rankings, and search**.

### Go API (`:8000`) — Integrations and Operational Endpoints

The Go API serves:

- `GET /`
- `GET /health`
- `GET /health/db`
- `GET /health/cache`
- `GET /docs/`
- `GET /api/v1/news/{type}/{id}?sport=&source=`
- `GET /api/v1/twitter/journalist-feed?q=&sport=`

The Go API is the home for **third-party data and operational endpoints**, not core sports data.

For a fuller endpoint reference, see [`ENDPOINTS.md`](./ENDPOINTS.md).

## Repository Layout

```text
scoracle-data/
├── README.md
├── ENDPOINTS.md
├── docker-compose.yml
├── sql/                    # Postgres schemas, views, functions, triggers
├── postgrest/              # PostgREST container config
├── go/                     # Go API: handlers, cache, listeners, workers, Swagger
│   ├── cmd/api/
│   ├── internal/
│   ├── docs/
│   ├── Dockerfile
│   ├── go.mod
│   └── railway.toml
├── seed/                   # Python seeder: CLI, providers, upserts, orchestration
│   ├── scoracle_seed/
│   ├── Dockerfile
│   └── pyproject.toml
├── planning_docs/
└── progress_docs/
```

## Quick Start

### Docker Compose

```bash
# Copy env vars and fill in your values
cp .env.example .env

# Start the runtime services
docker compose up --build

# Run the seeder on demand
docker compose run --rm seed process --max 50
```

Local URLs:

- PostgREST: http://localhost:3000
- Go API: http://localhost:8000
- Swagger UI: http://localhost:8000/docs/

### Run Components Manually

#### Go API

```bash
cd go
go build -o bin/scoracle-api ./cmd/api
./bin/scoracle-api
```

#### Python Seeder

```bash
cd seed
pip install -e .

scoracle-seed bootstrap-teams nba --season 2025
scoracle-seed load-fixtures nba --season 2025
scoracle-seed process --max 50
scoracle-seed seed-fixture --id 42
scoracle-seed percentiles --sport NBA --season 2025
```

## Data Sources

| Sport | Provider | Seeder Module |
|-------|----------|---------------|
| NBA | [BallDontLie](https://balldontlie.io) | `seed/scoracle_seed/bdl_nba.py` |
| NFL | [BallDontLie](https://balldontlie.io) | `seed/scoracle_seed/bdl_nfl.py` |
| Football | [SportMonks](https://sportmonks.com) | `seed/scoracle_seed/sportmonks_football.py` |

## Database Organization

Shared tables live in `public`, while each sport has its own schema:

- `sql/shared.sql`
- `sql/nba.sql`
- `sql/nfl.sql`
- `sql/football.sql`

PostgREST runs in multi-schema mode and exposes those sport schemas as separate API surfaces selected via `Accept-Profile`.

Core shared tables include:

- `players`
- `teams`
- `player_stats`
- `team_stats`
- `stat_definitions`
- `fixtures`
- `provider_seasons`
- `sports`
- `leagues`

## Testing and Validation

```bash
# Go tests
cd go && go test ./...

# Go build
cd go && go build -o bin/scoracle-api ./cmd/api
```

For seeding validation, the usual workflow is to run the Python CLI against a configured database:

```bash
cd seed
pip install -e .
scoracle-seed process --max 1
```

## Deployment

The Go API and PostgREST deploy as separate services and connect to the same Neon PostgreSQL database.

- **Go API** uses `go/Dockerfile` and `go/railway.toml`
- **PostgREST** uses `postgrest/Dockerfile`

## Environment Variables

See `.env.example` for the full template.

### Required

| Variable | Description |
|----------|-------------|
| `DATABASE_URL` | PostgreSQL connection string |
| `BALLDONTLIE_API_KEY` | BallDontLie API key for NBA and NFL seeding |
| `SPORTMONKS_API_TOKEN` | SportMonks API token for football seeding |

### Common Optional Variables

| Variable | Description |
|----------|-------------|
| `API_PORT` | Go API port |
| `POSTGREST_URL` | URL used by the Go service when it needs to reach PostgREST |
| `NEWS_API_KEY` | NewsAPI.org key |
| `TWITTER_BEARER_TOKEN` | X/Twitter API bearer token |
| `CACHE_ENABLED` | Enable or disable the Go in-memory cache |
| `RATE_LIMIT_ENABLED` | Enable or disable Go API rate limiting |
