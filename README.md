# Scoracle Data

Data seeding, statistics management, and machine learning backend for Scoracle.

## Features

### Multi-Sport Support

Sport-specific database schema with dedicated tables for each sport:

- **NBA** - Basketball statistics with per-36 minute normalization
- **NFL** - American football with position-specific stat groupings
- **Football (Soccer)** - European leagues with per-90 minute normalization

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

## API Endpoints

### Widget (`/api/v1/widget`)

Entity info and statistics for frontend widgets.

### ML (`/api/v1/ml`)

- Transfer predictions and trending transfers
- Vibe scores (sentiment analysis)
- Entity similarity and comparisons
- Game performance predictions
- Model accuracy metrics

### Intel (`/api/v1/intel`)

External data sources integration (requires API keys).

### News (`/api/v1/news`)

Google News RSS feeds for players and teams.

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

- `DATABASE_URL` (or `NEON_DATABASE_URL`) - PostgreSQL connection string
- `API_SPORTS_KEY` - API-Sports authentication

Optional:

- `REDIS_URL` - For distributed caching
- External service API keys for Intel endpoints

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

```text
src/scoracle_data/
├── api/                # FastAPI application
│   ├── main.py         # App entry, middleware, caching
│   ├── types.py        # Sport registry and configuration
│   └── routers/        # Endpoint handlers (widget, ml, intel, news)
├── ml/                 # Machine learning models
│   ├── models/         # TensorFlow model implementations
│   └── config.py       # ML configuration and source credibility tiers
├── percentiles/        # Percentile calculation engine
│   ├── python_calculator.py  # Pure Python calculator (active)
│   └── config.py       # Stat categories, min samples, inverse stats
├── seeders/            # Data population from API-Sports
├── migrations/         # Database schema migrations
├── external/           # External data sources (news, social)
├── queries/            # Query builders
├── models.py           # Pydantic models
└── cli.py              # Command-line interface
```

## Database Schema

### Sport-Specific Tables (v4.0)

Each sport has dedicated tables to prevent ID collisions:

- `{sport}_player_profiles` / `{sport}_team_profiles`
- `{sport}_player_stats` / `{sport}_team_stats`

### ML Tables

- `transfer_predictions`, `transfer_links`, `transfer_mentions`
- `vibe_scores`, `sentiment_samples`
- `entity_embeddings`, `entity_similarities`
- `performance_predictions`
- `ml_features`, `ml_models`, `ml_job_runs`

### Core Tables

- `sports`, `seasons`, `leagues`
- `players`, `teams`
- `percentile_archive`, `fixtures_schedule`

## Documentation

- [TENSORFLOW_ML_PLAN.md](docs/TENSORFLOW_ML_PLAN.md) - ML implementation roadmap
- [FRONTEND_INTEGRATION.md](docs/FRONTEND_INTEGRATION.md) - API usage for frontend
- [BOOTSTRAP_FRONTEND.md](docs/BOOTSTRAP_FRONTEND.md) - Frontend autofill integration
