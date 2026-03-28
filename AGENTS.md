# AGENTS.md

Coding agent guide for Scoracle Data. Read `CLAUDE.md` for detailed architecture rules.

## Architecture

Two runtime components, one database:

- **Go API** (`go/`, port 8000): unified public API for sport data pages, news, tweets, health/docs, notifications workers.
- **Python Seeder** (`seed/`): provider ingestion and fixture processing.
- **PostgreSQL** (`sql/`): domain engine (schema, derived stats, percentiles, shaped views/functions).

## Build Commands

### Go API

```bash
cd go

# Build
 go build -o bin/scoracle-api ./cmd/api

# Run
 ./bin/scoracle-api

# Run with custom port
 API_PORT=8080 ./bin/scoracle-api
```

### Python Seeder

```bash
cd seed

# Install (editable)
 pip install -e .

# Run commands
 scoracle-seed bootstrap-teams nba --season 2025
 scoracle-seed load-fixtures nba --season 2025
 scoracle-seed process --max 50
 scoracle-seed seed-fixture <fixture-id>
```

### Docker

```bash
# Full stack
docker compose up --build

# Run seeder via compose
docker compose run --rm seed process --max 50
```

## Test Commands

### Go Tests

```bash
cd go

# Run all tests
go test ./...

# Run specific package
go test ./internal/api/...

# Run single test (by name pattern)
go test ./internal/api -run TestRouteOwnershipSplit -v

# Run with verbose output
go test ./... -v

# Run with race detector
go test ./... -race

# Run with coverage
go test ./... -cover
```

### Python Tests

```bash
cd seed

# Run all tests
pytest

# Run specific test file
pytest tests/test_models.py

# Run specific test
pytest tests/test_models.py::test_team_defaults -v

# Run with verbose output
pytest -v
```

## Lint/Format Commands

This repository uses standard Go formatting and Python conventions. No custom linter configs are present.

```bash
# Go formatting (standard)
cd go && gofmt -w .

# Go vet (static analysis)
cd go && go vet ./...
```

## Code Style Guidelines

### Go Style

- **Formatting**: Use `gofmt`. Standard Go conventions.
- **Naming**:
  - PascalCase for exported (public) identifiers
  - camelCase for unexported (private) identifiers
  - No underscores in names (except in tests)
- **Imports**: Group by (1) stdlib, (2) third-party, (3) internal project imports. Use blank line between groups.
- **Error Handling**:
  - Wrap errors with context: `fmt.Errorf("context: %w", err)`
  - Return early on errors
  - Use sentinel errors sparingly (e.g., `pgx.ErrNoRows`)
- **Comments**: All exported symbols must have doc comments starting with the symbol name.
- **Structure**: Keep handlers thin (validate → cache → prepared statement → passthrough JSON).

### Python Style

- **Formatting**: Standard Python formatting (no custom config found).
- **Naming**: snake_case for functions/variables, PascalCase for classes.
- **Type Hints**: Use type hints on function signatures and dataclasses.
- **Imports**: Group by (1) stdlib, (2) third-party, (3) internal. Use `from __future__ import annotations` for forward references.
- **Structure**: Use dataclasses for models. Keep seeding logic thin (call provider → normalize → upsert).

### SQL Style

- Schemas per sport: `nba.*`, `nfl.*`, `football.*`
- Shared tables in `public` or sport-agnostic schemas
- Use `json_build_object` and `row_to_json` for API-shaped responses
- Percentiles and derived stats belong in Postgres (triggers/functions), not in Go/Python

## Core Rules

1. Postgres remains the serializer/contract engine for sport data.
2. Go handlers stay thin: validate → cache → prepared statement → passthrough JSON.
3. No service/repository layer between handlers and `pgxpool`.
4. No derived-stat or percentile logic in Go/Python.
5. Python is ingestion only.
6. Add prepared statements in `go/internal/db/db.go` for new DB reads.
7. Swagger annotations required for all handlers (swaggo format).

## Public Route Shape

Sport-scoped, page-shaped resource endpoints:

- `/api/v1/{sport}/{entityType}/{id}` (player, team profiles)
- `/api/v1/{sport}/meta` (metadata, autofill, stat definitions)
- `/api/v1/{sport}/health` (data freshness)
- `/api/v1/{sport}/leagues/{leagueId}/...` (league-scoped variants)

Integrations:

- `/api/v1/news/...`
- `/api/v1/twitter/...`

## Environment Variables

DB URL priority: `NEON_DATABASE_URL_V2` > `DATABASE_URL` > `NEON_DATABASE_URL`

See `.env.example` for complete local defaults.

## Key Files

- `go/internal/db/db.go`: Prepared statements registration
- `go/internal/api/handler/data.go`: Data endpoint handlers
- `go/internal/api/server.go`: Route wiring
- `seed/scoracle_seed/cli.py`: CLI entry point
- `sql/*.sql`: Database schemas and functions
