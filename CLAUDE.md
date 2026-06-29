# Scoracle Data

Backend data pipeline and unified API for the Scoracle sports platform.

## Session start: confirm branch is synced — ALWAYS step 1

**Before any editing, before any tool call beyond inspection, confirm the local branch is synced with `origin/main`.** Run `git fetch && git status`; if uncertain about divergence compare `git log origin/main..HEAD` (local-only) and `git log HEAD..origin/main` (remote-only). If the branch is behind — even by a single commit — `git pull --ff-only` before editing; if it has genuinely diverged, surface it to the user and agree a plan first.

Why this is non-negotiable: scoracle is a solo, multi-machine project (**archx220** + **archbox**), and work is pushed to `origin` from whichever machine made it. Starting on a stale baseline burns time on duplicate or conflicting work — e.g. re-designing a feature that was already shipped on `origin/main`. A `SessionStart` git-sync hook in `.claude/settings.json` runs this check automatically, but treat the hook as a backstop, not a substitute for the habit.

**End the session synced, too.** Before stopping, commit and push **the work *this session* did** — stage the files this session touched (`git add <paths>`), never a blanket `git add -A` that sweeps up unrelated or another machine's in-flight WIP. Leaving your own finished work (or an applied-but-uncommitted migration) unpushed is precisely the stale baseline the next session trips over; pre-existing changes that aren't yours stay untouched.

## Architecture — Three Components, One Database

```
Frontend (Solid)
    └── Curated sport pages + integrations ──► Go API (:8000)
                                              │
                                   Connects to PostgreSQL ◄── Rust Cognition Harness
                                              ▲
                                              │
                                   Python Seeder (ingestion)
```

### Go API — Unified Public API (port 8000)

The Go API owns all public HTTP endpoints. **We own all the data** — every serving endpoint is a
precomputed read from our own Postgres; there is **no third-party call on a serving request** (the
Google-RSS compile lives in the background pipeline, off the request path).

- Sport data endpoints (canonical profile + the per-product card endpoints, meta, health, league-scoped variants)
- Derived-product endpoints — `/news` (model narratives), `/transfers`, `/rating`, `/momentum`, `/sigil` (the synthesis) — all served from precomputed tables
- Health/docs endpoints
- Background workers — SQL-only maintenance/enqueue + notifications/LISTEN workers; model inference runs in Rust (`scoracle-cognition` daemon + `statcommentary` batch)

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

Verify route shape against `go/internal/api/server.go` (the only source of truth) —
the bundled all-in-one profile route `/{sport}/{entityType}/{id}` was **removed** (O16);
the page is now assembled from per-product card endpoints (the "two rails → convergence"
model). The convergence rename (O14) settled the per-entity product names below — `/vibes`,
`/trends`, and `/special` are **gone** (renamed/folded), not current.

Per-entity card endpoints (`{entityType}` ∈ `player|team`):

- `/{sport}/{entityType}/{id}/stats` — season Composite rating + per-event series + `available_seasons` + `stat_definitions`
- `/{sport}/{entityType}/{id}/rating` — model-divined statistical read + the stat commentary (`stat_summaries`)
- `/{sport}/{entityType}/{id}/momentum` — Rating-trajectory × Vibe-trajectory (stats trend + narrative trend)
- `/{sport}/{entityType}/{id}/sigil` — the Sigil crown synthesis (Rating + Vibe + Momentum → `sigil_synthesis`)
- `/{sport}/{entityType}/{id}/news` — model narratives (`news_summaries`)
- `/{sport}/{entityType}/{id}/transfers` — vetted transfer/trade rumor heat (`transfer_rumors`)
- `/{sport}/{entityType}/{id}/meta` — per-entity identity (page header); 404 when the entity is unknown
- `/{sport}/team/{id}/results`, `/{sport}/team/{id}/roster`

Sport-level + board routes:

- `/{sport}/meta` (search index — being repointed to `/{sport}/autofill`), `/{sport}/autofill`, `/{sport}/health`
- `/{sport}/leaderboard` (+ `?board=rating|vibes|sigil|news|transfers`), and the dedicated `/{sport}/leaderboard/{vibes,sigil,news,transfers,trending}`
- `/{sport}/leagues/{leagueId}/...` (league-scoped variants of momentum, results, meta, health)
- Mobile auth: `/api/v1/auth/{device,refresh,device/push,logout}`

Removed integration routes (we own all data now — **gone from the router**, kept here so docs match reality):

- `/api/v1/news/...` — live Google-RSS lookup; **removed** (O12). The eager News card reads the precomputed `/{sport}/{type}/{id}/news` narratives. (The Google-RSS *compile* still runs in the background pipeline, off the request path.)
- `/api/v1/twitter/...` — **removed** (O13); X was permanently decommissioned (O15, 2026-06-19) — client, routes, env, and tweet tables are all gone.

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

### Migrations & fresh environments

Migrations in `sql/migrations/` are tracked in `public.schema_migrations` (bootstrapped by
migration `051`). Apply pending ones with the runner — idempotent, ordered, records each:

```bash
DATABASE_PRIVATE_URL=… ./sql/migrate.sh
```

Stand up a fresh env (sandbox/fantasy.scoracle, dev) by **cloning the prod schema**, not by
replaying migrations (canonical `sql/*.sql` is the BASE only; the rating/fantasy engine lives
in migrations, several of which have data-dependent gates that fail on an empty DB):

```bash
./sql/build.sh "$PROD_URL" "$NEW_ENV_URL"
```

Apply a migration **before** restarting the Go API: `db.New` prepares every statement at boot
(validating columns + functions), so a restart against a drifted schema fails fast instead of
serving degraded. Full conventions in `sql/README-migrations.md`.

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

Common optional (full list + defaults in `go/internal/config/config.go`):

- `API_PORT` / `PORT`, `CACHE_ENABLED`, `RATE_LIMIT_ENABLED` (+ `RATE_LIMIT_REQUESTS`, `RATE_LIMIT_WINDOW`)
- `DB_POOL_MAX_CONNS` (default `25` — sized for the eager profile fan-out of ~6–9 concurrent reads), `DB_POOL_MIN_CONNS`, `DB_POOL_MAX_LIFE_MINUTES`
- `CORS_ALLOW_ORIGINS`, `CORS_PRODUCTION_ORIGINS` (the latter merged in only when `ENVIRONMENT=production`)
- **Rust cognition:** `OLLAMA_BASE_URL`, `OLLAMA_MODEL` (default `mistral:7b`), `OLLAMA_TIMEOUT_SECONDS`, `OLLAMA_MAX_CONCURRENT`, plus `COGNITION_*` stage/router controls in `rust/src/config.rs`
- **Go maintenance ticker:** `NEWS_SCRUB_ENABLED` (master switch; cadence/batch are code defaults in `internal/maintenance`)
- `PIPELINE_STATS_INTERVAL_MINUTES` (1440; 0 disables the daily corpus snapshot)
- **Mobile auth:** `JWT_SECRET` (unset ⇒ `/auth/*` returns 503; rest of API unaffected), `JWT_ACCESS_TTL_MINUTES` (30), `JWT_REFRESH_TTL_DAYS` (90)
- `FIREBASE_CREDENTIALS_FILE`
- Seeder-only third key: `API_SPORTS_KEY`

> X/Twitter was permanently decommissioned (O15, 2026-06-19): there are **no** `TWITTER_*` env vars in `config.go` — the client, routes, env, and tweet tables are all gone.

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
