# Scoracle Data

Backend data pipeline for Scoracle. Seeds sports statistics from external APIs into a Neon PostgreSQL database, computes derived stats via Postgres triggers, and calculates percentile rankings in Python.

**Database:** PostgreSQL (Neon) only.

## Sports Covered

- **NBA** — Basketball statistics with per-36 minute normalization and True Shooting %
- **NFL** — American football with position-specific stat groupings
- **Football (Soccer)** — Top 5 European leagues with per-90 minute normalization

## Data Sources

| Sport | Provider | Handler |
|-------|----------|---------|
| NBA | [BallDontLie](https://balldontlie.io) | `handlers/balldontlie.py` — `BDLNBAHandler` |
| NFL | [BallDontLie](https://balldontlie.io) | `handlers/balldontlie.py` — `BDLNFLHandler` |
| Football | [SportMonks](https://sportmonks.com) | `handlers/sportmonks.py` — `SportMonksHandler` |

## How Seeding Works

The pipeline follows a **handler + seeder** pattern:

1. **Handlers** (`handlers/`) — Fetch data from external APIs and normalize responses into a canonical format. Each handler extends `BaseApiClient` (from `core/http.py`) and returns dicts with standardized keys (`provider_id`, `full_name`, `stats`, etc.).

2. **Seeders** (`seeders/`) — Provider-agnostic orchestration that takes normalized data from handlers and upserts it into Postgres. `BaseSeedRunner` handles NBA and NFL (identical flow). `FootballSeedRunner` extends it for football's per-league, per-team squad iteration.

3. **Derived stats** — Postgres triggers automatically compute per-36, per-90, TS%, win_pct, and other derived metrics on INSERT/UPDATE to `player_stats` and `team_stats`.

4. **Percentiles** — A pure Python calculator (`percentiles/python_calculator.py`) computes per-position percentile rankings and stores them in the `percentile_archive` table.

Player profiles are derived from stats responses (BallDontLie embeds full player data in each stats record), so there is no separate player-fetching step.

## Database Schema

Single consolidated schema in `schema.sql` (v6.0). No incremental migrations.

### Core Tables (11)

| Table | Purpose |
|-------|---------|
| `meta` | Key-value store for schema version and metadata |
| `sports` | Sport definitions (NBA, NFL, FOOTBALL) |
| `leagues` | League definitions with SportMonks IDs |
| `players` | Player profiles (all sports, unified) |
| `teams` | Team profiles (all sports, unified) |
| `player_stats` | Player statistics with JSONB `stats` column |
| `team_stats` | Team statistics with JSONB `stats` column |
| `stat_definitions` | Stat registry (display names, categories, inverse flags) |
| `provider_seasons` | Maps provider season IDs to year strings |
| `fixtures` | Match schedule for post-match seeding |
| `percentile_archive` | Stored percentile rankings by position group |

### Views

- `v_player_profile` — Joins players with their latest stats
- `v_team_profile` — Joins teams with their latest stats

### Triggers & Functions

- **Derived stats triggers** on `player_stats` and `team_stats` — auto-compute per-36, per-90, TS%, win_pct, etc.
- `resolve_provider_season_id()` — Maps provider season IDs
- `fn_stat_leaders()` / `fn_standings()` — Query helpers
- `recalculate_percentiles()` — Percentile recalculation entry point

### ML Tables

Machine learning features (transfer predictions, vibe scores, similarity engine, performance forecasts) are future concepts. The 13 ML tables that previously existed have been removed from the schema and dropped from the database.

## API

FastAPI server with:

- msgspec JSON serialization
- Two-tier caching (L1 in-memory + L2 Redis)
- Background cache warming
- GZIP compression for responses >1KB
- Rate limiting

### Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /api/v1/profile/{type}/{id}` | Player/team profiles |
| `GET /api/v1/stats/{type}/{id}` | Statistics with percentile rankings |
| `GET /api/v1/news/{type}/{id}` | Unified news (Google News RSS + NewsAPI) |
| `GET /api/v1/twitter/journalist-feed` | Curated journalist feed from X/Twitter Lists |
| `GET /health` | Health check |
| `GET /health/db` | Database connectivity |

## CLI Commands

```bash
# Data seeding
scoracle-data seed                     # Full batch seed from APIs
scoracle-data seed --sport nba         # Seed a single sport

# Percentiles
scoracle-data percentiles              # Recalculate all percentiles
scoracle-data percentiles --sport nba  # Single sport

# Fixtures
scoracle-data fixtures load            # Load match schedule
scoracle-data fixtures status          # Show fixture summary
scoracle-data fixtures pending         # Show ready-to-seed fixtures
scoracle-data fixtures run-scheduler   # Process pending fixtures

# Queries
scoracle-data query leaders            # Top stat leaders
scoracle-data query standings          # League standings
scoracle-data query profile            # Entity profile details

# Data export
scoracle-data export                   # Export data to JSON
scoracle-data export-profiles          # Export for frontend autocomplete
```

## Architecture

```
src/scoracle_data/
├── handlers/                  # API fetch + normalize to canonical format
│   ├── __init__.py            # extract_value() utility, exports
│   ├── balldontlie.py         # BDLNBAHandler, BDLNFLHandler
│   └── sportmonks.py          # SportMonksHandler
├── seeders/                   # Provider-agnostic DB orchestration
│   ├── base.py                # BaseSeedRunner (upserts, generic seed flow)
│   ├── football.py            # FootballSeedRunner (per-league/team iteration)
│   └── common.py              # SeedResult, BatchSeedResult
├── core/                      # Centralized configuration
│   ├── config.py              # Settings (pydantic-settings)
│   ├── http.py                # BaseApiClient (shared HTTP, rate limiting)
│   ├── models.py              # Response models
│   └── types.py               # SPORT_REGISTRY, table mappings
├── api/                       # FastAPI application
│   ├── main.py                # App entry, middleware, caching
│   ├── cache.py               # Two-tier cache (memory + Redis)
│   ├── rate_limit.py          # Rate limiting
│   └── routers/               # Endpoint handlers
│       ├── profile.py         # GET /profile/{type}/{id}
│       ├── stats.py           # GET /stats/{type}/{id}
│       ├── news.py            # GET /news/{type}/{id}
│       └── twitter.py         # GET /twitter/journalist-feed
├── services/                  # Business logic
│   ├── profiles.py            # Profile lookups
│   ├── stats.py               # Stats retrieval
│   ├── news/                  # Unified NewsService (RSS + NewsAPI)
│   └── twitter/               # TwitterService
├── percentiles/               # Percentile calculation engine
│   ├── python_calculator.py   # Pure Python calculator
│   └── config.py              # Stat categories, inverse stats
├── fixtures/                  # Post-match seeding
│   ├── loader.py              # Fixture schedule loading
│   ├── post_match_seeder.py   # Seed stats after matches complete
│   └── scheduler.py           # Background fixture processing
├── external/                  # External API clients
│   ├── google_news.py         # Google News RSS
│   ├── news.py                # NewsAPI
│   └── twitter.py             # X/Twitter API
├── pg_connection.py           # PostgreSQL connection with Neon pooling
├── schema.py                  # Schema management (applies schema.sql)
├── schema.sql                 # Consolidated database schema (v6.0)
└── cli.py                     # Command-line interface
```

## Environment Variables

**Required:**

- `NEON_DATABASE_URL_V2` — PostgreSQL connection string (Neon)
  - Fallback: `DATABASE_URL` > `NEON_DATABASE_URL`
- `BALLDONTLIE_API_KEY` — BallDontLie API key (NBA + NFL)
- `SPORTMONKS_API_TOKEN` — SportMonks API token (Football)

**Optional:**

- `REDIS_URL` — For distributed caching
- `NEWS_API_KEY` — NewsAPI.org for enhanced news
- `TWITTER_BEARER_TOKEN` — X/Twitter API for journalist feed
- `TWITTER_JOURNALIST_LIST_ID` — Curated journalist X List ID

## Quick Start

```bash
pip install -e .
scoracle-data seed --sport nba
scoracle-data percentiles --sport nba
uvicorn scoracle_data.api.main:app --reload
```
