# Scoracle Data

Sports statistics backend and seeding service for the Scoracle platform. Aggregates multi-sport data from external providers, normalizes it into a unified PostgreSQL schema, calculates percentile rankings, and serves it via a high-performance FastAPI API.

**Database:** PostgreSQL (Neon serverless). SQLite is no longer supported.

## Supported Sports

| Sport | Provider | Normalization | Season Format |
|-------|----------|---------------|---------------|
| **NBA** (Basketball) | BallDontLie | Per-36 minute | `2024-25` |
| **NFL** (American Football) | BallDontLie | Per-game | `2025` |
| **Football** (Soccer) | SportMonks | Per-90 minute | `2025` |

All sports share unified database tables (`players`, `player_stats`, `teams`, `team_stats`) differentiated by a `sport` column, with sport-specific data stored in JSONB columns.

## Architecture

```
                    ┌──────────────────────────────┐
                    │     FastAPI API Server        │
                    │  (profile, stats, news, twitter) │
                    └──────────────┬───────────────┘
                                   │ reads
                    ┌──────────────▼───────────────┐
                    │   PostgreSQL (Neon serverless) │
                    │   Views · Functions · Triggers │
                    └──────────────▲───────────────┘
                                   │ writes
               ┌───────────────────┼───────────────────┐
               │                   │                   │
    ┌──────────▼──┐     ┌─────────▼────┐    ┌────────▼───────┐
    │ NBA Seeder  │     │ NFL Seeder   │    │ Football Seeder│
    │ BallDontLie │     │ BallDontLie  │    │ SportMonks     │
    └─────────────┘     └──────────────┘    └────────────────┘
```

Python seeders and CLI tools write to Postgres. The API server reads from it. Neither depends on the other — both talk to Postgres independently. This separation enables a future migration of the API layer to Go (see [docs/GO_MIGRATION_GUIDE.md](docs/GO_MIGRATION_GUIDE.md)).

### Source Layout

```
src/scoracle_data/
├── api/                    # FastAPI application
│   ├── main.py             # App factory, middleware, health checks
│   ├── cache.py            # Two-tier caching (L1 in-memory + L2 Redis)
│   ├── dependencies.py     # Dependency injection
│   ├── errors.py           # Custom error handling
│   ├── rate_limit.py       # Rate limiting middleware
│   └── routers/            # Endpoint handlers
│       ├── profile.py      # GET /profile/{type}/{id}
│       ├── stats.py        # GET /stats/{type}/{id}
│       ├── news.py         # GET /news/{type}/{id}
│       └── twitter.py      # GET /twitter/journalist-feed
├── core/                   # Shared configuration and types
│   ├── config.py           # Pydantic Settings (DB, CORS, cache, rate limits)
│   ├── http.py             # BaseApiClient with retry logic and rate limiting
│   ├── models.py           # Response models
│   └── types.py            # Sport/EntityType enums, SPORT_REGISTRY
├── external/               # External API clients
│   ├── google_news.py      # Google News RSS
│   ├── news.py             # Unified news aggregation
│   └── twitter.py          # X/Twitter API
├── fixtures/               # Match schedule management
│   ├── loader.py           # Load fixtures from CSV/JSON
│   ├── scheduler.py        # Schedule-driven fixture seeding
│   └── post_match_seeder.py
├── migrations/             # SQL schema migrations (001–006)
├── percentiles/            # Percentile calculation engine
│   ├── config.py           # Stat categories, inverse stats
│   └── python_calculator.py # Pure Python calculator
├── providers/              # Data provider clients
│   ├── balldontlie_nba.py  # NBA stats from BallDontLie API
│   ├── balldontlie_nfl.py  # NFL stats from BallDontLie API
│   └── sportmonks.py       # Football stats from SportMonks API
├── seeders/                # Data seeding runners
│   ├── base.py             # BaseSeedRunner abstract class
│   ├── seed_nba.py         # NBA seeding logic
│   ├── seed_nfl.py         # NFL seeding logic
│   └── seed_football.py    # Football seeding logic
├── services/               # Business logic layer
│   ├── profiles.py         # Player/team profile queries
│   ├── stats.py            # Statistics and percentile queries
│   ├── news/service.py     # Unified news service (RSS + NewsAPI)
│   └── twitter/service.py  # Journalist feed service
├── pg_connection.py        # PostgreSQL connection pooling (psycopg3)
├── schema.py               # Database schema initialization
└── cli.py                  # CLI entry point
```

## API Endpoints

### Profile — `/api/v1/profile/{type}/{id}`

Entity profiles with biographical data and photos.

```
GET /api/v1/profile/player/123?sport=NBA
GET /api/v1/profile/team/456?sport=NFL
```

### Stats — `/api/v1/stats/{type}/{id}`

Entity statistics with percentile rankings.

```
GET /api/v1/stats/player/123?sport=NBA&season=2024-25
GET /api/v1/stats/team/456?sport=FOOTBALL
```

### News — `/api/v1/news/{type}/{id}`

Unified news from Google News RSS and NewsAPI.

```
GET /api/v1/news/player/123?sport=NBA&limit=10
GET /api/v1/news/team/456?sport=NFL&source=both
```

Parameters: `source` can be `rss` (default, free), `api` (NewsAPI), or `both` (merged).

### Twitter — `/api/v1/twitter/journalist-feed`

Curated journalist feed from X/Twitter Lists.

```
GET /api/v1/twitter/journalist-feed?q=LeBron&sport=NBA
```

### Health Checks

- `/health` — Basic liveness
- `/health/db` — Database connectivity
- `/health/cache` — Cache statistics
- `/health/rate-limit` — Rate limiter status

### Performance

- **msgspec** JSON serialization (4–5x faster than stdlib)
- Two-tier caching: L1 in-memory (LRU, 10K max entries) + L2 Redis (optional)
- GZIP compression for responses >1KB
- HTTP cache headers with `stale-while-revalidate`
- ETag support for cache validation
- Connection pool pre-warming on startup

## CLI Commands

```bash
scoracle-data init                      # Initialize database schema
scoracle-data status                    # Show database status
scoracle-data seed --sport nba          # Seed data from providers
scoracle-data percentiles --sport nba   # Recalculate percentiles
scoracle-data export                    # Export data to JSON
scoracle-data query leaders             # Top stat leaders
scoracle-data query standings           # League standings
scoracle-data query profile             # Entity profile details

# Fixture management
scoracle-data fixtures load             # Load match schedule
scoracle-data fixtures status           # Show fixture summary
scoracle-data fixtures pending          # Show ready-to-seed fixtures
scoracle-data fixtures upcoming         # Show upcoming fixtures
scoracle-data fixtures run-scheduler    # Process pending fixtures
```

## Percentile Engine

Pure Python percentile calculator (database-agnostic):

- Per-36 (NBA) and per-90 (Football) normalized statistics
- Position and league-based comparison groups
- Inverse stat handling (turnovers, fouls — lower is better)
- Results stored as JSONB within stats tables
- Historical snapshots in `percentile_archive` table

## Database Schema

Unified tables shared by all sports:

| Table | Purpose |
|-------|---------|
| `players` | Player profiles (name, position, photo, team) |
| `player_stats` | Per-season player stats + percentiles (JSONB) |
| `teams` | Team profiles (name, logo, league) |
| `team_stats` | Per-season team stats + percentiles (JSONB) |
| `leagues` | League metadata |
| `fixtures` | Match schedule |
| `stat_definitions` | Canonical stat name registry with display names |
| `percentile_archive` | Historical percentile snapshots |

SQL views and functions (`v_player_profile`, `v_team_profile`, `fn_stat_leaders`, `fn_standings`) handle data shaping, making the API layer a thin query-and-serialize layer. Derived stats (NBA per-36, NFL td_int_ratio, Football per-90) are computed by database triggers on insert/update.

Migrations are in `src/scoracle_data/migrations/` (001–006).

## Development

### Prerequisites

- Python 3.11+
- PostgreSQL (or Neon serverless account)
- [uv](https://github.com/astral-sh/uv) (recommended) or pip

### Environment Variables

Required:

- `NEON_DATABASE_URL_V2` — PostgreSQL connection string (fallback: `DATABASE_URL` > `NEON_DATABASE_URL`)
- `BALLDONTLIE_API_KEY` — BallDontLie API key (NBA/NFL data)
- `SPORTMONKS_API_TOKEN` — SportMonks API token (Football data)

Optional:

- `REDIS_URL` — Redis URL for L2 distributed caching
- `NEWS_API_KEY` — NewsAPI.org key for enhanced news
- `TWITTER_BEARER_TOKEN` — X/Twitter API bearer token
- `TWITTER_JOURNALIST_LIST_ID` — Curated journalist X List ID

### Quick Start

```bash
# Install dependencies
pip install -e ".[api,dev]"

# Initialize database
scoracle-data init

# Start API server (development)
uvicorn scoracle_data.api.main:app --reload

# Start API server (production)
gunicorn scoracle_data.api.main:app -w 4 -k uvicorn.workers.UvicornWorker

# Seed data
scoracle-data seed --sport nba

# Recalculate percentiles
scoracle-data percentiles --sport nba
```

### Testing

```bash
pytest tests/
```

Test fixtures are in `tests/fixtures/small_dataset.json` for offline validation without API calls.

### Deployment

Configured for Railway (`railway.toml`) and Heroku/Dokku (`Procfile`). The API runs on Uvicorn with Gunicorn as the process manager in production.

## Future Goals

- **Go API migration** — Replace FastAPI with a Go HTTP server for lower latency and reduced memory. The Postgres-centric architecture (views, functions, triggers) means Go handlers are thin query wrappers. See [docs/GO_MIGRATION_GUIDE.md](docs/GO_MIGRATION_GUIDE.md) and [docs/LANGUAGE_EVALUATION.md](docs/LANGUAGE_EVALUATION.md).
- **ML-powered analytics** — Transfer predictions, sentiment/vibe scores, player similarity, and performance forecasts using TensorFlow. Schema tables exist; model implementation is planned. See [docs/TENSORFLOW_ML_PLAN.md](docs/TENSORFLOW_ML_PLAN.md) and [docs/ML_IMPLEMENTATION_STATUS.md](docs/ML_IMPLEMENTATION_STATUS.md).

## Documentation

All documentation lives in the [`docs/`](docs/) directory:

| Document | Description |
|----------|-------------|
| [API_README.md](docs/API_README.md) | API endpoint reference and usage |
| [GO_MIGRATION_GUIDE.md](docs/GO_MIGRATION_GUIDE.md) | Go API migration guide |
| [LANGUAGE_EVALUATION.md](docs/LANGUAGE_EVALUATION.md) | Language/framework evaluation |
| [FRONTEND_INTEGRATION.md](docs/FRONTEND_INTEGRATION.md) | Frontend API integration |
| [BOOTSTRAP_FRONTEND.md](docs/BOOTSTRAP_FRONTEND.md) | Frontend autofill bootstrap |
| [STATS_API.md](docs/STATS_API.md) | Stats endpoint specification |
| [INTEL_API_PLAN.md](docs/INTEL_API_PLAN.md) | Intel/analytics API design |
| [TENSORFLOW_ML_PLAN.md](docs/TENSORFLOW_ML_PLAN.md) | ML implementation roadmap |
| [ML_IMPLEMENTATION_STATUS.md](docs/ML_IMPLEMENTATION_STATUS.md) | ML feature status |
| [INTELLIGENT_SEEDING_PLAN.md](docs/INTELLIGENT_SEEDING_PLAN.md) | Seeding architecture |
| [NEON_MIGRATION_PLAN.md](docs/NEON_MIGRATION_PLAN.md) | Neon database migration |
| [NEW_DATABASE_GUIDELINES.md](docs/NEW_DATABASE_GUIDELINES.md) | Database schema guidelines |
| [RESTRUCTURE_SUMMARY.md](docs/RESTRUCTURE_SUMMARY.md) | Architecture refactor summary |
