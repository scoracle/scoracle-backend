# Scoracle Data

Data seeding, statistics management, and machine learning backend for Scoracle.

**Database:** PostgreSQL (Neon) only. SQLite is no longer supported.

## Features

### Multi-Sport Support

Sport-specific database schema with dedicated tables for each sport:

- **NBA** - Basketball statistics with per-36 minute normalization
- **NFL** - American football with position-specific stat groupings
- **Football (Soccer)** - European leagues with per-90 minute normalization

Each sport is configured via TOML files in `sports/{sport}/config.toml`, enabling:
- Independent data providers per sport
- Sport-specific position groups for percentile comparison
- Customizable normalization (per-36, per-90, per-game)

### Machine Learning

TensorFlow-powered predictive analytics:

- **Transfer Predictor** - Player transfer probability with confidence intervals
- **Sentiment Analyzer** - "Vibe scores" from social media and news sources
- **Similarity Engine** - Find similar players/teams using 64-dim embeddings
- **Performance Predictor** - LSTM-based game performance forecasts

### Percentile Calculations

Pure Python percentile engine (database-agnostic):

- Per-36 (NBA) and per-90 (Football) normalized statistics
- Position and league-based comparison groups
- Football uses Top 5 European Leagues for comparisons
- Inverse stat handling (turnovers, fouls - lower is better)

### High-Performance API

FastAPI server with optimizations:

- msgspec JSON serialization (4-5x faster than stdlib)
- Two-tier caching (L1 in-memory + L2 Redis)
- Background cache warming every 30 minutes
- GZIP compression for responses >1KB
- Rate limiting with configurable limits

## API Endpoints

### Profile (`/api/v1/profile/{type}/{id}`)

Entity profiles with biographical data and photos.

```
GET /api/v1/profile/player/123?sport=NBA
GET /api/v1/profile/team/456?sport=NFL
```

### Stats (`/api/v1/stats/{type}/{id}`)

Entity statistics with percentile rankings.

```
GET /api/v1/stats/player/123?sport=NBA&season=2024-25
GET /api/v1/stats/team/456?sport=FOOTBALL
```

### News (`/api/v1/news/{type}/{id}`)

Unified news from Google News RSS and NewsAPI.

```
GET /api/v1/news/player/123?sport=NBA&limit=10
GET /api/v1/news/team/456?sport=NFL&source=both
```

Parameters:
- `source`: `rss` (default, free), `api` (NewsAPI), or `both` (merged)

### Twitter (`/api/v1/twitter/journalist-feed`)

Curated journalist feed from X/Twitter Lists.

```
GET /api/v1/twitter/journalist-feed?q=LeBron&sport=NBA
```

### ML (`/api/v1/ml`)

- `GET /ml/transfers/trending` - Trending transfer predictions
- `GET /ml/vibe/{type}/{id}` - Entity vibe score
- `GET /ml/similar/{type}/{id}` - Similar entities
- `GET /ml/predictions/{type}/{id}` - Performance predictions

### Health Checks

- `/health` - Basic health
- `/health/db` - Database connectivity
- `/health/cache` - Cache statistics
- `/health/rate-limit` - Rate limiter status

## CLI Commands

```bash
# Data seeding
seed                    # Full sync from API-Sports
seed-2phase             # Two-phase seeding (recommended)
seed-debug              # Limited seeding (5 teams, 5 players)
seed-small              # Fixture-based seeding (no API calls)

# Percentiles
percentiles             # Recalculate percentiles (pure Python)

# Data export
export                  # Export data to JSON
export-profiles         # Export for frontend autocomplete (sport-specific)

# Fixtures/scheduling
fixtures load           # Load match schedule
fixtures status         # Show fixture summary
fixtures pending        # Show ready-to-seed fixtures
fixtures run-scheduler  # Process pending fixtures

# Queries
query leaders           # Top stat leaders
query standings         # League standings
query profile           # Entity profile details

# Utilities
diff                    # Detect trades/transfers between rosters
```

## Development

### Environment Variables

Required:

- `NEON_DATABASE_URL_V2` - PostgreSQL connection string (recommended for v4.0 schema)
  - Fallback order: `NEON_DATABASE_URL_V2` > `DATABASE_URL` > `NEON_DATABASE_URL`
- `API_SPORTS_KEY` - API-Sports authentication

Optional:

- `REDIS_URL` - For distributed caching
- `NEWS_API_KEY` - NewsAPI.org for enhanced news
- `TWITTER_BEARER_TOKEN` - X/Twitter API for journalist feed
- `TWITTER_JOURNALIST_LIST_ID` - Curated journalist X List ID

### Quick Start

```bash
# Install dependencies
pip install -e .

# Run small dataset seeder (no API calls)
python -m scoracle_data.seeders.small_dataset_seeder

# Start API server
uvicorn scoracle_data.api.main:app --reload

# Recalculate percentiles
scoracle-data percentiles --sport nba
```

### Testing

Small dataset fixture for quick validation without API calls:

- Fixture: [tests/fixtures/small_dataset.json](tests/fixtures/small_dataset.json)
- Seeder: [src/scoracle_data/seeders/small_dataset_seeder.py](src/scoracle_data/seeders/small_dataset_seeder.py)

## Architecture

```
src/scoracle_data/
├── core/                   # Centralized configuration
│   ├── config.py           # Settings (pydantic-settings)
│   ├── models.py           # Response models
│   └── types.py            # Sport registry, table mappings (single source of truth)
├── db/                     # Database layer
│   └── __init__.py         # PostgresDB, repositories, get_db()
├── pg_connection.py        # PostgreSQL connection with Neon pooling
├── connection.py           # Backward-compatible StatsDB alias
├── services/               # Business logic services
│   ├── news/               # Unified NewsService (RSS + NewsAPI)
│   ├── twitter/            # TwitterService for journalist feed
│   └── percentiles/        # PercentileService wrapper
├── sports/                 # Sport-specific configuration
│   ├── registry.py         # TOML config loader
│   ├── nba/config.toml     # NBA: API-Sports, per-36, position groups
│   ├── nfl/config.toml     # NFL: API-Sports, per-game, positions
│   └── football/config.toml # Football: API-Sports, per-90, Top 5 leagues
├── providers/              # Data provider abstraction
│   ├── base.py             # DataProviderProtocol
│   └── api_sports.py       # API-Sports implementation
├── api/                    # FastAPI application
│   ├── main.py             # App entry, middleware, caching
│   └── routers/            # Endpoint handlers
│       ├── profile.py      # GET /profile/{type}/{id}
│       ├── stats.py        # GET /stats/{type}/{id}
│       ├── news.py         # GET /news/{type}/{id}
│       ├── twitter.py      # GET /twitter/journalist-feed
│       └── ml.py           # ML endpoints
├── ml/                     # Machine learning models
│   ├── models/             # TensorFlow implementations
│   ├── jobs/               # Background ML jobs
│   └── config.py           # ML configuration
├── percentiles/            # Percentile calculation engine
│   ├── python_calculator.py # Pure Python calculator
│   └── config.py           # Stat categories, inverse stats
├── seeders/                # Data population from API-Sports
├── migrations/             # Database schema migrations
├── external/               # External API clients (Twitter, News)
└── cli.py                  # Command-line interface
```

## Database Schema

### Sport-Specific Tables (v4.0)

Each sport has dedicated tables to prevent cross-sport ID collisions:

**Profile Tables:**
- `nba_player_profiles` / `nba_team_profiles`
- `nfl_player_profiles` / `nfl_team_profiles`
- `football_player_profiles` / `football_team_profiles`

**Stats Tables:**
- `nba_player_stats` / `nba_team_stats`
- `nfl_player_stats` / `nfl_team_stats`
- `football_player_stats` / `football_team_stats`

### Unified Views

For backward compatibility, views aggregate all sport-specific profile tables:

- `players` - View combining all `*_player_profiles` tables with `sport_id` column
- `teams` - View combining all `*_team_profiles` tables with `sport_id` column

These views allow legacy queries like `SELECT * FROM players WHERE sport_id = 'NBA'` to work seamlessly.

### ML Tables

- `transfer_predictions`, `transfer_links`, `transfer_mentions`
- `vibe_scores`, `sentiment_samples`
- `entity_embeddings`, `entity_similarities`
- `performance_predictions`
- `ml_features`, `ml_models`, `ml_job_runs`

### Core Tables

- `sports`, `seasons`, `leagues`
- `percentile_archive`, `fixtures_schedule`

## Documentation

- [TENSORFLOW_ML_PLAN.md](docs/TENSORFLOW_ML_PLAN.md) - ML implementation roadmap
- [FRONTEND_INTEGRATION.md](docs/FRONTEND_INTEGRATION.md) - API usage for frontend
- [BOOTSTRAP_FRONTEND.md](docs/BOOTSTRAP_FRONTEND.md) - Frontend autofill integration
