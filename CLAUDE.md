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

## Build & Test

```bash
cd go && go test ./...
cd go && go build -o bin/scoracle-api ./cmd/api
```

## Environment

DB URL priority in Go config:

`NEON_DATABASE_URL_V2` > `DATABASE_URL` > `NEON_DATABASE_URL`

See `.env.example` for complete local defaults.

## Progress Docs

For major changes, add a session summary markdown file in `progress_docs/` with:

- goals
- decisions
- accomplishments
- quick reference
- updated file layout (if structure changed)
