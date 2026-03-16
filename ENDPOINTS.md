# Scoracle API Endpoints

> **Last updated:** 2026-03-16
>
> This document is the single source of truth for all API endpoints consumed by the frontend.
> Update it whenever endpoints are added, changed, or removed.

## Service Map

| Service   | Purpose                          | Production Base URL                                        | Local URL               |
|-----------|----------------------------------|------------------------------------------------------------|-------------------------|
| PostgREST | Stats, profiles, standings, search | `https://postgrest-production-0650.up.railway.app`        | `http://localhost:3000` |
| Go API    | News, tweets, health, docs        | *(your Go API Railway URL)*                                | `http://localhost:8000` |

---

## PostgREST (Data API)

PostgREST auto-generates a REST API from per-sport Postgres schemas. **Every request must include the `Accept-Profile` header** to select the sport schema.

### Headers

| Header           | Required | Values                        | Purpose                          |
|------------------|----------|-------------------------------|----------------------------------|
| `Accept-Profile` | **Yes**  | `nba`, `nfl`, `football`      | Selects the sport schema         |
| `Authorization`  | No       | `Bearer <JWT>`                | Authenticated access (`web_user` role) |
| `Content-Type`   | No       | `application/json`            | Required for RPC POST bodies     |

### Filtering, Ordering & Pagination

PostgREST supports powerful query parameters on all view endpoints. Full docs: [postgrest.org/en/stable/references/api/tables_views.html](https://postgrest.org/en/stable/references/api/tables_views.html)

| Pattern                        | Example                                      | Description                    |
|--------------------------------|----------------------------------------------|--------------------------------|
| `?column=op.value`             | `?id=eq.123`                                 | Filter by exact value          |
| `?column=ilike.*term*`         | `?name=ilike.*lebron*`                        | Case-insensitive search        |
| `?column=in.(a,b,c)`          | `?conference=in.(East,West)`                  | IN filter                      |
| `?order=column.desc`          | `?order=win_pct.desc`                         | Sort results                   |
| `?limit=N&offset=M`           | `?limit=25&offset=0`                          | Pagination                     |
| `?select=col1,col2`           | `?select=id,name,position`                    | Choose returned columns        |

---

### `GET /players`

Player profiles with embedded team info.

```
GET /players
Accept-Profile: nba
```

**Query examples:**
- `/players?id=eq.123` — single player by ID
- `/players?select=id,name,position,team` — only specific columns

**Response (each item):**

```json
{
  "id": 123,
  "name": "LeBron James",
  "first_name": "LeBron",
  "last_name": "James",
  "position": "F",
  "detailed_position": "SF",
  "nationality": "USA",
  "date_of_birth": "1984-12-30",
  "height": "206",
  "weight": "113",
  "photo_url": "https://...",
  "team_id": 17,
  "league_id": null,
  "meta": {},
  "team": {
    "id": 17,
    "name": "Los Angeles Lakers",
    "abbreviation": "LAL",
    "logo_url": "https://...",
    "country": "USA",
    "city": "Los Angeles",
    "conference": "West",
    "division": "Pacific"
  }
}
```

> **Football note:** Football players also include a `league` object: `{ "id": 8, "name": "Premier League", "country": "England", "logo_url": "..." }`.

---

### `GET /player_stats`

Season statistics with percentiles for a player.

```
GET /player_stats?player_id=eq.123&season=eq.2025
Accept-Profile: nba
```

**Response (each item):**

```json
{
  "player_id": 123,
  "season": 2025,
  "league_id": null,
  "team_id": 17,
  "stats": {
    "pts": 25.7,
    "reb": 7.3,
    "ast": 8.3,
    "pts_per36": 28.1,
    "ts_pct": 0.612
  },
  "percentiles": {
    "pts": 95,
    "reb": 80,
    "ast": 97
  },
  "percentile_metadata": {
    "position_group": "F",
    "sample_size": 245
  },
  "player_name": "LeBron James",
  "position": "F",
  "photo_url": "https://...",
  "team_name": "Los Angeles Lakers",
  "team_abbr": "LAL",
  "team_logo_url": "https://...",
  "updated_at": "2026-03-15T10:30:00Z"
}
```

> **Football note:** Football `player_stats` also includes `league_name`. Filter by league: `?league_id=eq.8`.

---

### `GET /teams`

Team profiles.

```
GET /teams
Accept-Profile: nfl
```

**Query examples:**
- `/teams?id=eq.5` — single team
- `/teams?conference=eq.West` — filter by conference (NBA/NFL)

**Response (each item):**

```json
{
  "id": 5,
  "name": "Kansas City Chiefs",
  "short_code": "KC",
  "logo_url": "https://...",
  "country": "USA",
  "city": "Kansas City",
  "founded": 1960,
  "league_id": null,
  "conference": "AFC",
  "division": "West",
  "venue_name": "Arrowhead Stadium",
  "venue_capacity": 76416,
  "meta": {}
}
```

> **Football note:** Football teams also include a `league` object and `league_id`.

---

### `GET /team_stats`

Season statistics with percentiles for a team.

```
GET /team_stats?team_id=eq.5&season=eq.2025
Accept-Profile: nfl
```

**Response (each item):**

```json
{
  "team_id": 5,
  "season": 2025,
  "league_id": null,
  "stats": {
    "wins": 15,
    "losses": 2,
    "win_pct": 0.882,
    "pts_scored": 30.2,
    "pts_allowed": 17.1
  },
  "percentiles": {
    "win_pct": 99,
    "pts_scored": 95
  },
  "percentile_metadata": {
    "sample_size": 32
  },
  "team_name": "Kansas City Chiefs",
  "team_abbr": "KC",
  "logo_url": "https://...",
  "conference": "AFC",
  "division": "West",
  "updated_at": "2026-03-15T10:30:00Z"
}
```

> **Football note:** Also includes `league_name`. Filter by league: `?league_id=eq.8`.

---

### `GET /standings`

League standings, pre-sorted by the sport's ranking criteria.

```
GET /standings?season=eq.2025
Accept-Profile: nba
```

**Query examples:**
- `?season=eq.2025&conference=eq.East` — NBA/NFL: filter by conference
- `?season=eq.2025&league_id=eq.8` — Football: filter by league

**Response (each item):**

| Field | NBA / NFL | Football |
|-------|-----------|----------|
| `team_id` | int | int |
| `season` | int | int |
| `league_id` | null | int |
| `team_name` | string | string |
| `team_abbr` | string | string |
| `logo_url` | string | string |
| `conference` | string | *(absent)* |
| `division` | string | *(absent)* |
| `league_name` | *(absent)* | string |
| `stats` | JSONB | JSONB |
| `win_pct` | numeric | *(absent)* |
| `sort_points` | *(absent)* | int |
| `sort_goal_diff` | *(absent)* | int |

**Default sort order:**
- **NBA / NFL:** `win_pct DESC`
- **Football:** `sort_points DESC, sort_goal_diff DESC`

---

### `GET /stat_definitions`

Registry of all stat keys for the sport — display names, categories, and metadata.

```
GET /stat_definitions?entity_type=eq.player
Accept-Profile: nba
```

**Response (each item):**

```json
{
  "id": 1,
  "key_name": "pts",
  "display_name": "Points",
  "entity_type": "player",
  "category": "scoring",
  "is_inverse": false,
  "is_derived": false,
  "is_percentile_eligible": true,
  "sort_order": 1
}
```

---

### `GET /autofill_entities`

Materialized view combining players and teams for search / autofill UI.

```
GET /autofill_entities
Accept-Profile: nba
```

**Query examples:**
- `?name=ilike.*lebron*` — search by name
- `?type=eq.player` — only players
- `?type=eq.team` — only teams

**Response (each item):**

```json
{
  "id": 123,
  "type": "player",
  "name": "LeBron James",
  "position": "F",
  "detailed_position": "SF",
  "team_abbr": "LAL",
  "team_name": "Los Angeles Lakers",
  "league_id": null,
  "league_name": null,
  "meta": {}
}
```

For **team** rows, `position` = conference, `detailed_position` = division, `team_name` = null.

> **Football note:** `league_id` and `league_name` are populated for football entities.

---

### `POST /rpc/stat_leaders`

Top N players for a given stat.

```
POST /rpc/stat_leaders
Accept-Profile: nba
Content-Type: application/json

{
  "p_season": 2025,
  "p_stat_name": "pts",
  "p_limit": 25,
  "p_position": null,
  "p_league_id": 0
}
```

| Parameter     | Type    | Required | Default | Description                     |
|---------------|---------|----------|---------|---------------------------------|
| `p_season`    | integer | Yes      | —       | Season year                     |
| `p_stat_name` | text    | Yes      | —       | Any key in the `stats` JSONB    |
| `p_limit`     | integer | No       | 25      | Max results                     |
| `p_position`  | text    | No       | null    | Filter by position              |
| `p_league_id` | integer | No       | 0       | Filter by league (football)     |

**Response (each item):**

```json
{
  "rank": 1,
  "player_id": 123,
  "name": "LeBron James",
  "position": "F",
  "team_name": "Los Angeles Lakers",
  "stat_value": 25.7
}
```

---

### `POST /rpc/health`

Per-schema health check.

```
POST /rpc/health
Accept-Profile: nba
```

**Response:** `{ "status": "ok" }`

---

### `GET /leagues` (Football only)

League metadata. Only available in the `football` schema.

```
GET /leagues
Accept-Profile: football
```

**Query examples:**
- `?is_active=is.true` — only active leagues
- `?is_benchmark=is.true` — only benchmark leagues

**Response (each item):**

```json
{
  "id": 8,
  "name": "Premier League",
  "country": "England",
  "logo_url": "https://...",
  "is_benchmark": true,
  "is_active": true,
  "handicap": 0,
  "meta": {}
}
```

---

## Go API (Integrations & Ingestion)

The Go API handles third-party data (news, tweets), health checks, and documentation. It does **not** serve stats, profiles, or any core data.

### Common Response Headers

| Header           | Example Value                                      | Description                        |
|------------------|----------------------------------------------------|------------------------------------|
| `X-Process-Time` | `2.45ms`                                           | Server processing time             |
| `X-Cache`        | `HIT` / `MISS`                                     | In-memory cache status             |
| `ETag`           | `W/"abc123de"`                                     | Weak ETag for conditional requests |
| `Cache-Control`  | `public, max-age=600, stale-while-revalidate=300`  | Browser/CDN caching                |

Send `If-None-Match: <etag>` to receive `304 Not Modified` on cache hit.

### Error Format

All errors follow this structure:

```json
{
  "error": {
    "code": "MISSING_SPORT",
    "message": "Human-readable message",
    "detail": "Optional additional context"
  }
}
```

---

### `GET /`

API metadata.

**Response:**

```json
{
  "name": "Scoracle Data API",
  "version": "2.0.0",
  "status": "running",
  "docs": "/docs",
  "optimizations": [
    "pgxpool_connection_pooling",
    "prepared_statements",
    "postgres_json_passthrough",
    "gzip_compression",
    "in_memory_cache",
    "etag_support"
  ]
}
```

---

### `GET /health`

Basic health status.

**Response:** `{ "status": "healthy", "timestamp": "2026-03-16T10:30:45Z" }`

### `GET /health/db`

Postgres connectivity check.

**Response (healthy):**

```json
{
  "status": "healthy",
  "database": "connected",
  "timestamp": "2026-03-16T10:30:45Z"
}
```

**Response (unhealthy — HTTP 503):**

```json
{
  "status": "unhealthy",
  "database": "disconnected",
  "error": "Database connection check failed",
  "timestamp": "2026-03-16T10:30:45Z"
}
```

### `GET /health/cache`

In-memory cache statistics.

**Response:**

```json
{
  "status": "healthy",
  "cache": {
    "enabled": true,
    "total_keys": 42,
    "active_keys": 35,
    "expired_keys": 7
  },
  "timestamp": "2026-03-16T10:30:45Z"
}
```

---

### `GET /api/v1/news/status`

News service configuration status.

**Response:**

```json
{
  "rss_available": true,
  "newsapi_configured": false,
  "primary_source": "google_news_rss",
  "fallback_source": null
}
```

---

### `GET /api/v1/news/{entityType}/{entityID}`

Fetch news articles for a player or team.

| Parameter    | In    | Type    | Required | Description                                    |
|--------------|-------|---------|----------|------------------------------------------------|
| `entityType` | path  | string  | Yes      | `player` or `team`                             |
| `entityID`   | path  | integer | Yes      | Database entity ID                             |
| `sport`      | query | string  | Yes      | `NBA`, `NFL`, or `FOOTBALL`                    |
| `team`       | query | string  | No       | Team name context (helps filter player news)   |
| `limit`      | query | integer | No       | 1–50, default 10                               |
| `source`     | query | string  | No       | `rss` (default), `api`, or `both`              |

**Example:**

```
GET /api/v1/news/player/123?sport=NBA&team=Lakers&limit=5
```

**Response:**

```json
{
  "query": "LeBron James",
  "sport": "NBA",
  "entity": {
    "type": "player",
    "id": 123,
    "name": "LeBron James",
    "sport": "NBA"
  },
  "articles": [
    {
      "title": "LeBron James scores 30 points in victory",
      "description": "NBA star delivers in clutch...",
      "url": "https://example.com/article",
      "source": "ESPN",
      "published_at": "2026-03-16T08:00:00Z",
      "image_url": "https://example.com/image.jpg",
      "author": "John Doe"
    }
  ],
  "provider": "google_news_rss",
  "meta": {
    "total_results": 3,
    "returned": 3,
    "source": "google_news_rss"
  }
}
```

**Cache:** 10-minute TTL. ETag support.

**Errors:** `400` (bad params), `404` (entity not found), `502` (external service error).

---

### `GET /api/v1/twitter/status`

Twitter API configuration status.

**Response:**

```json
{
  "service": "twitter",
  "configured": true,
  "journalist_list_configured": true,
  "journalist_list_id": "1234567890",
  "feed_cache_ttl_seconds": 3600,
  "rate_limit": "900 requests / 15 min (List endpoint)",
  "note": "Only journalist-feed endpoint available. Generic search removed to ensure content quality."
}
```

---

### `GET /api/v1/twitter/journalist-feed`

Search the cached journalist X List feed.

| Parameter | In    | Type    | Required | Description                           |
|-----------|-------|---------|----------|---------------------------------------|
| `q`       | query | string  | Yes      | Search query (1–200 chars)            |
| `sport`   | query | string  | No       | `NBA`, `NFL`, or `FOOTBALL` (metadata)|
| `limit`   | query | integer | No       | 1–50, default 10                      |

**Example:**

```
GET /api/v1/twitter/journalist-feed?q=Lakers&sport=NBA&limit=5
```

**Response:**

```json
{
  "query": "Lakers",
  "sport": "NBA",
  "tweets": [
    {
      "id": "1234567890",
      "text": "Lakers dominate in latest game...",
      "author": {
        "username": "sports_journalist",
        "name": "Sports Journalist",
        "verified": true,
        "profile_image_url": "https://pbs.twimg.com/..."
      },
      "created_at": "2026-03-16T10:30:00Z",
      "metrics": {
        "likes": 1250,
        "retweets": 450,
        "replies": 89
      },
      "url": "https://twitter.com/sports_journalist/status/1234567890"
    }
  ],
  "meta": {
    "result_count": 1,
    "feed_cached": true,
    "feed_size": 245,
    "cache_ttl_seconds": 3600
  }
}
```

**Cache:** Feed is cached for 1 hour. ETag support.

**Errors:** `400` (bad query), `502` (Twitter API error), `503` (Twitter not configured).

---

### `GET /docs/`

Multi-spec Swagger UI. Dropdown includes both the Go API spec and the PostgREST OpenAPI spec.

---

## Quick Reference — Frontend Integration Cheat Sheet

```
POSTGREST = "https://postgrest-production-0650.up.railway.app"
GO_API    = "<your-go-api-railway-url>"

# --- PostgREST (always send Accept-Profile) ---

# Search / autofill
GET  ${POSTGREST}/autofill_entities?name=ilike.*lebron*
     Accept-Profile: nba

# Player profile
GET  ${POSTGREST}/players?id=eq.123
     Accept-Profile: nba

# Player stats
GET  ${POSTGREST}/player_stats?player_id=eq.123&season=eq.2025
     Accept-Profile: nba

# Team profile
GET  ${POSTGREST}/teams?id=eq.5
     Accept-Profile: nfl

# Team stats
GET  ${POSTGREST}/team_stats?team_id=eq.5&season=eq.2025
     Accept-Profile: nfl

# Standings
GET  ${POSTGREST}/standings?season=eq.2025
     Accept-Profile: nba

# Stat leaders
POST ${POSTGREST}/rpc/stat_leaders
     Accept-Profile: nba
     Content-Type: application/json
     {"p_season": 2025, "p_stat_name": "pts", "p_limit": 10}

# Stat definitions
GET  ${POSTGREST}/stat_definitions?entity_type=eq.player
     Accept-Profile: nba

# Leagues (football only)
GET  ${POSTGREST}/leagues?is_active=is.true
     Accept-Profile: football

# --- Go API ---

# News
GET  ${GO_API}/api/v1/news/player/123?sport=NBA&limit=10

# Journalist tweets
GET  ${GO_API}/api/v1/twitter/journalist-feed?q=Lakers&sport=NBA

# Health
GET  ${GO_API}/health
GET  ${GO_API}/health/db
```
