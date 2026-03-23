# AGENTS.md

Coding agent guide for Scoracle Data. Read `CLAUDE.md` for repository rules.

## Architecture Summary

Two runtime components, one database:

- **Go API** (port 8000): unified public API for sport data pages, news, tweets, health/docs, notifications workers.
- **Python Seeder** (`seed/`): provider ingestion and fixture processing.
- **PostgreSQL**: domain engine (schema, derived stats, percentiles, shaped views/functions).

## Build & Run

Go API (`go/`):

```bash
go build -o bin/scoracle-api ./cmd/api
./bin/scoracle-api
```

Seeder (`seed/`):

```bash
scoracle-seed bootstrap-teams nba --season 2025
scoracle-seed load-fixtures nba --season 2025
scoracle-seed process --max 50
```

Docker (`repo root`):

```bash
docker compose up --build
docker compose run --rm seed process --max 50
```

## Testing

```bash
cd go && go test ./...
```

Use standard `testing` + `httptest`; table-driven tests preferred.

## Core Rules

1. Postgres remains serializer/contract engine for sport data.
2. Go handlers stay thin (validate -> cache -> prepared statement -> passthrough JSON).
3. No service/repository layer between handlers and `pgxpool`.
4. No derived-stat or percentile logic in Go/Python.
5. Python is ingestion only.
6. Add prepared statements in `go/internal/db/db.go` for new DB reads.

## Public Route Shape

Use sport-scoped, page-shaped resource endpoints:

- `/api/v1/{sport}/players/{id}`
- `/api/v1/{sport}/teams/{id}`
- `/api/v1/{sport}/standings`
- `/api/v1/{sport}/leaders`
- `/api/v1/{sport}/search`
- `/api/v1/{sport}/stat-definitions`
- `/api/v1/football/leagues`

Integrations remain:

- `/api/v1/news/...`
- `/api/v1/twitter/...`
