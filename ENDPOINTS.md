# Scoracle API Endpoints

> Last updated: 2026-03-30

Single public API base URL:

- Production: `https://scoracle-data-production.up.railway.app`
- Local: `http://localhost:8000`

## Core Data Endpoints (Canonical)

All canonical endpoints are sport-scoped under `/api/v1/{sport}`.

Supported sport path values:
- `nba`
- `nfl`  
- `football`

Supported entity type values:
- `player`
- `team`

### `GET /api/v1/{sport}/{entityType}/{id}`

Returns the canonical profile payload for a sport entity (player or team).

Path parameters:
- `sport` - Sport identifier (`nba`, `nfl`, `football`)
- `entityType` - Entity type (`player` or `team`)
- `id` - Entity ID (integer)

Query parameters:
- `season` (optional integer) - Filter by season year
- `league_id` (optional integer) - Filter by league

Response includes:
- Entity profile (name, position, team, etc.)
- Aggregated season stats
- Percentile rankings
- Metadata (sample size, position group)

### `GET /api/v1/{sport}/meta`

Returns complete metadata payload for frontend local DB hydration. Designed for caching on the frontend to enable instant autocomplete without repeated API calls.

Query parameters:
- `league_id` (optional integer) - Scope to specific league

Response includes:
- `meta_version` - Unix timestamp of last data update (for cache invalidation)
- `current_season` - The sport's current active season year
- `total_entities` - Count of players and teams in the response
- `items` - All entities (players + teams) with search tokens and metadata
- `stat_definitions` - All stat keys with display names and categories
- `leagues` - League information (populated for multi-league sports like football)

### `GET /api/v1/{sport}/health`

Returns sport-level data freshness and counts.

Query parameters:
- `league_id` (optional integer) - Scope to specific league

Response includes:
- Last update timestamp
- Fixture counts
- Box score coverage stats
- Data freshness indicators

## League-Scoped Endpoints

League-scoped routes are required for football (which has multiple leagues) and preferred when league context is explicit.

### `GET /api/v1/{sport}/leagues/{leagueId}/{entityType}/{id}`

Returns profile payload scoped to a specific league.

Path parameters:
- `leagueId` - League identifier (e.g., 8 for Premier League)
- `entityType` - `player` or `team`
- `id` - Entity ID

Query parameters:
- `season` (optional integer)

### `GET /api/v1/{sport}/leagues/{leagueId}/meta`

Returns metadata payload scoped to a specific league.

### `GET /api/v1/{sport}/leagues/{leagueId}/health`

Returns health/freshness payload scoped to a specific league.

## Integrations Endpoints

### `GET /api/v1/news/status`

News provider configuration status.

### `GET /api/v1/news/{entityType}/{entityID}`

News articles for a player or team.

Path parameters:
- `entityType` - `player` or `team`
- `entityID` - Entity ID (integer)

Query parameters:
- `sport` (required) - `NBA`, `NFL`, or `FOOTBALL`
- `team` (optional) - Team name/ID for filtering
- `limit` (optional, 1-50) - Number of articles to return
- `source` (optional) - `rss`, `api`, or `both`

### `GET /api/v1/twitter/status`

Twitter integration status.

### `GET /api/v1/twitter/journalist-feed`

Curated journalist-feed search.

Query parameters:
- `q` (required) - Search query
- `sport` (optional) - Sport filter
- `limit` (optional, 1-50) - Number of tweets to return

## Operational Endpoints

- `GET /` - Root endpoint (API info)
- `GET /health` - General health check
- `GET /health/db` - Database connectivity check
- `GET /health/cache` - Cache health check
- `GET /docs/` - Swagger UI documentation
- `GET /docs/go.json` - OpenAPI/Swagger JSON spec

## Response Structure

### Profile Response Example (Player)

```json
{
  "page": "profile",
  "sport": "nba",
  "entity_type": "player",
  "entity": {
    "id": 666609,
    "name": "Rui Hachimura",
    "first_name": "Rui",
    "last_name": "Hachimura",
    "position": "F",
    "nationality": "Japan",
    "height": "6-8",
    "weight": "230",
    "team": {
      "id": 14,
      "name": "Lakers",
      "abbreviation": "LAL",
      "city": "Los Angeles",
      "conference": "West",
      "division": "Pacific"
    },
    "season": 2025
  },
  "stats": {
    "pts": 18.0,
    "reb": 5.0,
    "ast": 1.0,
    "games_played": 1
  },
  "percentiles": {
    "pts": 100.0,
    "reb": 50.0,
    "ast": 0.0
  },
  "percentile_metadata": {
    "position_group": "F",
    "sample_size": 13
  },
  "meta": {
    "season": 2025,
    "league_id": null
  }
}
```

### Profile Response Example (Team)

```json
{
  "page": "profile",
  "sport": "nba",
  "entity_type": "team",
  "entity": {
    "id": 18,
    "name": "Timberwolves",
    "short_code": "MIN",
    "city": "Minnesota",
    "conference": "West",
    "division": "Northwest",
    "season": 2025
  },
  "stats": {
    "wins": 0,
    "losses": 1,
    "pts": 103.0,
    "games_played": 1
  },
  "percentiles": {
    "wins": 0.0,
    "pts": 0.0
  },
  "stat_definitions": [...]
}
```

### Meta Response Example

```json
{
  "page": "meta",
  "sport": "nba",
  "scope": {
    "league_id": null
  },
  "meta_version": "1743772800",
  "generated_at": "2026-04-04T16:00:00Z",
  "current_season": 2025,
  "total_entities": 524,
  "items": [
    {
      "id": 666609,
      "type": "player",
      "name": "Rui Hachimura",
      "first_name": "Rui",
      "last_name": "Hachimura",
      "position": "F",
      "nationality": "Japan",
      "date_of_birth": "1998-02-08",
      "height": "6-8",
      "weight": "230",
      "photo_url": "https://...",
      "team_id": 14,
      "team_abbr": "LAL",
      "team_name": "Lakers",
      "search_tokens": ["rui", "hachimura", "ruihachimura", "lal", "lakers"],
      "meta": {
        "display_name": "Rui Hachimura",
        "jersey_number": "28",
        "draft_year": 2019,
        "draft_pick": 9,
        "years_pro": 6,
        "college": "Gonzaga"
      }
    }
  ],
  "stat_definitions": [
    {
      "id": 1,
      "key_name": "pts",
      "display_name": "Points Per Game",
      "entity_type": "player",
      "category": "scoring",
      "is_inverse": false,
      "is_derived": false,
      "is_percentile_eligible": true,
      "sort_order": 3
    }
  ],
  "leagues": []
}
```

**Frontend Caching Strategy:**

Store `meta_version` locally and send it on subsequent requests:

```javascript
const response = await fetch('/api/v1/nba/meta', {
  headers: {
    'If-None-Match': localStorage.getItem('nba_meta_version')
  }
});

if (response.status === 304) {
  // Use cached data
} else {
  const data = await response.json();
  localStorage.setItem('nba_meta_version', data.meta_version);
  // Store data.items, data.stat_definitions for local search
}
```

## Response & Cache Conventions

- JSON responses include ETags where applicable
- Send `If-None-Match` header to receive `304 Not Modified`
- `X-Cache` header indicates `HIT` or `MISS` for cache-backed endpoints
- `X-Process-Time` header shows request processing time

Cache TTL:
- Default data: 5 minutes
- News: 10 minutes
- Twitter: Uses provider cache metadata

## Football League IDs

When using league-scoped endpoints for football:

| League | ID |
|--------|-----|
| Premier League (England) | 8 |
| Bundesliga (Germany) | 82 |
| Ligue 1 (France) | 301 |
| Serie A (Italy) | 384 |
| La Liga (Spain) | 564 |

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

## Backend Implementation Map

- Router: `go/internal/api/server.go`
- Data handlers: `go/internal/api/handler/data.go`
- Integrations handlers: `go/internal/api/handler/news.go`, `go/internal/api/handler/twitter.go`
- Prepared statements: `go/internal/db/db.go`
- Cache/ETag implementation: `go/internal/cache/cache.go`
- Swagger docs: `go/docs/swagger.json`, `go/docs/swagger.yaml`

---

**Note:** Legacy endpoints (`/players/`, `/teams/`, `/standings/`, `/leaders/`, `/search/`, `/autofill/`) have been consolidated into the canonical endpoint structure above. Use `/{entityType}/` routes instead.
