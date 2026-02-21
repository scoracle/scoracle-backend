# Scoracle Data API

High-performance JSON API for serving team and player statistics data.

## Features

- **Ultra-fast JSON serialization** using msgspec (4-5x faster than stdlib)
- **Optimized database queries** using single JOIN queries instead of multiple round-trips
- **In-memory caching** with 5-minute TTL for 80%+ cache hit rate
- **Connection pooling** with configurable pool sizes
- **Complete data delivery** - All datapoints returned, client-side filtering
- **Health monitoring** endpoints for database and cache status
- **Auto-generated API docs** at `/docs` and `/redoc`

## Performance Targets

- **Cached requests:** 1-3ms
- **Uncached requests:** 15-30ms
- **Average (80% cache hit):** 5-10ms
- **Throughput:** 3,000-5,000 req/sec (4 workers)

## API Endpoints

### Core Endpoints

#### Get Team Profile
```
GET /api/v1/teams/{team_id}?sport={sport}&season={season}
```

Returns complete team profile including:
- Team information (name, logo, venue, etc.)
- Season statistics
- Percentile rankings for all stat categories

**Parameters:**
- `team_id` (int): Team ID
- `sport` (string): Sport ID (`NBA`, `NFL`, `FOOTBALL`)
- `season` (int): Season year (2000-2030)

**Example:**
```bash
curl "http://localhost:8000/api/v1/teams/1?sport=NBA&season=2025"
```

#### Get Player Profile
```
GET /api/v1/players/{player_id}?sport={sport}&season={season}
```

Returns complete player profile including:
- Player information (name, photo, position, bio, etc.)
- Current team information
- Season statistics
- Percentile rankings for all stat categories

**Parameters:**
- `player_id` (int): Player ID
- `sport` (string): Sport ID (`NBA`, `NFL`, `FOOTBALL`)
- `season` (int): Season year (2000-2030)

**Example:**
```bash
curl "http://localhost:8000/api/v1/players/237?sport=NBA&season=2025"
```

### Health Check Endpoints

#### Basic Health
```
GET /health
```

Returns API status and timestamp.

#### Database Health
```
GET /health/db
```

Returns database connectivity status. Returns 503 if database is unavailable.

#### Cache Health
```
GET /health/cache
```

Returns cache status including entry count and TTL configuration.

## Running the API

### Development

```bash
# Set environment variables
export DATABASE_URL="postgresql://user:pass@host:port/db?sslmode=require"
export PYTHONPATH=/home/user/scoracle-data/src

# Run with auto-reload
uvicorn scoracle_data.api.main:app --reload --host 0.0.0.0 --port 8000
```

### Production

```bash
# Set environment variables
export DATABASE_URL="postgresql://user:pass@host:port/db?sslmode=require"
export DATABASE_POOL_SIZE=20
export PYTHONPATH=/home/user/scoracle-data/src

# Run with multiple workers
gunicorn scoracle_data.api.main:app \
  --workers 4 \
  --worker-class uvicorn.workers.UvicornWorker \
  --bind 0.0.0.0:8000 \
  --timeout 30
```

## Environment Variables

- `DATABASE_URL` (required): PostgreSQL connection string
- `DATABASE_POOL_SIZE` (optional): Max connections in pool (default: 10, recommended for API: 20)
- `DATABASE_POOL_MIN_SIZE` (optional): Min connections in pool (default: 2, recommended: 5)

## Architecture

### Technologies

- **FastAPI**: Async web framework with automatic validation
- **msgspec**: Ultra-fast JSON serialization (4-5x faster than orjson)
- **psycopg3**: PostgreSQL driver with connection pooling
- **Uvicorn**: ASGI server with uvloop for high performance

### Caching Strategy

- **Layer**: In-memory thread-safe cache
- **TTL**: 5 minutes (configurable)
- **Key format**: MD5 hash of (entity_type, entity_id, sport, season)
- **Cache invalidation**: Automatic TTL expiration

### Database Optimization

Uses optimized single-query methods that combine:
- Entity info (team/player)
- Season statistics
- Percentile rankings

Instead of 3-4 separate queries, uses a single JOIN query with JSON aggregation.

**Performance gain:** ~50% faster than multiple queries

## API Documentation

Once running, visit:
- **Swagger UI:** http://localhost:8000/docs
- **ReDoc:** http://localhost:8000/redoc

## Response Format

### Team Profile Response
```json
{
  "team": {
    "id": 1,
    "sport_id": "NBA",
    "name": "Los Angeles Lakers",
    "abbreviation": "LAL",
    "logo_url": "https://...",
    "conference": "Western",
    "division": "Pacific",
    ...
  },
  "stats": {
    "games_played": 41,
    "wins": 28,
    "losses": 13,
    "points_per_game": 115.2,
    ...
  },
  "percentiles": [
    {
      "stat_category": "points_per_game",
      "stat_value": 115.2,
      "percentile": 87.3,
      "rank": 4,
      "sample_size": 30,
      "comparison_group": "NBA:2025"
    },
    ...
  ]
}
```

### Player Profile Response
```json
{
  "player": {
    "id": 237,
    "sport_id": "NBA",
    "full_name": "LeBron James",
    "position": "SF",
    "photo_url": "https://...",
    "current_team_id": 1,
    ...
  },
  "team": {
    "id": 1,
    "name": "Los Angeles Lakers",
    ...
  },
  "stats": {
    "games_played": 40,
    "points_per_game": 25.8,
    "assists_per_game": 7.2,
    ...
  },
  "percentiles": [
    {
      "stat_category": "points_per_game",
      "stat_value": 25.8,
      "percentile": 92.1,
      "rank": 12,
      "sample_size": 450,
      "comparison_group": "NBA:2025:SF"
    },
    ...
  ]
}
```

## CORS Configuration

Currently configured to allow all origins (`*`) for development. For production, update `main.py`:

```python
app.add_middleware(
    CORSMiddleware,
    allow_origins=["https://yourwebsite.com", "https://app.yourwebsite.com"],
    allow_methods=["GET", "HEAD", "OPTIONS"],
    allow_headers=["*"],
    allow_credentials=False,
)
```

## Monitoring

The API includes:
- **Request timing middleware**: Adds `X-Process-Time` header to all responses
- **Health check endpoints**: For load balancer health checks
- **Automatic error handling**: Returns consistent error responses

## Load Testing

Use tools like Locust to test performance:

```python
from locust import HttpUser, task, between

class APIUser(HttpUser):
    wait_time = between(0.1, 0.5)

    @task(3)
    def get_team(self):
        self.client.get("/api/v1/teams/1?sport=NBA&season=2025")

    @task(2)
    def get_player(self):
        self.client.get("/api/v1/players/237?sport=NBA&season=2025")
```

Run: `locust -f locustfile.py --host=http://localhost:8000`
