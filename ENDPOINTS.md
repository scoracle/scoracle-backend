# Scoracle API Endpoints

> Last updated: 2026-03-28

Single public API base URL:

- Production: `https://scoracle-data-production.up.railway.app`
- Local: `http://localhost:8000`

## Core Data Endpoints (Canonical)

All canonical endpoints are sport-scoped.

Supported sport path values:

- `nba`
- `nfl`
- `football`

Supported canonical `entityType` values:

- `player`
- `team`

### `GET /api/v1/{sport}/{entityType}/{id}`

Returns the canonical profile payload for a sport entity.

Query params:

- `season` (optional integer)
- `league_id` (optional integer)

### `GET /api/v1/{sport}/meta`

Returns complete metadata payload for frontend local DB hydration (autofill + meta widget).

Query params:

- `league_id` (optional integer)

### `GET /api/v1/{sport}/health`

Returns sport-level data freshness and counts.

Query params:

- `league_id` (optional integer)

## League-Scoped Endpoints

League-scoped routes are especially important for football and are preferred when league context is explicit.

### `GET /api/v1/{sport}/leagues/{leagueId}/{entityType}/{id}`

Returns profile payload scoped to a specific league.

Query params:

- `season` (optional integer)

### `GET /api/v1/{sport}/leagues/{leagueId}/meta`

Returns metadata payload scoped to a specific league.

### `GET /api/v1/{sport}/leagues/{leagueId}/health`

Returns health/freshness payload scoped to a specific league.

## Breaking Changes (2026-03-28)

Legacy sport data routes were removed in the clean-break migration.

Removed routes:

- `GET /api/v1/{sport}/players/{id}`
- `GET /api/v1/{sport}/teams/{id}`
- `GET /api/v1/{sport}/standings`
- `GET /api/v1/{sport}/leaders`
- `GET /api/v1/{sport}/search`
- `GET /api/v1/{sport}/autofill`
- `GET /api/v1/{sport}/stat-definitions`
- `GET /api/v1/football/leagues`

Use the canonical route family in this document instead.

## Integrations Endpoints

### `GET /api/v1/news/status`

News provider configuration status.

### `GET /api/v1/news/{entityType}/{entityID}`

News articles for a player/team.

Path params:

- `entityType`: `player` or `team`
- `entityID`: integer

Query params:

- `sport` (required: `NBA`, `NFL`, `FOOTBALL`)
- `team` (optional)
- `limit` (optional 1-50)
- `source` (optional: `rss`, `api`, `both`)

### `GET /api/v1/twitter/status`

Twitter integration status.

### `GET /api/v1/twitter/journalist-feed`

Curated journalist-feed search.

Query params:

- `q` (required)
- `sport` (optional)
- `limit` (optional 1-50)

## Operational Endpoints

- `GET /`
- `GET /health`
- `GET /health/db`
- `GET /health/cache`
- `GET /docs/`

## Response & Cache Conventions

- JSON responses include ETags where applicable.
- Send `If-None-Match` to receive `304 Not Modified`.
- `X-Cache` indicates `HIT` or `MISS` for cache-backed endpoints.

Data endpoint cache policy:

- Default data TTL: 5 minutes (`TTLData`).
- News endpoint TTL: 10 minutes (`TTLNews`).
- Twitter journalist feed uses provider cache metadata and endpoint-specific cache headers.

## Backend Implementation Map

- Router: `go/internal/api/server.go`
- Data handlers: `go/internal/api/handler/data.go`
- Integrations handlers: `go/internal/api/handler/news.go`, `go/internal/api/handler/twitter.go`
- Prepared statements: `go/internal/db/db.go`
- Cache/ETag implementation: `go/internal/cache/cache.go`
- Swagger docs: `go/docs/swagger.json`, `go/docs/swagger.yaml`

## Error Shape

```json
{
  "error": {
    "code": "INVALID_QUERY_PARAM",
    "message": "season must be an integer",
    "detail": "optional"
  }
}
```
