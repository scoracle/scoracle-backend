# Scoracle Data

Backend data pipeline and unified API for Scoracle sports data.

## Architecture

Scoracle runs as a single public Go API backed by PostgreSQL, plus a Python seeder.

- **Go API (`:8000`)** serves curated sport data pages, third-party integrations (news, journalist tweets), health/docs endpoints, and background workers.
- **Python Seeder (`seed/`)** ingests provider data and upserts raw rows to PostgreSQL.
- **PostgreSQL (`sql/`)** is the source of truth for schema, derived stats, percentiles, views, and API-shaping SQL.

The frontend calls one API origin and receives page-shaped JSON payloads designed for direct rendering.

## Service Responsibilities

| Component | Responsibility | Location |
|---|---|---|
| Go API | Public HTTP API, caching, ETags, CORS, rate limiting, integrations, worker runtime | `go/` |
| Python Seeder | Provider ingestion and fixture processing | `seed/` |
| PostgreSQL | Data model, stat normalization, derived metrics, percentile logic, shaping views/functions | `sql/` |

## API Surface

Canonical data routes are sport-scoped and page-shaped:

- `GET /api/v1/{sport}/players/{id}`
- `GET /api/v1/{sport}/teams/{id}`
- `GET /api/v1/{sport}/standings`
- `GET /api/v1/{sport}/leaders`
- `GET /api/v1/{sport}/search`
- `GET /api/v1/{sport}/stat-definitions`
- `GET /api/v1/football/leagues`

Integrations and operational routes:

- `GET /api/v1/news/status`
- `GET /api/v1/news/{entityType}/{entityID}`
- `GET /api/v1/twitter/status`
- `GET /api/v1/twitter/journalist-feed`
- `GET /health`, `GET /health/db`, `GET /health/cache`
- `GET /docs/`

See `ENDPOINTS.md` for full contract details.

## Implementation Notes

- Core data handlers live in `go/internal/api/handler/data.go` and follow a strict thin pattern (validate -> cache -> prepared statement -> passthrough JSON).
- Prepared statements for page payloads are registered in `go/internal/db/db.go` and return final JSON documents for frontend widgets.
- Sport routes are constrained to `nba`, `nfl`, and `football` at the router level.
- Data endpoints use in-memory caching with ETag support (`TTLData=5m`), while integrations use their own TTL strategy.

## Repository Layout

```text
scoracle-data/
├── README.md
├── ENDPOINTS.md
├── docker-compose.yml
├── sql/                    # Postgres schemas, views, functions, triggers
├── go/                     # Unified public API service
│   ├── cmd/api/
│   ├── internal/
│   ├── docs/
│   ├── Dockerfile
│   └── go.mod
├── seed/                   # Python seeder and provider clients
├── planning_docs/
└── progress_docs/
```

## Quick Start

### Docker Compose

```bash
cp .env.example .env
docker compose up --build
docker compose run --rm seed process --max 50
```

Local URL: `http://localhost:8000`

### Run Components Manually

Go API:

```bash
cd go
go build -o bin/scoracle-api ./cmd/api
./bin/scoracle-api
```

Python seeder:

```bash
cd seed
pip install -e .

scoracle-seed bootstrap-teams nba --season 2025
scoracle-seed load-fixtures nba --season 2025
scoracle-seed process --max 50
```

## Testing

```bash
cd go && go test ./...
cd go && go build -o bin/scoracle-api ./cmd/api
```

## Environment Variables

See `.env.example`.

Required for local operation:

- `DATABASE_URL` (or `NEON_DATABASE_URL_V2`/`NEON_DATABASE_URL`)
- `BALLDONTLIE_API_KEY` (seeder)
- `SPORTMONKS_API_TOKEN` (seeder)

Common optional:

- `API_PORT`
- `CACHE_ENABLED`
- `RATE_LIMIT_ENABLED`
- `NEWS_API_KEY`
- `TWITTER_BEARER_TOKEN`
- `TWITTER_JOURNALIST_LIST_ID`
- `FIREBASE_CREDENTIALS_FILE`
