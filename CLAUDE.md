# Scoracle Data

Backend data pipeline and API for the Scoracle sports stats platform. Seeds NBA, NFL, and Football (soccer) statistics into Neon PostgreSQL, computes derived stats via Postgres triggers, and serves everything through two separate services with distinct responsibilities.

## Architecture — Two Services, One Database

```
Frontend (Astro)
    ├── Stats, profiles, rankings ──► PostgREST (:3000)
    └── News, tweets ───────────────► Go API (:8000)
                                          │
                              Both connect to ──► Neon PostgreSQL
```

### PostgREST — The Data API (port 3000)

PostgREST auto-generates a REST API from per-sport Postgres schemas (`nba`, `nfl`, `football`). It owns **all core data endpoints**: player/team stats, profiles, standings, stat definitions, autofill/search, and leagues. It uses JWT auth, row-level security, and requires zero application code — adding a new data endpoint means adding a view or function to the sport's schema file in `sql/`, not writing Go.

PostgREST uses multi-schema mode (`PGRST_DB_SCHEMAS=nba,nfl,football`). The frontend selects the sport via the `Accept-Profile` header (e.g., `Accept-Profile: nba`). Each sport schema exposes views like `players`, `player_stats`, `standings`, and RPC functions like `stat_leaders()`. The `web_anon` role has read-only access; `web_user` can also manage follows/subscriptions.

### Go API — Integrations & Ingestion (port 8000)

The Go API handles **third-party integrations only**: news (Google News RSS + NewsAPI) and curated journalist tweets from X. It also serves health checks, the multi-spec Swagger UI, and background workers (notifications, maintenance, LISTEN/NOTIFY).

The Go API does **not** own stats, profiles, or any core data endpoints. Those moved to PostgREST. If you see stats/profile routes referenced in old code or docs, they belong to PostgREST now.

### Where New Endpoints Go

| Data comes from...         | Build it in...                              |
|----------------------------|---------------------------------------------|
| Postgres (stats, profiles) | `sql/<sport>.sql` — add a view/function in the sport's schema + wire it in `sql/api_views.sql`, PostgREST exposes it automatically |
| Third-party API            | Go — add a handler in `go/internal/api/handler/` |

## Legacy Code — Do Not Use

The `legacy_fastapi/` directory contains the original Python/FastAPI implementation. It is **not active**, not deployed, and not maintained. Do not modify, reference, or port patterns from it. All active code is in `go/`.

Some env var comments and cache constants still reference the Python codebase (e.g., `core/types.py`). Ignore these — Go is the source of truth.

## Key Design Rules

1. **Postgres-as-serializer** — SQL functions (`api_player_profile`, `api_entity_stats`, etc.) return complete JSON. Go handlers pass raw `[]byte` straight to the HTTP response. No struct scanning, no marshaling.

2. **No service layer** — Handlers call `pgxpool` directly. Postgres functions ARE the service layer. Do not introduce a service/repository pattern between handlers and the database.

3. **No shared Provider interface** — Provider-agnosticism comes from canonical output structs (`provider.Team`, `provider.Player`, `provider.PlayerStats`, `provider.TeamStats`), not input interfaces. Adding a new data provider means adding a new handler package under `go/internal/provider/`; nothing else changes.

4. **Per-sport schema separation** — The database uses separate Postgres schemas per sport (`nba`, `nfl`, `football`) for views, functions, and PostgREST surface. Shared tables live in `public`. Each sport has its own self-contained SQL file in `sql/` (`nba.sql`, `nfl.sql`, `football.sql`). See `planning_docs/SPORT_SCHEMA_SEPARATION.md` for the full plan.

5. **Derived stats in Postgres** — Triggers auto-compute per-36, per-90, TS%, win_pct, and other derived metrics on INSERT/UPDATE. Go never calculates derived stats.

6. **Percentiles in Postgres** — `recalculate_percentiles()` runs in Postgres. Go calls it but doesn't compute percentiles.

7. **JSONB for sport-specific data** — The `stats` and `meta` columns are JSONB. No schema changes needed for new stat keys — add them to `stat_definitions` and the provider handler.

> **Critical goal: independent sport growth.** Each sport's database schema, seed pipeline, and API surface must be owned and evolved independently. Adding stats, derived metrics, standings logic, or API views for one sport must never require touching another sport's code or schema. This enables assigning a product owner per sport and makes adding a new sport a bounded, predictable task. See `planning_docs/SPORT_SCHEMA_SEPARATION.md`.

## Codebase Layout

All Go code lives under `go/`:

```
go/
├── cmd/api/main.go              # API server entry point
├── cmd/ingest/main.go           # Cobra CLI for seeding + percentiles + fixtures
├── internal/
│   ├── api/
│   │   ├── server.go            # Chi router setup, middleware, route registration
│   │   ├── middleware.go         # Timing, rate limiting
│   │   ├── respond/respond.go   # JSON response helpers (WriteJSON, WriteError)
│   │   └── handler/             # HTTP handlers (one file per domain)
│   │       ├── handler.go       # Handler struct with shared deps (pool, cache, config)
│   │       ├── news.go          # News endpoints
│   │       ├── twitter.go       # Twitter/journalist feed
│   │       ├── profile.go       # Profile passthrough (Postgres JSON)
│   │       ├── stats.go         # Stats passthrough (Postgres JSON)
│   │       └── bootstrap.go     # Autofill/search endpoint
│   ├── cache/cache.go           # In-memory TTL cache with ETag support
│   ├── config/config.go         # Env var loading, sport registry, table name constants
│   ├── db/db.go                 # pgxpool wrapper, prepared statement registration
│   ├── provider/
│   │   ├── canonical.go         # Canonical structs (Team, Player, PlayerStats, TeamStats)
│   │   ├── bdl/                 # BallDontLie handlers (NBA, NFL)
│   │   └── sportmonks/          # SportMonks handler (Football)
│   ├── seed/                    # Per-sport seed orchestration + upsert functions
│   ├── fixture/                 # Post-match fixture processing
│   ├── thirdparty/              # News + Twitter clients
│   ├── listener/                # Postgres LISTEN/NOTIFY consumer
│   ├── maintenance/             # Periodic background tickers
│   └── notifications/           # Milestone detection + FCM push pipeline
├── docs/                        # Auto-generated Swagger specs (swaggo)
├── go.mod                       # Go 1.25, module: github.com/albapepper/scoracle-data
├── Dockerfile                   # Multi-stage: golang:1.25-alpine → alpine:3.21
└── railway.toml                 # Railway deployment config
```

## Build & Run

```bash
# From the go/ directory:

# Build
go build -o bin/scoracle-api ./cmd/api
go build -o bin/scoracle-ingest ./cmd/ingest

# Run API server
./bin/scoracle-api

# Seed data
./bin/scoracle-ingest seed nba --season 2025
./bin/scoracle-ingest seed nfl --season 2025
./bin/scoracle-ingest seed football --season 2025 --league 8

# Recalculate percentiles
./bin/scoracle-ingest percentiles --sport NBA --season 2025

# Process fixtures
./bin/scoracle-ingest fixtures process --sport NBA --max 10 --workers 2

# Docker (both services)
cd .. && docker compose up --build
# PostgREST → http://localhost:3000
# Go API    → http://localhost:8000
# Swagger   → http://localhost:8000/docs/
```

## Tests

```bash
cd go && go test ./...
```

Standard library `testing` package — no third-party test framework. Tests use `httptest` for HTTP handler tests. See `go/internal/api/server_test.go` for the pattern.

## Prepared Statements

All database queries use prepared statements registered in `go/internal/db/db.go`. Handlers reference statements by name (e.g., `pool.QueryRow(ctx, "api_player_profile", id, sport)`), not raw SQL. When adding a new query, register it in `registerPreparedStatements()`.

## Handler Pattern

Every handler follows the same shape:

1. Parse and validate request params
2. Build cache key, check cache (return early on hit or ETag match)
3. Call a prepared statement that returns JSON bytes
4. Store in cache, write response with `respond.WriteJSON()`

For third-party data (news, twitter): use `respond.WriteJSONObject()` instead since the data is marshaled from Go structs, not passed through from Postgres.

## Adding a New Provider

1. Create a package under `go/internal/provider/yourprovider/`
2. Implement functions that return `provider.Team`, `provider.Player`, `provider.PlayerStats`, `provider.TeamStats`
3. Create a seed runner in `go/internal/seed/`
4. Add a Cobra subcommand in `go/cmd/ingest/main.go`

The canonical structs in `go/internal/provider/canonical.go` are the contract. The upsert functions in `go/internal/seed/upsert.go` handle writing to Postgres.

## Environment Variables

Config loads from env vars via `go/internal/config/config.go`. Priority for DB URL: `NEON_DATABASE_URL_V2` > `DATABASE_URL` > `NEON_DATABASE_URL`.

Required: `DATABASE_URL` (or equivalent), `BALLDONTLIE_API_KEY`, `SPORTMONKS_API_TOKEN`.

See `.env.example` for the full list.

## Progress Docs

After any major edit — new feature, new file/folder, or significant refactor — generate a markdown summary and save it to `progress_docs/`. This is a mandatory step, not optional.

**Filename format:** `YYYY-MM-DD_short-description.md` (e.g., `2026-03-02_add-fixture-worker.md`)

**Required sections** (follow the pattern established in existing progress docs):

```markdown
# Session: <Short Title>
**Date:** YYYY-MM-DD

## Goals
- Bullet list of what the session set out to accomplish

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| ...      | ...       |

## Accomplishments
### Created
- New files/directories and what they do
### Updated
- Existing files that were modified and why
### Cleaned Up (if applicable)
- Anything removed or deprecated

## Quick Reference (if applicable)
- Commands, URLs, or other useful snippets

## File Layout After This Session (if applicable)
- Tree showing relevant structural changes
```

**When to write one:** Any session that introduces a new feature, adds/removes files or folders, changes the schema, adds a new provider, or performs a significant refactor. Do **not** write one for minor bug fixes, typo corrections, or comment-only changes.

## Things to Avoid

- Do not create migration files — edit the sport's schema file in `sql/` directly
- Do not add a service/repository layer between handlers and pgxpool
- Do not compute derived stats or percentiles in Go — that's Postgres's job
- Do not add core data endpoints to the Go API — those belong in PostgREST (the `api` schema)
- Do not modify or reference anything in `legacy_fastapi/` — it is dead code
- Do not introduce a shared Provider interface — use canonical structs instead
- Do not marshal/unmarshal Postgres JSON responses into Go structs — pass raw bytes through
