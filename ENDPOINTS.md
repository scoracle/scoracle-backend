# Scoracle API Endpoints

> Last updated: 2026-04-20

Single public API base URL:

- Production: `https://api.scoracle.com` (Cloudflare Tunnel → self-hosted Go API)
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

## News Endpoints

### `GET /api/v1/news/status`

News provider configuration status. Reports the Google News RSS source only — NewsAPI was removed; RSS is the sole provider.

### `GET /api/v1/news/{entityType}/{entityID}`

Returns news articles for a player or team, pulled from Google News RSS. Matched articles are also **write-through persisted** to the `news_articles` and `news_article_entities` tables, with cross-entity linking against a cached pool of all teams + players for the sport (confidence 1.0 for the queried entity, 0.8 for co-mentioned entities).

Path parameters:
- `entityType` - `player` or `team`
- `entityID` - Entity ID (integer)

Query parameters:
- `sport` (required) - `NBA`, `NFL`, or `FOOTBALL`
- `team` (optional) - Team name for player context
- `limit` (optional, 1-50) - Number of articles to return

## Twitter / X Endpoints

X is used for a user-facing journalist feed and as a real-time context signal for the vibe generator. Tweet data is subject to a hard 24h TTL (ToS compliance — no long-term storage for ML training).

### `GET /api/v1/twitter/status`

Per-sport Twitter list configuration + cache state + daily cost telemetry.

Response includes, per sport:
- `sport`, `list_id`, `configured`, `ttl_seconds`
- `last_fetched_at`, `since_id`, `last_error`, `last_error_at`
- `calls_today` - X API calls made today (resets 00:00 UTC)
- `tweets_today` - tweets returned from those calls today

Plus service-level fields:
- `bearer_token_configured`, `cache_ttl_seconds`
- `rate_limit` - human-readable limit string
- `architecture: "lazy_cache"`

### `GET /api/v1/{sport}/twitter/feed`

Cached journalist tweets for a sport. Refreshes on demand from X when the cache is older than the list TTL (default 20 min). Concurrent refreshes are coalesced via singleflight; stale cache is served if the upstream call fails.

Path parameters:
- `sport` - `nba`, `nfl`, or `football`

Query parameters:
- `limit` (optional, 1-100, default 25)

### `GET /api/v1/{sport}/twitter/{entityType}/{id}`

Cached tweets linked to a specific player or team via the shared `search_aliases` matcher.

Path parameters:
- `sport` - `nba`, `nfl`, or `football`
- `entityType` - `player` or `team`
- `id` - Entity ID

Query parameters:
- `limit` (optional, 1-100, default 25)

## Vibe (Gemma Blurb) Endpoints

Vibe blurbs are ~140-character narrative summaries produced by a local Ollama-hosted Gemma 4 e4b model, informed by recent news (last 72h) and recent tweets (last 24h) for the entity. Blurbs are NOT generated on request — they land via the vibe CLI (`go/cmd/vibe`) or the milestone listener worker (fires on `percentile_changed` events for `tier=headliner` entities crossing the 90th percentile, with a 30-min per-entity debounce). These endpoints serve what has already been generated.

### `GET /api/v1/{sport}/vibe/{entityType}/{id}`

Returns the most recent vibe blurb for the entity. Returns **404** (not a 200 with empty data) when no blurb has been generated yet — frontends should handle that as "no blurb to show" rather than as an error state.

Path parameters:
- `sport` - `nba`, `nfl`, or `football`
- `entityType` - `player` or `team`
- `id` - Entity ID (integer)

Response example:
```json
{
  "id": 6,
  "entity_type": "player",
  "entity_id": 115,
  "sport": "NBA",
  "trigger_type": "manual",
  "trigger_payload": {"stat_key": "pts", "new_percentile": 97.2},
  "blurb": "From play-in losses and knee drama, Curry keeps the vibes going with jokes and the anticipation of a massive comeback.",
  "model_version": "gemma4:e4b",
  "prompt_version": "v1",
  "generated_at": "2026-04-19T11:51:45.006058-04:00"
}
```

`trigger_type` values: `milestone` (listener-driven), `manual` (CLI ad-hoc), `periodic` (nightly batch).

`trigger_payload` is JSONB — populated for milestone triggers (stat_key + percentile movement), often `null` for manual / periodic runs.

### `GET /api/v1/{sport}/vibe/{entityType}/{id}/history`

Returns the N most recent vibe blurbs for the entity, newest first.

Path parameters: same as above.

Query parameters:
- `limit` (optional, 1-50, default 10)

Response:
```json
{
  "entity_type": "player",
  "entity_id": 115,
  "sport": "NBA",
  "count": 3,
  "vibes": [ /* array of full vibe objects as above */ ]
}
```

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
- Default data endpoints: 5 minutes
- News: 10 minutes (in addition to permanent write-through to `news_articles`)
- Twitter cache: per-sport, default 20 minutes (configurable via `TWITTER_CACHE_TTL_SECONDS`)
- Tweet retention: hard 24h TTL enforced by the maintenance ticker (ToS)

## Data Retention Summary

Different consumers have different persistence rules:

| Source | Storage | Retention |
|---|---|---|
| `news_articles` (Google RSS) | Permanent, for training + RAG corpus | No TTL — grows indefinitely |
| `news_article_entities` | Same | Cascade on article delete |
| `tweets` + `tweet_entities` | Inference cache only | 24h hard TTL |
| `vibe_scores` | Permanent, with `model_version` + `prompt_version` | No TTL |

## Entity Tiering

`players.tier` and `teams.tier` enum values drive vibe-generation scheduling:

| Tier | Description | Real-time vibe? | Daily batch vibe? |
|---|---|---|---|
| `headliner` | Top 150 starters per sport + all teams | ✅ on milestone | ✅ covered |
| `starter` | Regular contributors below top-150 | ❌ | ✅ (if played in last 24h) |
| `bench` | Played at some point but below starter bar | ❌ | ❌ |
| `inactive` | No box scores this season | ❌ | ❌ |

Recompute weekly via `SELECT * FROM recompute_entity_tiers('NBA', 2025);` (and equivalents for NFL / FOOTBALL). Real-time path also requires `new_percentile >= 90` + 30-min per-entity debounce.

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
- News handler: `go/internal/api/handler/news.go`
- Twitter handler: `go/internal/api/handler/twitter.go`
- Vibe handlers: `go/internal/api/handler/vibe.go`
- News write-through + entity pool: `go/internal/thirdparty/news.go`
- Twitter service + telemetry: `go/internal/thirdparty/twitter.go`
- Ollama client + vibe generator: `go/internal/ml/ollama.go`, `go/internal/ml/vibe.go`
- Listener (vibe dispatch): `go/internal/listener/listener.go`, `go/internal/listener/vibe_worker.go`
- Maintenance tickers (tweet TTL purge, etc.): `go/internal/maintenance/maintenance.go`
- Prepared statements: `go/internal/db/db.go`
- Cache/ETag implementation: `go/internal/cache/cache.go`
- Swagger docs: `go/docs/swagger.json`, `go/docs/swagger.yaml`

---

**Note:** Legacy endpoints (`/players/`, `/teams/`, `/standings/`, `/leaders/`, `/search/`, `/autofill/`, `/similarity/`) have been removed. Use the canonical `/api/v1/{sport}/{entityType}/{id}` routes. Comparison-style features should live on the frontend using data from the profile + meta endpoints.
