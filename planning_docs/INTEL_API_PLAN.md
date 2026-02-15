# Sports Intelligence API Plan: Lazy-Load Tab Architecture

## Overview

Extend the existing FastAPI service with endpoints for Twitter, News, and Reddit data. The frontend uses a **lazy-load tab pattern**: stats and news load immediately, while Twitter and Reddit only fetch when the user clicks those tabs.

## Why This Architecture

**Previous approach (rejected):** Go service with SSE streaming, concurrent goroutine orchestration, fan-out/fan-in patterns.

**Current approach:** Simple FastAPI request/response endpoints with frontend-driven lazy loading.

### Benefits

1. **No SSE complexity** - Simple request/response, no streaming logic
2. **No concurrent orchestration** - Each endpoint is independent
3. **Frontend owns UX** - Loading states, tab switching, caching all handled where they belong
4. **Existing stack** - Extends FastAPI service that already exists
5. **Frontend caching** - localStorage/memory handles 80% of caching needs with zero backend complexity

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Astro Frontend (Vercel)                           │
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                        Player/Team Page                              │   │
│  │                                                                      │   │
│  │  ┌──────────────┐  ┌──────────────────────────────────────────────┐ │   │
│  │  │              │  │              Tab Panel                        │ │   │
│  │  │   Stats      │  │  ┌────────┬────────┬────────┬────────┐       │ │   │
│  │  │   Widget     │  │  │ News   │Twitter │ Reddit │ Stats  │       │ │   │
│  │  │              │  │  │(active)│        │        │        │       │ │   │
│  │  │  (loads      │  │  └────────┴────────┴────────┴────────┘       │ │   │
│  │  │   immediately)│  │                                              │ │   │
│  │  │              │  │  News loads immediately (same as stats)      │ │   │
│  │  │              │  │  Twitter/Reddit: fetch on tab click          │ │   │
│  │  │              │  │  Results cached in localStorage              │ │   │
│  │  └──────────────┘  └──────────────────────────────────────────────┘ │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  Caching Strategy:                                                          │
│  - Stats: 5min cache (API-side)                                            │
│  - News: localStorage with 15min TTL                                       │
│  - Twitter: localStorage with 5min TTL                                     │
│  - Reddit: localStorage with 10min TTL                                     │
└─────────────────────────────────────────────────────────────────────────────┘
                    │               │               │
                    ▼               ▼               ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         FastAPI Service                                     │
│                                                                             │
│  Existing Endpoints:                    New Endpoints:                      │
│  ├── GET /api/v1/teams/:id              ├── GET /api/v1/intel/twitter      │
│  ├── GET /api/v1/players/:id            ├── GET /api/v1/intel/news         │
│  ├── GET /api/v1/teams/:id/stats        ├── GET /api/v1/intel/reddit       │
│  └── GET /api/v1/players/:id/stats      └── GET /health/external           │
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                    External API Clients                              │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐                  │   │
│  │  │   Twitter   │  │    News     │  │   Reddit    │                  │   │
│  │  │   Client    │  │   Client    │  │   Client    │                  │   │
│  │  │             │  │             │  │             │                  │   │
│  │  │ Rate limit: │  │ Rate limit: │  │ Rate limit: │                  │   │
│  │  │ 450/15min   │  │ 100/day     │  │ 100/min     │                  │   │
│  │  └─────────────┘  └─────────────┘  └─────────────┘                  │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
                    │               │               │
                    ▼               ▼               ▼
           ┌────────────┐  ┌────────────┐  ┌────────────┐
           │ Twitter    │  │ NewsAPI /  │  │ Reddit     │
           │ API v2     │  │ Bing News  │  │ API        │
           └────────────┘  └────────────┘  └────────────┘
```

## New API Endpoints

### Twitter/X Intelligence

```
GET /api/v1/intel/twitter?q={query}&sport={sport}&limit={limit}
```

**Parameters:**
- `q` (required): Search query (player name, team name)
- `sport` (optional): NBA, NFL, FOOTBALL - adds context to search
- `limit` (optional): Max results, default 10, max 50

**Response:**
```json
{
  "query": "Cole Palmer",
  "sport": "FOOTBALL",
  "tweets": [
    {
      "id": "1234567890",
      "text": "Cole Palmer with another assist! Chelsea looking strong.",
      "author": {
        "username": "SportsCenter",
        "name": "SportsCenter",
        "verified": true,
        "profile_image_url": "https://..."
      },
      "created_at": "2024-01-15T14:30:00Z",
      "metrics": {
        "likes": 1523,
        "retweets": 342,
        "replies": 89
      },
      "url": "https://twitter.com/SportsCenter/status/1234567890"
    }
  ],
  "meta": {
    "result_count": 10,
    "newest_id": "1234567890",
    "oldest_id": "1234567880"
  }
}
```

### News Intelligence

```
GET /api/v1/intel/news?q={query}&sport={sport}&days={days}&limit={limit}
```

**Parameters:**
- `q` (required): Search query
- `sport` (optional): Adds sport-specific sources
- `days` (optional): How far back to search, default 7, max 30
- `limit` (optional): Max results, default 10, max 50

**Response:**
```json
{
  "query": "Cole Palmer",
  "sport": "FOOTBALL",
  "articles": [
    {
      "title": "Cole Palmer Named Premier League Player of the Month",
      "description": "Chelsea's young star continues to impress...",
      "url": "https://www.espn.com/soccer/story/...",
      "source": "ESPN",
      "author": "James Olley",
      "published_at": "2024-01-14T10:00:00Z",
      "image_url": "https://..."
    }
  ],
  "meta": {
    "total_results": 45,
    "returned": 10
  }
}
```

### Reddit Intelligence

```
GET /api/v1/intel/reddit?q={query}&sport={sport}&sort={sort}&limit={limit}
```

**Parameters:**
- `q` (required): Search query
- `sport` (optional): Determines subreddit (NBA→r/nba, NFL→r/nfl, FOOTBALL→r/soccer)
- `sort` (optional): relevance, hot, new, top (default: relevance)
- `limit` (optional): Max results, default 10, max 50

**Response:**
```json
{
  "query": "Cole Palmer",
  "sport": "FOOTBALL",
  "subreddit": "soccer",
  "posts": [
    {
      "id": "abc123",
      "title": "Cole Palmer's stats this season are insane",
      "selftext": "Looking at his numbers compared to other young players...",
      "author": "soccer_stats_guy",
      "subreddit": "soccer",
      "score": 2341,
      "num_comments": 456,
      "created_utc": 1705234800,
      "url": "https://reddit.com/r/soccer/comments/abc123",
      "permalink": "/r/soccer/comments/abc123/cole_palmers_stats_this_season_are_insane",
      "is_self": true,
      "thumbnail": "https://..."
    }
  ],
  "meta": {
    "result_count": 10
  }
}
```

## Project Structure Updates

```
src/scoracle_data/
├── api/
│   ├── main.py                 # Add new router
│   ├── routers/
│   │   ├── teams.py
│   │   ├── players.py
│   │   └── intel.py            # NEW: Twitter/News/Reddit endpoints
│   └── ...
├── external/                    # NEW: External API clients
│   ├── __init__.py
│   ├── base.py                 # Base client with retry logic
│   ├── twitter.py              # Twitter API v2 client
│   ├── news.py                 # NewsAPI client
│   └── reddit.py               # Reddit API client
└── ...
```

## External API Client Design

### Base Client

```python
class BaseExternalClient:
    """Base client with retry logic, rate limiting, and error handling."""

    def __init__(self, rate_limit: tuple[int, int]):  # (requests, seconds)
        self.client = httpx.AsyncClient(timeout=10.0)
        self.rate_limiter = AsyncLimiter(*rate_limit)

    async def _request(self, method: str, url: str, **kwargs) -> dict:
        async with self.rate_limiter:
            for attempt in range(3):
                try:
                    response = await self.client.request(method, url, **kwargs)
                    response.raise_for_status()
                    return response.json()
                except httpx.HTTPStatusError as e:
                    if e.response.status_code == 429:  # Rate limited
                        await asyncio.sleep(2 ** attempt)
                        continue
                    raise
```

### Rate Limits

| Service | Rate Limit | Strategy |
|---------|------------|----------|
| Twitter API v2 | 450 req/15min | AsyncLimiter + backoff |
| NewsAPI | 100 req/day (free) | Track daily usage |
| Reddit API | 100 req/min | AsyncLimiter |

## Frontend Integration

```typescript
// src/lib/intel.ts

interface CacheEntry<T> {
  data: T;
  timestamp: number;
}

const CACHE_TTL = {
  twitter: 5 * 60 * 1000,   // 5 minutes
  news: 15 * 60 * 1000,     // 15 minutes
  reddit: 10 * 60 * 1000,   // 10 minutes
};

function getCached<T>(key: string, ttl: number): T | null {
  const raw = localStorage.getItem(key);
  if (!raw) return null;

  const entry: CacheEntry<T> = JSON.parse(raw);
  if (Date.now() - entry.timestamp > ttl) {
    localStorage.removeItem(key);
    return null;
  }
  return entry.data;
}

function setCache<T>(key: string, data: T): void {
  const entry: CacheEntry<T> = { data, timestamp: Date.now() };
  localStorage.setItem(key, JSON.stringify(entry));
}

export async function fetchTwitterIntel(query: string, sport?: string) {
  const cacheKey = `twitter:${query}:${sport}`;
  const cached = getCached(cacheKey, CACHE_TTL.twitter);
  if (cached) return cached;

  const params = new URLSearchParams({ q: query });
  if (sport) params.set('sport', sport);

  const res = await fetch(`${API_URL}/api/v1/intel/twitter?${params}`);
  const data = await res.json();

  setCache(cacheKey, data);
  return data;
}

// Similar for news, reddit...
```

```astro
---
// Tab component with lazy loading
---
<div class="tabs" x-data="{ activeTab: 'news', loaded: { news: true, twitter: false, reddit: false } }">
  <div class="tab-buttons">
    <button @click="activeTab = 'news'">News</button>
    <button @click="activeTab = 'twitter'; if (!loaded.twitter) { loadTwitter(); loaded.twitter = true }">
      Twitter
    </button>
    <button @click="activeTab = 'reddit'; if (!loaded.reddit) { loadReddit(); loaded.reddit = true }">
      Reddit
    </button>
  </div>

  <div x-show="activeTab === 'news'">
    <!-- News content (loaded immediately) -->
  </div>

  <div x-show="activeTab === 'twitter'">
    <!-- Twitter content (loaded on tab click) -->
  </div>

  <div x-show="activeTab === 'reddit'">
    <!-- Reddit content (loaded on tab click) -->
  </div>
</div>
```

## Implementation Phases

### Phase 1: Infrastructure
- [x] Plan document
- [ ] Create `external/` package structure
- [ ] Base client with retry logic
- [ ] Add new dependencies (aiolimiter)

### Phase 2: API Clients
- [ ] Twitter API v2 client
- [ ] NewsAPI client
- [ ] Reddit OAuth2 client

### Phase 3: Endpoints
- [ ] Intel router with Twitter endpoint
- [ ] News endpoint
- [ ] Reddit endpoint
- [ ] Health check for external services

### Phase 4: Testing
- [ ] Unit tests for clients
- [ ] Integration tests with mocked APIs
- [ ] Rate limit testing

## Environment Variables

```bash
# Existing
DATABASE_URL=postgresql://...
API_SPORTS_KEY=...

# New - External APIs
TWITTER_BEARER_TOKEN=          # Twitter API v2 App-only bearer token
NEWS_API_KEY=                  # NewsAPI.org API key
REDDIT_CLIENT_ID=              # Reddit app client ID
REDDIT_CLIENT_SECRET=          # Reddit app client secret
```

## Error Handling

All intel endpoints return consistent error responses:

```json
{
  "error": {
    "code": "RATE_LIMITED",
    "message": "Twitter API rate limit exceeded. Try again in 5 minutes.",
    "retry_after": 300
  }
}
```

Error codes:
- `RATE_LIMITED` - External API rate limit hit
- `EXTERNAL_API_ERROR` - External API returned error
- `INVALID_QUERY` - Bad search query
- `SERVICE_UNAVAILABLE` - External service down

The frontend handles these gracefully with appropriate UI states.
