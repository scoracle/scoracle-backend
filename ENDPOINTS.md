# Scoracle API Endpoints

> Last updated: 2026-03-22

Single public API base URL:

- Production: `https://scoracle-data-production.up.railway.app`
- Local: `http://localhost:8000`

## Core Data Endpoints

All core endpoints are page-shaped and sport-scoped.

Supported sport path values:

- `nba`
- `nfl`
- `football`

### `GET /api/v1/{sport}/players/{id}`

Returns a curated player page payload.

Query params:

- `season` (optional integer)
- `league_id` (optional integer, football only)

### `GET /api/v1/{sport}/teams/{id}`

Returns a curated team page payload.

Query params:

- `season` (optional integer)
- `league_id` (optional integer, football only)

### `GET /api/v1/{sport}/standings`

Returns standings page payload.

Query params:

- `season` (required integer)
- `conference` (optional string, NBA/NFL)
- `division` (optional string, NBA/NFL)
- `league_id` (optional integer, football)

### `GET /api/v1/{sport}/leaders`

Returns stat leaders page payload.

Query params:

- `season` (required integer)
- `stat` (required string)
- `limit` (optional integer, 1-100, default 25)
- `position` (optional string)
- `league_id` (required for football)

### `GET /api/v1/{sport}/search`

Returns search/autofill page payload (filtered by query).

Query params:

- `q` (required string, 1-100 chars)

### `GET /api/v1/{sport}/autofill`

Returns complete autofill database for a sport with full entity metadata.

This endpoint returns all players and teams with their complete profile data (excluding performance statistics), optimized for frontend caching and instant meta widget rendering.

**Response includes:**
- All entity IDs and names
- Profile data: nationality, DOB, height, weight, photo URLs
- Team affiliations and league context
- `search_tokens` array for frontend fuzzy search
- Filtered `meta` object with display-worthy fields

**Caching:** No server-side caching. Frontend should cache at build time.

### `GET /api/v1/{sport}/stat-definitions`

Returns stat-definition page payload.

Query params:

- `entity_type` (optional: `player`, `team`)

### `GET /api/v1/football/leagues`

Returns football leagues page payload.

Query params:

- `active` (optional boolean)
- `benchmark` (optional boolean)

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
