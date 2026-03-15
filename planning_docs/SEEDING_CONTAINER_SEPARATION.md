# Seeding Container Separation Plan (Revised)

**Date:** 2026-03-15
**Status:** Approved

## Problem

The Go service conflates three concerns: API serving, data ingestion, and notification infrastructure. This causes role confusion, bloated context for AI agents, and deployment coupling.

## Philosophy

Each language/tool handles what it does best:

- **PostgreSQL** — Owns ALL data logic: normalization, derived stats, percentiles, change detection, event emission
- **Python** — Thin seeding layer: calls provider APIs, navigates JSON, inserts raw data, calls `finalize_fixture()`
- **Go** — Third-party integrations (news, Twitter) and reacts to Postgres events for notifications
- **PostgREST** — Serves data to frontend

Python has zero notification awareness. Go has zero seeding awareness. Postgres is the event boundary.

## Architecture

**Four containers, one database.**

```
Provider APIs (BDL, SportMonks)
         |
         v
+-------------------------+
|  Python Seeder          |  seed/
|  THIN: call API,        |  Navigate JSON, extract raw
|  key/value pairs,       |  INSERT into Postgres,
|  call finalize_fixture()|
+---------+---------------+
          | INSERT raw stats + finalize_fixture()
          v
+-------------------------+
|  PostgreSQL             |  ALL DATA WORK:
|  normalize_stat_keys()  |  -- stat key normalization (trigger)
|  compute_derived_*()    |  -- derived stats (existing triggers)
|  recalculate_pctiles()  |  -- percentiles (called by finalize)
|  finalize_fixture()     |  -- orchestrates post-seed pipeline
|  NOTIFY trigger         |  -- detects significant changes
+--+------------------+--+
   |                  | pg_notify('percentile_changed')
   v                  v
+----------+  +----------------+
| PostgREST|  |  Go API        |  INTEGRATIONS + NOTIFICATIONS:
| (:3000)  |  |  (:8000)       |  -- news, twitter feeds
| Stats API|  |                |  -- LISTEN percentile_changed
+----------+  |                |  -- follower lookup + FCM push
              |                |  -- maintenance tickers
              +----------------+
```

### Responsibility Boundaries

| Concern | Owner |
|---------|-------|
| Fetching fixture schedules from providers | Python seeder |
| Upserting fixture schedule into Postgres | Python seeder |
| Fetching game data from BDL/SportMonks | Python seeder |
| Navigating provider JSON responses | Python seeder |
| Upserting teams, players, stats (raw keys) | Python seeder |
| Calling `finalize_fixture()` when done | Python seeder |
| Normalizing stat keys (tov -> turnover) | Postgres trigger |
| Computing derived stats (per-36, TS%, win_pct) | Postgres triggers (existing) |
| Computing percentile rankings | Postgres function |
| Refreshing materialized views | Postgres (inside finalize_fixture) |
| Marking fixtures as seeded | Postgres (inside finalize_fixture) |
| Detecting significant percentile changes | Postgres trigger (enhanced) |
| Emitting change events | Postgres NOTIFY |
| Listening for percentile change events | Go API |
| Looking up followers, building messages | Go API |
| Scheduling delivery (timezone-aware) | Go API |
| Sending push notifications (FCM) | Go API |
| Serving news + Twitter feeds | Go API |
| Maintenance (cleanup, catch-up sweep) | Go API |
| Serving stats, profiles, standings, search | PostgREST |

## Fixture-Driven Seeding Model

All seeding is fixture-driven. No daily full seeds. No cron-based scheduling of seed runs.

```
Season start:
  1. scoracle-seed bootstrap-teams nba --season 2025  (one-time team roster)
  2. scoracle-seed load-fixtures nba --season 2025     (upsert all game dates)

Per fixture:
  status = 'scheduled', seed_after = start_time + seed_delay_hours

External cron (every 30 min):
  scoracle-seed process --max 50

  Internally:
    SELECT * FROM get_pending_fixtures()
    WHERE status IN ('scheduled','completed')
      AND NOW() >= start_time + seed_delay_hours
      AND seed_attempts < max_retries

    For each ready fixture group (sport, season, league_id):
      1. Fetch player/team stats from provider API
      2. Upsert players + player_stats + team_stats (raw provider keys)
      3. Call finalize_fixture(fixture_id)
         -> Postgres normalizes stat keys
         -> Postgres computes derived stats (existing triggers)
         -> Postgres calls recalculate_percentiles()
         -> Postgres refreshes materialized views
         -> Postgres marks fixture 'seeded'
         -> Percentile UPDATE trigger fires
         -> NOTIFY 'percentile_changed' for significant changes
```

## Postgres Changes

### New: `provider_stat_mappings` table

```sql
CREATE TABLE IF NOT EXISTS provider_stat_mappings (
    provider TEXT NOT NULL,
    sport TEXT NOT NULL,
    entity_type TEXT NOT NULL CHECK (entity_type IN ('player', 'team')),
    raw_key TEXT NOT NULL,
    canonical_key TEXT NOT NULL,
    PRIMARY KEY (provider, sport, entity_type, raw_key)
);
```

Populated with mappings ported from Go's `normalizeStatKeys()` and `normalizeCode()`.

### New: `normalize_stat_keys()` trigger function

Fires BEFORE INSERT OR UPDATE on player_stats and team_stats. Looks up each key in `provider_stat_mappings`, falls back to hyphen-to-underscore replacement. Runs before existing derived stat triggers (trigger naming ensures alphabetical ordering).

### New: `finalize_fixture()` function

```sql
CREATE OR REPLACE FUNCTION finalize_fixture(p_fixture_id INTEGER)
RETURNS TABLE (players_updated INTEGER, teams_updated INTEGER) AS $$
  -- 1. Look up fixture sport/season
  -- 2. Call recalculate_percentiles(sport, season)
  -- 3. REFRESH MATERIALIZED VIEW CONCURRENTLY mv_autofill_entities
  -- 4. Mark fixture as seeded via mark_fixture_seeded()
  -- 5. Return recalculation counts
$$
```

### New: `upsert_fixture()` function

For loading fixture schedules from provider APIs.

### Enhanced: `notify_percentile_changed()` trigger

Replaces existing `notify_milestone_reached()`. Uses OLD vs NEW to detect significant changes (milestone crossing at 90/95/99 or delta >= 10). Eliminates need for `percentile_archive` table and `archive_current_percentiles()` function in the notification flow.

### Deprecated

| Item | Status |
|------|--------|
| `archive_current_percentiles()` | No longer needed for notifications |
| `detect_percentile_changes()` | Replaced by trigger-level detection |
| `percentile_archive` table | Decoupled from notification flow (keep for analytics if desired) |

## Python Tech Stack

| Purpose | Library |
|---------|---------|
| HTTP client | `httpx` |
| DB driver | `psycopg[binary]` v3 |
| CLI | `click` |
| Config | stdlib `os.environ` + `dataclass` |
| Logging | stdlib `logging` |

No APScheduler, PyYAML, pydantic-settings, or structlog.

## Directory Structure

```
seed/
+-- pyproject.toml
+-- Dockerfile
+-- scoracle_seed/
|   +-- __init__.py
|   +-- cli.py               # Click CLI entry point
|   +-- config.py             # Env var loading, DB URL resolution
|   +-- db.py                 # psycopg pool, connection management
|   +-- models.py             # Canonical dataclasses (Team, Player, etc.)
|   +-- upsert.py             # INSERT ON CONFLICT functions
|   +-- fixtures.py           # Load schedule, poll pending, mark seeded
|   +-- bdl_client.py         # BDL HTTP client (rate limiting, pagination)
|   +-- bdl_nba.py            # NBA: teams, player stats, team stats
|   +-- bdl_nfl.py            # NFL: teams, player stats, standings
|   +-- sportmonks_client.py  # SportMonks HTTP client (rate limiting, 429 backoff)
|   +-- sportmonks_football.py # Football: teams, squads, standings
|   +-- seed_nba.py           # NBA fixture seed orchestration
|   +-- seed_nfl.py           # NFL fixture seed orchestration
|   +-- seed_football.py      # Football fixture seed orchestration
+-- tests/
    +-- test_models.py
    +-- test_bdl_normalize.py
    +-- test_sportmonks_normalize.py
```

## CLI Commands

```bash
# Season setup (manual, once per sport per season)
scoracle-seed bootstrap-teams nba --season 2025
scoracle-seed load-fixtures nba --season 2025
scoracle-seed load-fixtures football --season 2025 --league 8

# Ongoing (called by external cron every 30 min)
scoracle-seed process --max 50

# Manual operations
scoracle-seed seed-fixture --id 42
scoracle-seed percentiles --sport NBA --season 2025
```

## Go Changes

### Delete entirely (~2,996 lines)

| Directory | Lines | Contents |
|-----------|-------|----------|
| `go/cmd/ingest/` | 313 | Cobra CLI |
| `go/internal/seed/` | 507 | Seed orchestration + upserts |
| `go/internal/provider/` | 1,525 | Provider clients + normalization |
| `go/internal/fixture/` | 651 | Fixture processing |

### Modify

| File | Change |
|------|--------|
| `go/internal/listener/listener.go` | Listen on `percentile_changed`. Parse richer payload (old/new percentile). Write to `notifications` table with timezone-aware scheduling. |
| `go/internal/db/db.go` | Remove 10 seeding-only prepared statements. Keep notification + API statements. |
| `go/internal/config/config.go` | Remove `SportRegistry`, `BDLAPIKey`, `SportMonksAPIToken`, table name constants. |
| `go/cmd/api/main.go` | Remove imports of fixture, seed, provider. |

### Keep as-is

- `go/internal/notifications/` (dispatch worker, store, schedule, sender)
- `go/internal/maintenance/` (cleanup, catch-up sweep)
- `go/internal/api/handler/` (news, twitter)
- `go/internal/thirdparty/` (news, twitter clients)
- `go/internal/cache/`

## Docker Integration

### seed/Dockerfile

```dockerfile
FROM python:3.13-slim
WORKDIR /app
COPY pyproject.toml ./
RUN pip install --no-cache-dir .
COPY scoracle_seed/ scoracle_seed/
RUN pip install --no-cache-dir -e .
RUN adduser --disabled-password --uid 1000 scoracle
USER scoracle
ENTRYPOINT ["scoracle-seed"]
```

### docker-compose.yml addition

```yaml
seed:
  build: seed/
  env_file: .env
  environment:
    NEON_DATABASE_URL_V2: ${NEON_DATABASE_URL_V2:-}
    BALLDONTLIE_API_KEY: ${BALLDONTLIE_API_KEY:-}
    SPORTMONKS_API_TOKEN: ${SPORTMONKS_API_TOKEN:-}
  profiles: ["seed"]
```

Usage: `docker compose run --rm seed process --max 50`

### Scheduling

External cron (Railway cron job or system crontab):
```
*/30 * * * *  scoracle-seed process --max 50
```

## Implementation Phases

### Phase 0: Legacy cleanup
Delete `legacy_fastapi/`. Update AGENTS.md, CLAUDE.md, README.md references.

### Phase 1: Postgres normalization infrastructure
Add `provider_stat_mappings` table + data. Add `normalize_stat_keys()` trigger. Add `finalize_fixture()` function. Add `upsert_fixture()` function. Replace milestone trigger with enhanced `notify_percentile_changed()`.

### Phase 2: Python scaffold
Create `seed/` directory, `pyproject.toml`, `config.py`, `db.py`, `models.py`, CLI skeleton. Verify DB connectivity.

### Phase 3: Provider clients
Port BDL client + SportMonks client with rate limiting, pagination, 429 backoff.

### Phase 4: Sport handlers (thin)
Port BDL NBA/NFL and SportMonks Football handlers. Extract raw key/value pairs only -- no stat key normalization (Postgres handles that).

### Phase 5: Upsert + fixture management
Port upsert functions. Build fixture loading (new: fetch schedules from provider APIs). Build process command. Wire CLI.

### Phase 6: Docker + deployment
Dockerfile, docker-compose addition, railway.toml.

### Phase 7: Go listener update
Update listener for `percentile_changed` channel. Add notification scheduling.

### Phase 8: Go cleanup
Delete seeding code. Update db.go, config.go. Verify Go builds and tests pass.

## Known Challenges

1. **Fixture loading is new work.** No existing code fetches game schedules from BDL or SportMonks. Provider API endpoints need exploration.

2. **Trigger volume.** `recalculate_percentiles()` updates hundreds of rows. Each fires the NOTIFY trigger. Go listener must handle burst reception gracefully.

3. **Trigger execution order.** `normalize_stat_keys` must fire before sport-specific derived stat triggers. Naming convention (alphabetical ordering) or explicit sequencing required.

4. **First-season notification burst.** Initial percentile calculation may trigger many notifications. Consider a quiet-mode flag.

5. **SportMonks N+1 player fetch.** Each player requires an individual API call. Port faithfully first, optimize later.

6. **Unit conversion (cm/ft, kg/lbs).** Deliberately omitted from backend. Frontend handles display conversion.
