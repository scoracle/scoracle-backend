# Scoracle Data

Backend data pipeline and unified API for the Scoracle sports platform.

## Architecture — Two Components, One Database

```
Frontend (Astro)
    └── Curated sport pages + integrations ──► Go API (:8000)
                                              │
                                   Connects to PostgreSQL
                                              ▲
                                              │
                                   Python Seeder (ingestion)
```

### Go API — Unified Public API (port 8000)

The Go API owns all public HTTP endpoints:

- Sport data endpoints (canonical profile, meta, and health plus league-scoped variants)
- Third-party integrations (news + journalist tweets)
- Health/docs endpoints
- Background workers (notifications, maintenance, LISTEN/NOTIFY)

Go handlers must remain thin:

1. parse/validate input
2. cache lookup / ETag handling
3. execute prepared statement
4. pass raw JSON through

### PostgreSQL — Contract + Domain Engine

Postgres remains the system of record for:

- schema and shared tables
- stat key normalization
- derived stats and percentiles
- standings logic
- views/functions that shape API payloads

No derived-stat, percentile, or ranking logic belongs in Go or Python.

### Python Seeder (`seed/`)

Python is ingestion-only:

- call providers
- normalize raw payloads enough to upsert
- write to shared tables
- call `finalize_fixture()`

Seeder does not own API response shaping.

## Design Rules

1. **Postgres-as-serializer** — data endpoints are JSON passthrough from SQL.
2. **No service layer** — handlers call `pgxpool` directly.
3. **No derived stats in Go/Python** — keep this in SQL triggers/functions.
4. **Per-sport boundaries** — `nba`, `nfl`, `football` logic remains separated in SQL contracts.
5. **Prepared statements required** — add all new reads in `go/internal/db/db.go`.
6. **Swagger annotations required** for all handlers (swaggo format).

## Route Conventions

Canonical public route shape:

- `/api/v1/{sport}/{entityType}/{id}` (player, team profiles)
- `/api/v1/{sport}/meta` (metadata, autofill, stat definitions)
- `/api/v1/{sport}/health` (data freshness)
- `/api/v1/{sport}/leagues/{leagueId}/...` (league-scoped variants)

Integrations:

- `/api/v1/news/...`
- `/api/v1/twitter/...`

## Implementation Boundaries

- Route wiring belongs in `go/internal/api/server.go`.
- Data endpoint handler logic belongs in `go/internal/api/handler/data.go`.
- Response helpers live in `go/internal/api/respond/`.
- Caching policy defaults live in `go/internal/cache/cache.go`.
- Query contracts are prepared statements in `go/internal/db/db.go`.

Any new public data endpoint must follow this flow:

1. Add a prepared statement in `go/internal/db/db.go` that returns final JSON.
2. Add a thin handler in `go/internal/api/handler/data.go`.
3. Wire route in `go/internal/api/server.go` under `/api/v1/{sport}` or sport-specific path.
4. Update `ENDPOINTS.md`, `README.md`, and Swagger annotations.

## Build & Run

### Go API

```bash
cd go
go build -o bin/scoracle-api ./cmd/api
./bin/scoracle-api
# custom port: API_PORT=8080 ./bin/scoracle-api
```

### Python Seeder

```bash
cd seed
pip install -e .

scoracle-seed event load-fixtures nba --season 2025
scoracle-seed event process --sport nba --season 2025 --max 50
scoracle-seed meta seed nba --season 2025
```

### Docker

```bash
docker compose up --build
docker compose run --rm seed event process --max 50
```

## Test

### Go

```bash
cd go
go test ./...                                # all
go test ./internal/api/... -v                # package
go test ./internal/api -run TestName -v      # single test
go test ./... -race -cover                   # race + coverage
```

### Python

```bash
cd seed
pytest
pytest tests/test_models.py::test_team_defaults -v
```

### Lint / Format

```bash
cd go && gofmt -w . && go vet ./...
```

## Code Style

### Go

- `gofmt` is authoritative. No custom config.
- PascalCase exported, camelCase unexported, no underscores (except tests).
- Imports grouped: stdlib / third-party / internal, blank line between groups.
- Errors: wrap with `fmt.Errorf("context: %w", err)`, return early, use sentinel errors sparingly (e.g. `pgx.ErrNoRows`).
- Exported symbols need doc comments starting with the symbol name.
- Handlers stay thin: validate → cache → prepared statement → passthrough JSON.

### Python

- snake_case functions/variables, PascalCase classes.
- Type hints on function signatures and dataclasses.
- Imports grouped: stdlib / third-party / internal. Use `from __future__ import annotations` for forward refs.
- Dataclasses for models. Seeder stays thin: call provider → normalize → upsert.

### SQL

- Schemas per sport: `nba.*`, `nfl.*`, `football.*`.
- Shared tables in `public` (or sport-agnostic schemas).
- Use `json_build_object` and `row_to_json` for API-shaped responses.
- Percentiles and derived stats belong in Postgres (triggers/functions), never in Go or Python.

## Environment

Go config resolves the DB URL in this order:

`DATABASE_PRIVATE_URL` > `DATABASE_URL`

Env file convention:

- `.env` — committed template with safe defaults / placeholders only.
- `.env.local` — gitignored, real values (DB creds, provider keys). Loaded with priority over `.env`.

Required for local operation:

- `DATABASE_PRIVATE_URL` (or `DATABASE_URL`)
- `BALLDONTLIE_API_KEY` (seeder, NBA/NFL)
- `SPORTMONKS_API_TOKEN` (seeder, football)

Common optional:

- `API_PORT`, `CACHE_ENABLED`, `RATE_LIMIT_ENABLED`
- `NEWS_API_KEY`
- `TWITTER_BEARER_TOKEN`, `TWITTER_LIST_NBA`, `TWITTER_LIST_NFL`, `TWITTER_LIST_FOOTBALL`
- `TWITTER_CACHE_TTL_SECONDS` (default `1200`)
- `FIREBASE_CREDENTIALS_FILE`

## Key Files

- `go/internal/db/db.go` — prepared statement registration
- `go/internal/api/handler/data.go` — data endpoint handlers
- `go/internal/api/server.go` — route wiring
- `go/internal/config/config.go` — env resolution
- `seed/scoracle_seed/cli.py` — seeder CLI entry point
- `sql/*.sql` — schemas, functions, triggers

## Progress Docs

For major changes, add a session summary markdown file in `progress_docs/` with:

- goals
- decisions
- accomplishments
- quick reference
- updated file layout (if structure changed)
