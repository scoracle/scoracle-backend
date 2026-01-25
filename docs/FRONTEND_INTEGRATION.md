# Scoracle Data API - Frontend Integration Guide

## Base URL

```
https://scoracle-data-production.up.railway.app/api/v1
```

## Quick Start

```javascript
// Fetch player info
const response = await fetch(
  'https://scoracle-data-production.up.railway.app/api/v1/widget/info/player/123?sport=NBA'
);
const data = await response.json();

// Check cache status
console.log(response.headers.get('X-Cache')); // HIT or MISS
```

## Authentication

**No authentication required** - All endpoints are publicly accessible.

## CORS

Configured for common frontend dev ports:
- `http://localhost:3000` (Next.js, React)
- `http://localhost:4321` (Astro)
- `http://localhost:5173` (Vite)
- Production: `https://scoracle.com`, `https://www.scoracle.com`

## API Endpoints

### Widget Endpoints (`/api/v1/widget`)

Primary endpoints for player/team data. Optimized for performance with caching and ETags.

#### Get Entity Info

```http
GET /widget/info/{entity_type}/{entity_id}?sport={sport}
```

Returns basic entity information (name, position, team, etc.)

**Parameters:**
- `entity_type`: `player` or `team`
- `entity_id`: Entity ID (integer)
- `sport`: `NBA`, `NFL`, or `FOOTBALL` (required)

**Example:**
```javascript
// Get player info
GET /widget/info/player/237?sport=NBA

// Get team info
GET /widget/info/team/1?sport=NBA
```

**Response:**
```json
{
  "id": 237,
  "name": "LeBron James",
  "firstname": "LeBron",
  "lastname": "James",
  "birth_date": "1984-12-30",
  "birth_country": "USA",
  "nba_start": 2003,
  "height_feet": 6,
  "height_inches": 9,
  "weight_pounds": 250,
  "college": "St. Vincent-St. Mary HS",
  "affiliation": "St. Vincent-St. Mary HS/USA",
  "jersey_number": "6"
}
```

**Cache:** 24 hours

---

#### Get Entity Stats

```http
GET /widget/stats/{entity_type}/{entity_id}?sport={sport}&season={year}
```

Returns season statistics for an entity.

**Parameters:**
- `entity_type`: `player` or `team`
- `entity_id`: Entity ID (integer)
- `sport`: `NBA`, `NFL`, or `FOOTBALL` (required)
- `season`: Year (optional, defaults to current season)
- `league_id`: League ID for FOOTBALL (optional)

**Example:**
```javascript
// Get current season stats (defaults to 2025 for NBA)
GET /widget/stats/player/237?sport=NBA

// Get specific season
GET /widget/stats/player/237?sport=NBA&season=2024
```

**Response:**
```json
{
  "player_id": 237,
  "season": 2025,
  "team_id": 31,
  "games": 58,
  "points": 25.4,
  "totReb": 7.4,
  "assists": 7.9,
  "steals": 1.3,
  "blocks": 0.6,
  "fgm": 9.7,
  "fga": 18.5,
  "fgp": "52.4",
  "ftm": 5.3,
  "fta": 7.2,
  "ftp": "73.6",
  "tpm": 0.7,
  "tpa": 2.3,
  "tpp": "30.4",
  "offReb": 1.1,
  "defReb": 6.3,
  "turnovers": 3.2,
  "pFouls": 1.8,
  "plusMinus": "+4.2"
}
```

**Cache:** 1 hour (current season), 24 hours (historical)

---

#### Get Available Seasons

```http
GET /widget/stats/{entity_type}/{entity_id}/seasons?sport={sport}
```

Returns list of seasons with available stats (useful for building season selectors).

**Example:**
```javascript
GET /widget/stats/player/237/seasons?sport=NBA
```

**Response:**
```json
{
  "seasons": [2025, 2024, 2023, 2022, 2021, 2020, ...]
}
```

**Cache:** 1 hour

---

#### Get Complete Profile (Unified)

```http
GET /widget/profile/{entity_type}/{entity_id}?sport={sport}&season={year}&include_percentiles={bool}
```

**🔥 Recommended:** Returns info + stats + percentiles in a single optimized request. Eliminates 3 separate API calls.

**Parameters:**
- `entity_type`: `player` or `team`
- `entity_id`: Entity ID (integer)
- `sport`: `NBA`, `NFL`, or `FOOTBALL` (required)
- `season`: Year (optional, defaults to current)
- `league_id`: League ID for FOOTBALL (optional)
- `include_percentiles`: Include percentile rankings (default: `true`)

**Example:**
```javascript
GET /widget/profile/player/237?sport=NBA&include_percentiles=true
```

**Response:**
```json
{
  "info": {
    "id": 237,
    "name": "LeBron James",
    "firstname": "LeBron",
    "lastname": "James",
    ...
  },
  "stats": {
    "player_id": 237,
    "season": 2025,
    "points": 25.4,
    "totReb": 7.4,
    ...
  },
  "percentiles": {
    "points": 95.2,
    "totReb": 78.5,
    "assists": 92.1,
    ...
  }
}
```

**Cache:** 1 hour (current season), 24 hours (historical)

---

### News Endpoints (`/api/v1/news`)

News articles about players/teams. Uses NewsAPI (if configured) or Google News RSS (free fallback).

#### Get Entity News

```http
GET /news/{entity_name}?sport={sport}&team={team}&limit={limit}
```

**Parameters:**
- `entity_name`: Player or team name (e.g., "LeBron James", "Lakers")
- `sport`: `NBA`, `NFL`, or `FOOTBALL` (optional, improves filtering)
- `team`: Team name for context (optional)
- `limit`: Max results (1-50, default: 10)

**Example:**
```javascript
GET /news/LeBron%20James?sport=NBA&limit=10
```

**Response:**
```json
{
  "articles": [
    {
      "title": "LeBron James Scores 40 in Lakers Win",
      "description": "LeBron James led the Lakers to victory...",
      "url": "https://example.com/article",
      "source": "ESPN",
      "published_at": "2025-01-11T12:00:00Z",
      "image": "https://example.com/image.jpg"
    },
    ...
  ],
  "total": 10,
  "source": "google_news"  // or "newsapi"
}
```

**Cache:** 10 minutes

---

#### Get Sport News

```http
GET /news?sport={sport}&limit={limit}
```

General news for a sport (no specific entity).

**Example:**
```javascript
GET /news?sport=NBA&limit=20
```

**Cache:** 10 minutes

---

### Intel Endpoints (`/api/v1/intel`)

Social media and news intel from external APIs. **Requires API keys** (optional).

#### Twitter Search

```http
GET /intel/twitter?query={query}&sport={sport}&limit={limit}
```

**Requires:** `TWITTER_BEARER_TOKEN` environment variable

**Parameters:**
- `query`: Search query (e.g., "LeBron James")
- `sport`: `NBA`, `NFL`, or `FOOTBALL` (optional, improves filtering)
- `limit`: Max tweets (1-100, default: 10)

**Example:**
```javascript
GET /intel/twitter?query=LeBron%20James&sport=NBA&limit=10
```

**Response:**
```json
{
  "tweets": [
    {
      "id": "123456789",
      "text": "LeBron with another 40 point game! 🔥",
      "author": "ESPN",
      "author_username": "espn",
      "created_at": "2025-01-11T20:30:00Z",
      "metrics": {
        "retweet_count": 1250,
        "reply_count": 89,
        "like_count": 5420,
        "quote_count": 45
      }
    },
    ...
  ],
  "total": 10
}
```

**Cache:** 5 minutes

---

#### News Search (NewsAPI)

```http
GET /intel/news?query={query}&sport={sport}&days={days}&limit={limit}
```

**Requires:** `NEWS_API_KEY` environment variable (otherwise use `/news` endpoints)

**Parameters:**
- `query`: Search query
- `sport`: `NBA`, `NFL`, or `FOOTBALL` (optional)
- `days`: Days back to search (1-30, default: 7)
- `limit`: Max articles (1-100, default: 10)

**Cache:** 5 minutes

---

#### Reddit Search

```http
GET /intel/reddit?query={query}&sport={sport}&subreddit={subreddit}&sort={sort}&limit={limit}
```

**Requires:** `REDDIT_CLIENT_ID` and `REDDIT_CLIENT_SECRET` environment variables

**Parameters:**
- `query`: Search query
- `sport`: `NBA`, `NFL`, or `FOOTBALL` (optional, auto-selects subreddit)
- `subreddit`: Specific subreddit (optional, e.g., "nba")
- `sort`: `relevance`, `hot`, `new`, or `top` (default: `relevance`)
- `limit`: Max posts (1-100, default: 10)

**Example:**
```javascript
GET /intel/reddit?query=Lakers&sport=NBA&sort=hot&limit=10
```

**Cache:** 5 minutes

---

#### Intel Status

```http
GET /intel/status
```

Check which external APIs are configured and available.

**Response:**
```json
{
  "twitter": true,
  "news": false,
  "reddit": true
}
```

---

### Health Check

```http
GET /health
```

Check if API is running.

**Response:**
```json
{
  "status": "healthy"
}
```

---

## Performance Features

### Caching

All endpoints use intelligent caching with `Cache-Control` headers:

```javascript
// Check if response came from cache
const cacheStatus = response.headers.get('X-Cache'); // "HIT" or "MISS"
```

**Cache TTLs:**
- Entity info: 24 hours
- Current season stats: 1 hour
- Historical stats: 24 hours
- News: 10 minutes
- External APIs (Twitter/Reddit): 5 minutes

### Conditional Requests (ETags)

Widget endpoints support ETags for efficient conditional requests:

```javascript
// First request
const response1 = await fetch('/widget/info/player/237?sport=NBA');
const etag = response1.headers.get('ETag');
const data1 = await response1.json();

// Later request - save bandwidth if data unchanged
const response2 = await fetch('/widget/info/player/237?sport=NBA', {
  headers: { 'If-None-Match': etag }
});

if (response2.status === 304) {
  // Not modified - use cached data
  console.log('Data unchanged, using cache');
} else {
  // Modified - parse new data
  const data2 = await response2.json();
}
```

---

## Error Handling

All errors return consistent JSON format:

```json
{
  "error": "NotFound",
  "message": "Player not found",
  "detail": "No player with id=999 in NBA database",
  "status_code": 404
}
```

**Common Status Codes:**
- `200` - Success
- `304` - Not Modified (ETag match)
- `400` - Bad Request (invalid parameters)
- `404` - Not Found (entity doesn't exist)
- `429` - Rate Limited (too many requests)
- `500` - Server Error
- `503` - Service Unavailable (external API down)

**Example Error Handler:**
```javascript
try {
  const response = await fetch('/widget/stats/player/999?sport=NBA');

  if (!response.ok) {
    const error = await response.json();
    console.error(`${error.error}: ${error.message}`);
    return;
  }

  const data = await response.json();
} catch (e) {
  console.error('Network error:', e);
}
```

---

## Rate Limiting

Rate limiting is **enabled** with these defaults:
- **100 requests per 60 seconds** per IP

Rate limit info in headers:
```javascript
const remaining = response.headers.get('X-RateLimit-Remaining');
const reset = response.headers.get('X-RateLimit-Reset');
```

When rate limited (429):
```json
{
  "error": "RateLimited",
  "message": "Rate limit exceeded",
  "retry_after": 60
}
```

---

## Best Practices

### 1. Use the Profile Endpoint

Instead of 3 separate calls:
```javascript
// ❌ Don't do this
const info = await fetch('/widget/info/player/237?sport=NBA');
const stats = await fetch('/widget/stats/player/237?sport=NBA');
const percentiles = await fetch('/widget/percentiles/player/237?sport=NBA');
```

Use the unified profile endpoint:
```javascript
// ✅ Do this
const profile = await fetch('/widget/profile/player/237?sport=NBA');
```

### 2. Implement ETag Support

Save bandwidth and improve performance:
```javascript
const etag = localStorage.getItem('player-237-etag');

const response = await fetch('/widget/info/player/237?sport=NBA', {
  headers: etag ? { 'If-None-Match': etag } : {}
});

if (response.status === 304) {
  // Use cached data
  return JSON.parse(localStorage.getItem('player-237-data'));
}

const data = await response.json();
localStorage.setItem('player-237-etag', response.headers.get('ETag'));
localStorage.setItem('player-237-data', JSON.stringify(data));
```

### 3. Respect Cache Headers

Use the `Cache-Control` header to implement client-side caching:
```javascript
const maxAge = response.headers.get('Cache-Control')
  .match(/max-age=(\d+)/)?.[1];
```

### 4. Handle External API Failures

Intel endpoints may be unavailable if API keys aren't configured:
```javascript
// Check status first
const status = await fetch('/intel/status').then(r => r.json());

if (status.twitter) {
  // Twitter is available
  const tweets = await fetch('/intel/twitter?query=Lakers&sport=NBA');
}
```

### 5. Lazy Load Intel Data

Load stats immediately, social intel on demand:
```javascript
// Immediately load profile
const profile = await fetch('/widget/profile/player/237?sport=NBA');

// Load news/Twitter when user clicks "News" tab
document.getElementById('news-tab').addEventListener('click', async () => {
  const news = await fetch('/news/LeBron%20James?sport=NBA');
});
```

---

## Interactive Documentation

Full interactive API documentation with request/response examples:

- **Swagger UI**: https://scoracle-data-production.up.railway.app/docs
- **ReDoc**: https://scoracle-data-production.up.railway.app/redoc

---

## Example: Complete Player Widget

```javascript
async function loadPlayerWidget(playerId, sport) {
  try {
    // 1. Load complete profile (info + stats + percentiles)
    const profileResponse = await fetch(
      `https://scoracle-data-production.up.railway.app/api/v1/widget/profile/player/${playerId}?sport=${sport}`
    );

    if (!profileResponse.ok) {
      throw new Error('Failed to load player profile');
    }

    const profile = await profileResponse.json();

    // 2. Display player info
    document.getElementById('player-name').textContent = profile.info.name;
    document.getElementById('player-position').textContent = profile.info.position;

    // 3. Display stats
    document.getElementById('points').textContent = profile.stats.points;
    document.getElementById('rebounds').textContent = profile.stats.totReb;
    document.getElementById('assists').textContent = profile.stats.assists;

    // 4. Display percentiles (for ranking visualization)
    document.getElementById('points-percentile').textContent =
      `${profile.percentiles.points}th percentile`;

    // 5. Lazy-load news when tab is clicked
    document.getElementById('news-tab').addEventListener('click', async () => {
      const newsResponse = await fetch(
        `https://scoracle-data-production.up.railway.app/api/v1/news/${encodeURIComponent(profile.info.name)}?sport=${sport}&limit=10`
      );
      const news = await newsResponse.json();

      displayNews(news.articles);
    }, { once: true });

  } catch (error) {
    console.error('Error loading player widget:', error);
    displayError('Failed to load player data');
  }
}

// Usage
loadPlayerWidget(237, 'NBA'); // LeBron James
```

---

## Support

- **API Docs**: https://scoracle-data-production.up.railway.app/docs
- **Health Check**: https://scoracle-data-production.up.railway.app/health
- **Issues**: Contact backend team or check Railway logs

---

## Summary

**Essential Endpoints:**
- `GET /widget/profile/{type}/{id}` - Complete player/team data (recommended)
- `GET /news/{name}` - Entity news (free, no API key required)
- `GET /intel/status` - Check external API availability

**Performance:**
- All responses cached with appropriate TTLs
- ETag support for conditional requests
- `X-Cache` header shows cache hits
- Rate limited at 100 req/min per IP

**Data Freshness:**
- Stats updated once daily (API-Sports limitation)
- Use 1-hour cache for current season
- News refreshed every 10 minutes
- Social intel refreshed every 5 minutes
