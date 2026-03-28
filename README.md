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

Canonical data routes are sport-scoped:

- `GET /api/v1/{sport}/{entityType}/{id}` (profile)
- `GET /api/v1/{sport}/meta`
- `GET /api/v1/{sport}/health`

League-scoped variants (preferred for multi-league precision):

- `GET /api/v1/{sport}/leagues/{leagueId}/{entityType}/{id}`
- `GET /api/v1/{sport}/leagues/{leagueId}/meta`
- `GET /api/v1/{sport}/leagues/{leagueId}/health`

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
- Prepared statements for canonical payloads are registered in `go/internal/db/db.go` and return final JSON documents for frontend widgets.
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
scoracle-seed load-fixtures nba --season 2025 --from-date 2025-10-01 --to-date 2025-10-31
scoracle-seed process --max 50
scoracle-seed backfill nba --season 2024 --from-date 2024-10-22 --to-date 2025-04-13 --max 200
scoracle-seed percentiles --sport NBA --season 2025 --required-stat pts --required-stat ast
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
