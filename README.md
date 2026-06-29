# Scoracle Data

Backend data pipeline and unified API for Scoracle sports data.

## Architecture

Scoracle runs as a Go API + Rust cognition layer backed by PostgreSQL, plus a Python seeder.

- **Go API (`:8000`)** serves curated sport data pages and health/docs endpoints from precomputed Postgres tables. It runs SQL-only maintenance/notification workers and the ingest funnel wiring, but does not execute model inference.
- **Rust Cognition Harness (`rust/`)** owns all model inference stages (scrub, headlines, transfers, narratives, vibe, sigil) via `pipeline_work`, plus the `statcommentary` rating batch.
- **Python Seeder (`seed/`)** ingests provider data and upserts raw rows to PostgreSQL.
- **PostgreSQL (`sql/`)** is the source of truth for schema, derived stats, percentiles, views, and API-shaping SQL.

> Operating the backend (release/rollback, backup/restore, jobs, durable work queue + repair commands): see **[`RUNBOOK.md`](RUNBOOK.md)**.

The frontend calls one API origin and receives page-shaped JSON payloads designed for direct rendering.

## Service Responsibilities

| Component | Responsibility | Location |
|---|---|---|
| Go API | Public HTTP API, caching, ETags, CORS, rate limiting, mobile auth, SQL-only maintenance + notifications | `go/` |
| Rust Cognition Harness | Queue-stage model inference + rating batch (`statcommentary`) | `rust/` |
| Python Seeder | Provider ingestion and fixture processing | `seed/` |
| PostgreSQL | Data model, stat normalization, derived metrics, percentile logic, shaping views/functions | `sql/` |

## API Surface

Canonical data routes are sport-scoped (`{sport}` ∈ `nba|nfl|football`). The page is
assembled from **per-product card endpoints** — the bundled all-in-one profile route
was removed (O16). The two data **sources** (stats, news) refine into end products that
converge into the **Sigil**. Route shape is authoritative in `go/internal/api/server.go`.

Per-entity products (`{entityType}` ∈ `player|team`):

- **stats source:**
  - `GET /api/v1/{sport}/{entityType}/{id}/stats` — season Composite rating (breakdown, modes, fantasy, scoped ranks) + `available_seasons` + the per-event series
  - `GET /api/v1/{sport}/{entityType}/{id}/rating` — model-divined statistical read + the stat commentary (`stat_summaries`)
- **news source:**
  - `GET /api/v1/{sport}/{entityType}/{id}/news` — latest model narratives, hottest first by impact (`news_summaries`)
  - `GET /api/v1/{sport}/{entityType}/{id}/transfers` — vetted transfer/trade rumors by heat — team→players, player→clubs (`transfer_rumors`)
- **convergence:**
  - `GET /api/v1/{sport}/{entityType}/{id}/momentum` — Rating-trajectory × Vibe-trajectory (stats trend + narrative trend)
  - `GET /api/v1/{sport}/{entityType}/{id}/sigil` — the Sigil crown synthesis (Rating + Vibe + Momentum → `sigil_synthesis`)
- `GET /api/v1/{sport}/{entityType}/{id}/meta` — per-entity identity (page header); 404 when the entity is unknown
- `GET /api/v1/{sport}/team/{id}/results` — a team's finalized scorelines for a season
- `GET /api/v1/{sport}/team/{id}/roster` — the rating board narrowed to one team

> **Convergence rename (O14):** the earlier per-product names `/special`, `/trends`, and
> per-entity `/vibes` are **gone** — `/special` folded into `/rating`, `/trends` became
> `/momentum`, and per-entity `/vibes` became `/sigil` (the crown). The bundled `/news`
> rail and `/sparkline`/`/starline` were retired earlier (2026-06-15).

Sport-level + leaderboard routes:

- `GET /api/v1/{sport}/meta` (search index — being repointed to `/autofill`), `GET /api/v1/{sport}/autofill`, `GET /api/v1/{sport}/health`
- `GET /api/v1/{sport}/leaderboard` (rating board — `entity_type=player|team`, `scope=composite|specialist|<skill>`; also `?board=rating|vibes|sigil|news|transfers`)
- `GET /api/v1/{sport}/leaderboard/vibes` — sport-wide Vibe board (latest sentiment 1-100)
- `GET /api/v1/{sport}/leaderboard/sigil` — sport-wide Sigil crown board (+ `previous_score` delta)
- `GET /api/v1/{sport}/leaderboard/news` — hottest model narratives by per-narrative impact
- `GET /api/v1/{sport}/leaderboard/transfers` — model-vetted rumors by heat 0-100
- `GET /api/v1/{sport}/leaderboard/trending` — vibe & rating risers

League-scoped variants (preferred for multi-league precision):

- `GET /api/v1/{sport}/leagues/{leagueId}/{entityType}/{id}/momentum`
- `GET /api/v1/{sport}/leagues/{leagueId}/team/{id}/results`
- `GET /api/v1/{sport}/leagues/{leagueId}/meta`
- `GET /api/v1/{sport}/leagues/{leagueId}/health`

Operational + mobile-auth routes:

- `GET /`, `GET /health`, `GET /health/db`, `GET /health/cache`
- `GET /docs/`, `GET /docs/go.json` (Swagger UI + spec)
- `POST /api/v1/auth/device`, `POST /api/v1/auth/refresh` (public); `POST /api/v1/auth/device/push`, `POST /api/v1/auth/logout` (bearer)

> The live `/api/v1/news/*` and `/api/v1/twitter/*` integration routes were **removed**
> (O12/O13); X was decommissioned (O15). They are no longer wired in `server.go`.

See `ENDPOINTS.md` for full contract details.

## Implementation Notes

- Core data handlers live in `go/internal/api/handler/data.go` and follow a strict thin pattern (validate -> cache -> prepared statement -> passthrough JSON).
- Prepared statements for canonical payloads are registered in `go/internal/db/db.go` and return final JSON documents for frontend widgets.
- Sport routes are constrained to `nba`, `nfl`, and `football` at the router level.
- Data endpoints use in-memory caching with ETag support (`TTLData=5m`).
- Background workers in the API process are SQL-only (maintenance/news-scrub enqueue + notifications/listener) and are not on the serving path. Model inference runs in Rust (`scoracle-cognition` + `statcommentary`). See `RUNBOOK.md`.

## Repository Layout

```text
scoracle-backend/
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
cp .env .env.local  # fill in real values
docker compose up --build
docker compose run --rm seed event process --max 50
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

scoracle-seed event load-fixtures nba --season 2025 --from-date 2025-10-01 --to-date 2025-10-31
scoracle-seed event process --sport nba --season 2025 --max 50
scoracle-seed meta seed nba --season 2025
```

## Testing

```bash
cd go && go test ./...
cd go && go build -o bin/scoracle-api ./cmd/api
```

## Environment Variables

See `.env` (committed template) and copy to `.env.local` (gitignored) for real values.
DB URL priority (per `go/internal/config/config.go`): `DATABASE_PRIVATE_URL` > `DATABASE_URL`.

Required for local operation:

- `DATABASE_PRIVATE_URL` (or `DATABASE_URL`)
- `BALLDONTLIE_API_KEY` (seeder, NBA/NFL)
- `SPORTMONKS_API_TOKEN` (seeder, football)

Common optional (full list + defaults in `config.go`):

- `API_PORT`/`PORT`, `CACHE_ENABLED`, `RATE_LIMIT_ENABLED`
- `DB_POOL_MAX_CONNS` (default `25`), `DB_POOL_MIN_CONNS`, `DB_POOL_MAX_LIFE_MINUTES`
- `CORS_ALLOW_ORIGINS`, `CORS_PRODUCTION_ORIGINS`
- Rust cognition (read by Rust config, not Go): `OLLAMA_BASE_URL`, `OLLAMA_MODEL` (default `mistral:7b`), `OLLAMA_TIMEOUT_SECONDS`, `OLLAMA_MAX_CONCURRENT`
- Go workers: `NEWS_SCRUB_ENABLED`, `PIPELINE_STATS_INTERVAL_MINUTES`
- Mobile auth: `JWT_SECRET` (unset ⇒ `/auth/*` returns 503), `JWT_ACCESS_TTL_MINUTES`, `JWT_REFRESH_TTL_DAYS`
- `FIREBASE_CREDENTIALS_FILE`; seeder third key `API_SPORTS_KEY`

> X/Twitter was permanently decommissioned (O15, 2026-06-19) — there are **no** `TWITTER_*` env vars anymore.

## Trademarks & Nominative Fair Use

Team names, logos, and other identifying marks displayed by Scoracle are the property of their respective owners (leagues, teams, and affiliated entities). These marks are used solely to identify the teams and players whose statistical data is presented — not to imply any official sponsorship, endorsement, or affiliation between Scoracle and any league, team, or player.

This usage satisfies the three-part test for nominative fair use:

1. The teams and leagues cannot reasonably be identified without reference to their marks.
2. Only as much of each mark is used as necessary for identification.
3. Nothing in the presentation suggests official sponsorship or endorsement by the mark holder.

Scoracle is not affiliated with, endorsed by, or in any way officially connected to the NBA, NFL, the Premier League, La Liga, Bundesliga, Serie A, Ligue 1, or any of their member teams and clubs.

## License & Copyright

Copyright (c) 2026 Scoracle. All rights reserved.

This repository and its contents — including but not limited to source code, database schemas, API designs, data pipeline architecture, and documentation — are proprietary and confidential. No part of this repository may be reproduced, distributed, transmitted, or otherwise used in any form without the prior written permission of the copyright holder.

Unauthorized use, copying, modification, or distribution of any materials in this repository is strictly prohibited and may result in legal action.
