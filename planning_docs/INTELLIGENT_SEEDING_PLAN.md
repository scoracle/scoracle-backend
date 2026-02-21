# Intelligent Schedule-Driven Seeding System
## Comprehensive Implementation Plan

**Last Updated:** 2026-01-07
**Target Implementation:** Q1 2026
**Status:** Planning Phase

---

## Executive Summary

This document outlines a comprehensive plan to transform the current blanket seeding approach into an intelligent, game-schedule-driven system that:

1. **Maximizes API Efficiency** - Only seeds entities that have played games
2. **Delivers Same-Day Data** - Stats available within hours of game completion
3. **Minimizes Wasted Calls** - No blanket updates for inactive players/teams
4. **Scales Across Sports** - Unified architecture for NBA, NFL, Football/Soccer
5. **Remains Maintainable** - Manual schedule ingestion, automated execution

---

## Table of Contents

1. [Current State Analysis](#1-current-state-analysis)
2. [System Architecture](#2-system-architecture)
3. [Component Design](#3-component-design)
4. [Database Schema Extensions](#4-database-schema-extensions)
5. [Implementation Phases](#5-implementation-phases)
6. [API Optimization Strategies](#6-api-optimization-strategies)
7. [Schedule Ingestion Workflow](#7-schedule-ingestion-workflow)
8. [Seeding Execution Logic](#8-seeding-execution-logic)
9. [Error Handling & Monitoring](#9-error-handling--monitoring)
10. [Testing Strategy](#10-testing-strategy)
11. [Deployment & Operations](#11-deployment--operations)

---

## 1. Current State Analysis

### 1.1 Existing Infrastructure

**Strengths:**
- ✅ Well-architected three-phase seeding (Discovery → Profile → Stats)
- ✅ Async/await support for concurrent API calls
- ✅ PostgreSQL with proper indexing and UPSERT patterns
- ✅ Profile tracking (`profile_fetched_at`) to avoid redundant fetches
- ✅ Sync logging for audit trails
- ✅ Multi-sport support (NBA, NFL, Football)

**Gaps:**
- ❌ No game schedule tracking
- ❌ No targeted seeding based on game participation
- ❌ No scheduling/orchestration layer
- ❌ Blanket updates waste API calls on inactive entities
- ❌ No game-time awareness for optimal seeding windows

### 1.2 Current Seeding Inefficiencies

| Sport | Total Teams | Total Players | Games/Season | Current Approach | Waste Factor |
|-------|-------------|---------------|--------------|------------------|--------------|
| NBA | 30 | ~450 active | 1,230 | Daily blanket seed | ~70% (inactive days) |
| NFL | 32 | ~1,800 active | 272 | Weekly blanket seed | ~60% (bye weeks) |
| Football | 120 teams (6 leagues) | ~3,000 | ~2,500 | Daily blanket seed | ~85% (midweek) |

**Problem:** Seeding all entities regardless of game schedule results in:
- Wasted API calls on players who didn't play
- Delayed data delivery (waiting for full seed cycles)
- Inefficient resource usage

---

## 2. System Architecture

### 2.1 High-Level Design

```
┌─────────────────────────────────────────────────────────────┐
│                   INTELLIGENT SEEDING SYSTEM                 │
└─────────────────────────────────────────────────────────────┘

┌──────────────────┐
│  MANUAL INPUT    │  User provides league schedules
│  League Schedules│  (CSV, JSON, or web scraping)
└────────┬─────────┘
         │
         ↓
┌────────────────────────────────────────────────────────────┐
│  SCHEDULE INGESTION SERVICE                                │
│  - Parse schedule files (CSV/JSON)                         │
│  - Extract: game_date, home_team, away_team, start_time   │
│  - Store in `games` table                                  │
│  - Generate seeding tasks (game_time + 4 hours)            │
└────────┬───────────────────────────────────────────────────┘
         │
         ↓
┌────────────────────────────────────────────────────────────┐
│  SEEDING SCHEDULER (APScheduler or Celery)                 │
│  - Job queue: scheduled seeding tasks                      │
│  - Trigger: 4 hours after each game start time             │
│  - Payload: [team_ids, player_ids] for that game           │
└────────┬───────────────────────────────────────────────────┘
         │
         ↓
┌────────────────────────────────────────────────────────────┐
│  TARGETED SEEDING ENGINE                                   │
│  - Fetch stats ONLY for participating entities             │
│  - Use existing seeders with entity filtering              │
│  - Update `last_seeded_at` timestamp                       │
│  - Log results to `sync_log`                               │
└────────┬───────────────────────────────────────────────────┘
         │
         ↓
┌────────────────────────────────────────────────────────────┐
│  POSTGRESQL DATABASE                                       │
│  - games (schedule + metadata)                             │
│  - seeding_tasks (scheduled jobs)                          │
│  - players/teams (stats tables)                            │
│  - sync_log (audit trail)                                  │
└────────────────────────────────────────────────────────────┘
```

### 2.2 Data Flow

```
1. SCHEDULE INGESTION (Manual, as-needed)
   Input: League schedule file
   Output: `games` table populated with season schedule

2. TASK GENERATION (Automatic, post-ingestion)
   Input: New games in `games` table
   Output: `seeding_tasks` entries (game_time + 4 hours)

3. SCHEDULER EXECUTION (Automatic, continuous)
   Trigger: Task due time reached
   Action: Execute targeted seeding for game participants

4. TARGETED SEEDING (Automatic, triggered)
   Input: [team_ids, player_ids] from game roster
   Action: Fetch stats from API-Sports
   Output: Updated stats in database

5. MONITORING (Automatic, continuous)
   Track: Success/failure rates, API usage, latency
   Alert: Failed seeds, API quota warnings
```

---

## 3. Component Design

### 3.1 Schedule Ingestion Service

**File:** `python/scoracle_data/schedulers/schedule_ingester.py`

**Responsibilities:**
- Parse schedule files (CSV, JSON, or API response)
- Map team names/IDs to database entities
- Validate game dates and times
- Insert games into `games` table
- Generate corresponding seeding tasks

**Input Formats Supported:**

**CSV Format:**
```csv
date,time,home_team,away_team,league,season
2026-01-15,19:30,Lakers,Warriors,NBA,2025
2026-01-15,20:00,Celtics,Heat,NBA,2025
```

**JSON Format:**
```json
{
  "sport": "NBA",
  "season": "2025",
  "games": [
    {
      "date": "2026-01-15",
      "time": "19:30",
      "home_team": "Lakers",
      "away_team": "Warriors",
      "venue": "Crypto.com Arena"
    }
  ]
}
```

**Key Methods:**
```python
class ScheduleIngester:
    async def ingest_from_csv(self, file_path: str, sport: str, season: str)
    async def ingest_from_json(self, file_path: str)
    async def map_team_name_to_id(self, team_name: str, sport: str) -> int
    async def create_game_entry(self, game_data: dict) -> int
    async def generate_seeding_task(self, game_id: int, game_start_time: datetime)
```

### 3.2 Seeding Scheduler

**File:** `python/scoracle_data/schedulers/seeding_scheduler.py`

**Technology Options:**

**Option A: APScheduler** (Recommended for MVP)
- Pure Python, no external dependencies
- In-process or persistent job stores (PostgreSQL)
- Supports cron, interval, and date-based triggers
- Easy to integrate with existing async codebase

**Option B: Celery + Redis**
- Production-grade task queue
- Better for distributed systems
- Requires Redis/RabbitMQ infrastructure
- Overkill for single-service deployment

**Recommendation:** Start with **APScheduler** using PostgreSQL job store.

**Implementation:**
```python
from apscheduler.schedulers.asyncio import AsyncIOScheduler
from apscheduler.jobstores.sqlalchemy import SQLAlchemyJobStore

class SeedingScheduler:
    def __init__(self, db_url: str):
        jobstores = {
            'default': SQLAlchemyJobStore(url=db_url)
        }
        self.scheduler = AsyncIOScheduler(jobstores=jobstores)

    async def schedule_game_seeding(
        self,
        game_id: int,
        game_start_time: datetime,
        team_ids: list[int],
        player_ids: list[int]
    ):
        """Schedule seeding 4 hours after game start"""
        seed_time = game_start_time + timedelta(hours=4)

        self.scheduler.add_job(
            self.execute_targeted_seed,
            'date',
            run_date=seed_time,
            args=[game_id, team_ids, player_ids],
            id=f'seed_game_{game_id}',
            replace_existing=True
        )

    async def execute_targeted_seed(
        self,
        game_id: int,
        team_ids: list[int],
        player_ids: list[int]
    ):
        """Execute targeted seeding for game participants"""
        # Implementation in next section
        pass
```

### 3.3 Targeted Seeding Engine

**File:** `python/scoracle_data/seeders/targeted_seeder.py`

**Design Philosophy:**
- Extend existing `BaseSeeder` architecture
- Override `fetch_players()` and `fetch_teams()` to accept entity filters
- Reuse transformation logic and database upserts
- Add game-context awareness

**Implementation:**
```python
class TargetedSeeder(BaseSeeder):
    async def seed_game_participants(
        self,
        game_id: int,
        team_ids: list[int],
        player_ids: list[int],
        fetch_profiles: bool = False
    ):
        """Seed stats for specific teams and players"""

        # 1. Fetch team stats
        for team_id in team_ids:
            team_data = await self.api_client.get_team_stats(
                sport=self.sport,
                season=self.season,
                team_id=team_id
            )
            await self.upsert_team_stats(team_data)

        # 2. Fetch player stats
        for player_id in player_ids:
            player_data = await self.api_client.get_player_stats(
                sport=self.sport,
                season=self.season,
                player_id=player_id
            )
            await self.upsert_player_stats(player_data)

        # 3. Update game seeding status
        await self.mark_game_seeded(game_id)

        # 4. Log sync operation
        await self.log_sync(
            operation='targeted_seed',
            context={'game_id': game_id},
            team_count=len(team_ids),
            player_count=len(player_ids)
        )
```

### 3.4 Roster Resolution Service

**Challenge:** Need to know which players participated in each game.

**Solutions:**

**Option 1: Pre-fetch Active Rosters (Recommended)**
- During schedule ingestion, fetch current team rosters
- Store in `game_rosters` junction table
- Assumption: Players on roster are likely to play
- Pro: No additional API calls at seed time
- Con: May include inactive players (bench warmers)

**Option 2: Game Boxscore API (Most Accurate)**
- 4 hours after game, fetch boxscore to get actual participants
- Only seed players who played
- Pro: Zero wasted calls
- Con: Requires additional API call per game

**Option 3: Hybrid Approach**
- Start with roster-based seeding (Option 1)
- Add boxscore validation for high-priority games
- Gradually refine based on API quota

**Recommendation:** Start with **Option 1**, migrate to **Option 2** once stable.

**Implementation (Option 1):**
```python
class RosterResolver:
    async def resolve_game_participants(
        self,
        team_home_id: int,
        team_away_id: int,
        season: str
    ) -> tuple[list[int], list[int]]:
        """
        Returns (team_ids, player_ids) for a game
        """
        team_ids = [team_home_id, team_away_id]

        # Fetch active players for both teams
        player_ids = await self.db.query(
            """
            SELECT id FROM players
            WHERE current_team_id IN (%s, %s)
            AND season = %s
            """,
            (team_home_id, team_away_id, season)
        )

        return team_ids, player_ids
```

---

## 4. Database Schema Extensions

### 4.1 New Tables

#### `games` Table
Stores league schedules for all sports.

```sql
CREATE TABLE games (
    id SERIAL PRIMARY KEY,
    sport_id INTEGER NOT NULL REFERENCES sports(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    league_id INTEGER REFERENCES leagues(id),  -- For Football multi-league

    -- Game identification
    external_game_id VARCHAR(50),  -- API-Sports game ID (if available)
    game_date DATE NOT NULL,
    game_time TIME,
    game_datetime TIMESTAMP WITH TIME ZONE NOT NULL,  -- Combined date+time in UTC

    -- Teams
    home_team_id INTEGER NOT NULL REFERENCES teams(id),
    away_team_id INTEGER NOT NULL REFERENCES teams(id),

    -- Game metadata
    venue_name VARCHAR(200),
    week INTEGER,  -- For NFL
    game_type VARCHAR(20),  -- 'regular', 'playoff', 'preseason'
    status VARCHAR(20) DEFAULT 'scheduled',  -- 'scheduled', 'in_progress', 'completed', 'postponed'

    -- Seeding tracking
    seeded_at TIMESTAMP WITH TIME ZONE,
    seeding_status VARCHAR(20) DEFAULT 'pending',  -- 'pending', 'in_progress', 'completed', 'failed'
    seeding_attempts INTEGER DEFAULT 0,
    last_seeding_error TEXT,

    -- Timestamps
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),

    -- Indexes
    CONSTRAINT unique_game UNIQUE(sport_id, season_id, game_date, home_team_id, away_team_id)
);

CREATE INDEX idx_games_datetime ON games(game_datetime);
CREATE INDEX idx_games_sport_season ON games(sport_id, season_id);
CREATE INDEX idx_games_seeding_status ON games(seeding_status) WHERE seeding_status != 'completed';
CREATE INDEX idx_games_teams ON games(home_team_id, away_team_id);
```

#### `game_rosters` Table (Junction)
Links games to participating players (pre-resolved rosters).

```sql
CREATE TABLE game_rosters (
    id SERIAL PRIMARY KEY,
    game_id INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    player_id INTEGER NOT NULL REFERENCES players(id),
    team_id INTEGER NOT NULL REFERENCES teams(id),

    -- Player status
    is_starter BOOLEAN DEFAULT false,
    is_injured BOOLEAN DEFAULT false,
    status VARCHAR(20),  -- 'active', 'inactive', 'injured', 'dnp'

    -- Timestamps
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),

    CONSTRAINT unique_game_player UNIQUE(game_id, player_id)
);

CREATE INDEX idx_game_rosters_game ON game_rosters(game_id);
CREATE INDEX idx_game_rosters_player ON game_rosters(player_id);
```

#### `seeding_tasks` Table
Tracks scheduled and executed seeding jobs.

```sql
CREATE TABLE seeding_tasks (
    id SERIAL PRIMARY KEY,
    game_id INTEGER NOT NULL REFERENCES games(id) ON DELETE CASCADE,

    -- Scheduling
    scheduled_time TIMESTAMP WITH TIME ZONE NOT NULL,
    execution_time TIMESTAMP WITH TIME ZONE,

    -- Execution metadata
    status VARCHAR(20) DEFAULT 'pending',  -- 'pending', 'running', 'completed', 'failed', 'cancelled'
    team_count INTEGER,
    player_count INTEGER,
    api_calls_made INTEGER,
    duration_seconds NUMERIC(8,2),

    -- Error tracking
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    max_retries INTEGER DEFAULT 3,
    next_retry_at TIMESTAMP WITH TIME ZONE,

    -- Sync log reference
    sync_log_id INTEGER REFERENCES sync_log(id),

    -- Timestamps
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),

    CONSTRAINT unique_game_task UNIQUE(game_id)
);

CREATE INDEX idx_seeding_tasks_scheduled ON seeding_tasks(scheduled_time) WHERE status = 'pending';
CREATE INDEX idx_seeding_tasks_status ON seeding_tasks(status);
CREATE INDEX idx_seeding_tasks_game ON seeding_tasks(game_id);
```

### 4.2 Schema Modifications

#### Add to `players` table:
```sql
ALTER TABLE players ADD COLUMN last_seeded_at TIMESTAMP WITH TIME ZONE;
ALTER TABLE players ADD COLUMN last_game_date DATE;  -- Track last known game appearance
CREATE INDEX idx_players_last_seeded ON players(last_seeded_at);
```

#### Add to `teams` table:
```sql
ALTER TABLE teams ADD COLUMN last_seeded_at TIMESTAMP WITH TIME ZONE;
ALTER TABLE teams ADD COLUMN last_game_date DATE;
CREATE INDEX idx_teams_last_seeded ON teams(last_seeded_at);
```

#### Add to `sync_log` table:
```sql
ALTER TABLE sync_log ADD COLUMN game_id INTEGER REFERENCES games(id);
ALTER TABLE sync_log ADD COLUMN seeding_type VARCHAR(20);  -- 'blanket', 'targeted', 'discovery'
CREATE INDEX idx_sync_log_game ON sync_log(game_id);
```

---

## 5. Implementation Phases

### Phase 1: Foundation (Week 1-2)
**Goal:** Database schema and basic ingestion

**Tasks:**
1. ✅ Create migration script for new tables (`games`, `game_rosters`, `seeding_tasks`)
2. ✅ Add schema modifications to existing tables
3. ✅ Build `ScheduleIngester` class
   - CSV parser
   - JSON parser
   - Team name → ID mapping
   - Game entry creation
4. ✅ Create CLI command: `python -m scoracle_data.cli ingest-schedule --file schedule.csv --sport NBA`
5. ✅ Write unit tests for ingestion logic

**Deliverables:**
- Migration: `008_game_scheduling.sql`
- Module: `python/scoracle_data/schedulers/schedule_ingester.py`
- CLI command: `ingest-schedule`
- Tests: `tests/test_schedule_ingestion.py`

**Success Criteria:**
- Can ingest full NBA season schedule (1,230 games)
- Team mapping works for all 30 NBA teams
- Games table populated with correct timezone handling

---

### Phase 2: Scheduler Integration (Week 3-4)
**Goal:** Implement APScheduler with PostgreSQL job store

**Tasks:**
1. ✅ Install APScheduler: `pip install apscheduler`
2. ✅ Build `SeedingScheduler` class
   - Initialize with PostgreSQL job store
   - Schedule job creation (game_time + 4 hours)
   - Job persistence across restarts
3. ✅ Create scheduler daemon/service
   - Long-running process or systemd service
   - Graceful shutdown handling
   - Signal handling (SIGTERM, SIGINT)
4. ✅ Build `RosterResolver` service
   - Fetch team rosters
   - Populate `game_rosters` table
5. ✅ Create CLI commands:
   - `python -m scoracle_data.cli scheduler start`
   - `python -m scoracle_data.cli scheduler stop`
   - `python -m scoracle_data.cli scheduler status`

**Deliverables:**
- Module: `python/scoracle_data/schedulers/seeding_scheduler.py`
- Module: `python/scoracle_data/schedulers/roster_resolver.py`
- Service: `schedulers/scheduler_daemon.py`
- CLI commands: `scheduler start|stop|status`
- Tests: `tests/test_scheduler.py`

**Success Criteria:**
- Scheduler persists jobs across process restarts
- Jobs trigger at correct times (UTC timezone handling)
- Can handle 100+ scheduled jobs simultaneously

---

### Phase 3: Targeted Seeding (Week 5-6)
**Goal:** Implement smart seeding that only targets game participants

**Tasks:**
1. ✅ Build `TargetedSeeder` class extending `BaseSeeder`
   - Override methods to accept entity filters
   - Add game-context tracking
   - Implement `seed_game_participants()` method
2. ✅ Modify existing sport seeders to support filtering:
   - `NBASeeder.fetch_team_stats(team_ids: list[int] | None)`
   - `NFLSeeder.fetch_player_stats(player_ids: list[int] | None)`
   - `FootballSeeder` (same pattern)
3. ✅ Integrate with scheduler:
   - Scheduler calls `TargetedSeeder.seed_game_participants()`
   - Pass team_ids and player_ids from `game_rosters`
4. ✅ Implement seeding status tracking:
   - Update `games.seeding_status`
   - Update `seeding_tasks.status`
   - Update `players.last_seeded_at` and `teams.last_seeded_at`
5. ✅ Add retry logic for failed seeds
   - Exponential backoff
   - Max retries configuration
   - Error logging

**Deliverables:**
- Module: `python/scoracle_data/seeders/targeted_seeder.py`
- Modified: NBA/NFL/Football seeders (add filtering support)
- Tests: `tests/test_targeted_seeding.py`

**Success Criteria:**
- Can seed a single game's participants (2 teams, ~30 players) in <30 seconds
- Failed seeds automatically retry with backoff
- API call reduction of 70%+ compared to blanket seeding

---

### Phase 4: Monitoring & Observability (Week 7)
**Goal:** Build dashboards and alerting for system health

**Tasks:**
1. ✅ Create monitoring queries:
   - Upcoming seeds (next 24 hours)
   - Failed seeds (last 7 days)
   - API usage stats (calls per day)
   - Seeding latency (scheduled vs executed time)
2. ✅ Build CLI monitoring commands:
   - `python -m scoracle_data.cli monitor upcoming`
   - `python -m scoracle_data.cli monitor failures`
   - `python -m scoracle_data.cli monitor stats`
3. ✅ Implement health check endpoint (if running as service):
   - Scheduler status
   - Database connectivity
   - Last successful seed
4. ✅ Create alerting logic:
   - Email/Slack on repeated failures
   - API quota warnings (approaching limit)
   - Scheduler downtime alerts

**Deliverables:**
- Module: `python/scoracle_data/monitoring/metrics.py`
- CLI commands: `monitor upcoming|failures|stats`
- Alerting: `python/scoracle_data/monitoring/alerts.py`
- Documentation: `docs/MONITORING.md`

**Success Criteria:**
- Can identify failed seeds within 5 minutes
- API usage trends visible over time
- Alerting catches 100% of scheduler crashes

---

### Phase 5: Multi-Sport Expansion (Week 8-9)
**Goal:** Expand from NBA to NFL and Football/Soccer

**Tasks:**
1. ✅ Ingest NFL schedule (272 games, 18 weeks)
   - Handle bye weeks
   - Handle Thursday/Monday night games
2. ✅ Ingest Football schedules (6 priority leagues)
   - Premier League, La Liga, Bundesliga, Serie A, Ligue 1, MLS
   - Handle midweek fixtures (Champions League, domestic cups)
   - League-specific calendar variations
3. ✅ Test targeted seeding across all sports
   - Validate API endpoints work with filtering
   - Verify transformation logic handles targeted data
4. ✅ Optimize scheduler for high game volume:
   - Football can have 50+ games in a single day
   - Batch processing for concurrent games
   - Resource throttling to avoid API rate limits

**Deliverables:**
- NFL schedule ingestion working
- Football schedule ingestion working (all 6 leagues)
- Batch seeding optimization
- Tests: `tests/test_multi_sport_seeding.py`

**Success Criteria:**
- Can handle 50+ concurrent game seeds without failures
- API rate limits never exceeded
- All sports maintain <5% failure rate

---

### Phase 6: Production Hardening (Week 10)
**Goal:** Production-ready deployment

**Tasks:**
1. ✅ Add configuration management:
   - Environment-specific configs (dev, staging, prod)
   - Secrets management (API keys, DB credentials)
   - Feature flags (enable/disable targeted seeding per sport)
2. ✅ Implement graceful degradation:
   - Fall back to blanket seeding if targeted fails
   - Partial success handling (some players seed, others fail)
3. ✅ Add comprehensive logging:
   - Structured JSON logs for parsing
   - Log levels (DEBUG, INFO, WARNING, ERROR)
   - Correlation IDs for request tracing
4. ✅ Performance optimization:
   - Connection pooling for database
   - HTTP connection reuse for API calls
   - Concurrent async processing with semaphores
5. ✅ Create deployment guide:
   - Docker container setup
   - Kubernetes manifests (if applicable)
   - Systemd service file
   - Environment variable documentation

**Deliverables:**
- Configuration: `config/production.yaml`
- Dockerfile and docker-compose.yaml
- Deployment guide: `docs/DEPLOYMENT.md`
- Runbook: `docs/RUNBOOK.md`

**Success Criteria:**
- 99% uptime over 30-day test period
- Zero data loss from failures
- Recovery from crashes without manual intervention

---

## 6. API Optimization Strategies

### 6.1 API Call Reduction Analysis

**Current Blanket Approach:**
```
NBA Example (Daily):
- 30 teams × 1 call = 30 calls
- 450 players × 1 call = 450 calls
Total: 480 calls/day × 180 days = 86,400 calls/season
```

**Targeted Approach:**
```
NBA Example (Daily, avg 12 games/day):
- 24 teams × 1 call = 24 calls (only teams playing)
- ~300 players × 1 call = 300 calls (only active rosters)
Total: 324 calls/day × 180 days = 58,320 calls/season
Savings: 32.7% reduction
```

**Even Better: Active Players Only (with boxscore API):**
```
NBA Example (Daily, avg 12 games/day):
- 24 teams × 1 call = 24 calls
- ~150 players × 1 call = 150 calls (only players who played)
Total: 174 calls/day × 180 days = 31,320 calls/season
Savings: 63.7% reduction
```

### 6.2 Rate Limiting Strategy

**API-Sports Rate Limits:**
- Free tier: 100 calls/day (not viable)
- Pro tier: 1,000 calls/day
- Ultra tier: 10,000 calls/day

**Recommendation:** Ultra tier for production (3 sports × 500 avg calls/day = 1,500 calls/day)

**Implementation:**
```python
from asyncio import Semaphore

class RateLimitedApiClient:
    def __init__(self, max_calls_per_second: int = 10):
        self.semaphore = Semaphore(max_calls_per_second)
        self.call_times = []

    async def make_call(self, endpoint: str, params: dict):
        async with self.semaphore:
            await self._ensure_rate_limit()
            response = await self.client.get(endpoint, params=params)
            self.call_times.append(time.time())
            return response

    async def _ensure_rate_limit(self):
        """Ensure we don't exceed rate limits"""
        now = time.time()
        # Remove calls older than 1 second
        self.call_times = [t for t in self.call_times if now - t < 1.0]

        if len(self.call_times) >= 10:
            sleep_time = 1.0 - (now - self.call_times[0])
            if sleep_time > 0:
                await asyncio.sleep(sleep_time)
```

### 6.3 Caching Strategy

**What to Cache:**
- Team rosters (cache for 24 hours, invalidate on trades)
- Player profiles (cache indefinitely, update on-demand)
- League schedules (cache for full season)

**What NOT to Cache:**
- Player/team stats (always fetch fresh)
- Live game data

**Implementation:**
```python
from functools import lru_cache
from datetime import datetime, timedelta

class CachedApiClient:
    def __init__(self):
        self.roster_cache = {}  # {team_id: (roster, expiry)}

    async def get_team_roster(self, team_id: int):
        # Check cache
        if team_id in self.roster_cache:
            roster, expiry = self.roster_cache[team_id]
            if datetime.now() < expiry:
                return roster

        # Fetch fresh
        roster = await self.api_client.get_roster(team_id)
        expiry = datetime.now() + timedelta(hours=24)
        self.roster_cache[team_id] = (roster, expiry)
        return roster
```

---

## 7. Schedule Ingestion Workflow

### 7.1 Manual Workflow (User-Facing)

**Step 1: Obtain Schedule**
User obtains schedule from:
- League website (e.g., NBA.com, NFL.com)
- Third-party aggregator (ESPN, TheScore)
- Manual CSV creation

**Step 2: Convert to Standard Format**
User converts schedule to one of supported formats:

**Template CSV:**
```csv
date,time,home_team,away_team,venue,game_type
2026-01-15,19:30,Lakers,Warriors,Crypto.com Arena,regular
2026-01-15,20:00,Celtics,Heat,TD Garden,regular
```

**Template JSON:**
```json
{
  "sport": "NBA",
  "season": "2025",
  "games": [
    {
      "date": "2026-01-15",
      "time": "19:30",
      "timezone": "America/Los_Angeles",
      "home_team": "Lakers",
      "away_team": "Warriors"
    }
  ]
}
```

**Step 3: Run Ingestion**
```bash
# Ingest schedule
python -m scoracle_data.cli ingest-schedule \
  --file nba_schedule_2025.csv \
  --sport NBA \
  --season 2025 \
  --timezone "America/New_York"

# Verify ingestion
python -m scoracle_data.cli query games --sport NBA --season 2025 | head -20

# Generate seeding tasks
python -m scoracle_data.cli generate-tasks --sport NBA --season 2025

# Check upcoming seeds
python -m scoracle_data.cli monitor upcoming --days 7
```

### 7.2 Automated Workflow (Backend)

**Ingestion Pipeline:**
```
1. Parse File → 2. Validate Data → 3. Map Teams → 4. Insert Games → 5. Resolve Rosters → 6. Generate Tasks
```

**Pseudocode:**
```python
async def ingest_schedule(file_path: str, sport: str, season: str):
    # 1. Parse
    games = await parse_schedule_file(file_path)

    # 2. Validate
    for game in games:
        validate_game_data(game)

    # 3. Map teams
    for game in games:
        game['home_team_id'] = await map_team_name_to_id(game['home_team'], sport)
        game['away_team_id'] = await map_team_name_to_id(game['away_team'], sport)

    # 4. Insert games (batch)
    game_ids = await batch_insert_games(games)

    # 5. Resolve rosters
    for game_id, game in zip(game_ids, games):
        await resolve_and_store_roster(
            game_id,
            game['home_team_id'],
            game['away_team_id'],
            season
        )

    # 6. Generate seeding tasks
    for game_id, game in zip(game_ids, games):
        seed_time = game['game_datetime'] + timedelta(hours=4)
        await create_seeding_task(game_id, seed_time)

    return len(game_ids)
```

### 7.3 Team Name Mapping

**Challenge:** User-provided names may not match database names.

**Solution:** Fuzzy matching with manual override table.

```python
# Mapping table for ambiguous cases
TEAM_NAME_OVERRIDES = {
    'Lakers': 'Los Angeles Lakers',
    'Clippers': 'Los Angeles Clippers',
    'Warriors': 'Golden State Warriors',
    # ... etc
}

async def map_team_name_to_id(name: str, sport: str) -> int:
    # 1. Exact match
    team = await db.query(
        "SELECT id FROM teams WHERE name = %s AND sport_id = (SELECT id FROM sports WHERE display_name = %s)",
        (name, sport)
    )
    if team:
        return team['id']

    # 2. Check overrides
    canonical_name = TEAM_NAME_OVERRIDES.get(name, name)
    team = await db.query(
        "SELECT id FROM teams WHERE name = %s AND sport_id = (SELECT id FROM sports WHERE display_name = %s)",
        (canonical_name, sport)
    )
    if team:
        return team['id']

    # 3. Fuzzy match (using abbreviation or partial name)
    team = await db.query(
        "SELECT id FROM teams WHERE (abbreviation = %s OR name ILIKE %s) AND sport_id = (SELECT id FROM sports WHERE display_name = %s)",
        (name, f'%{name}%', sport)
    )
    if team:
        return team['id']

    # 4. Fail with helpful error
    raise ValueError(f"Could not map team name '{name}' to database. Please add to TEAM_NAME_OVERRIDES.")
```

---

## 8. Seeding Execution Logic

### 8.1 Execution Trigger

**Scheduler Job:**
```python
# APScheduler job registered during task generation
scheduler.add_job(
    func=execute_targeted_seed,
    trigger='date',
    run_date=game_start_time + timedelta(hours=4),
    args=[game_id],
    id=f'seed_game_{game_id}',
    replace_existing=True
)
```

**Execution Entry Point:**
```python
async def execute_targeted_seed(game_id: int):
    """Main entry point called by scheduler"""

    # 1. Fetch game metadata
    game = await db.get_game(game_id)

    # 2. Get participants (teams and players)
    team_ids, player_ids = await get_game_participants(game_id)

    # 3. Initialize sport-specific seeder
    seeder = get_seeder_for_sport(game.sport)

    # 4. Execute targeted seed
    try:
        await seeder.seed_game_participants(
            game_id=game_id,
            team_ids=team_ids,
            player_ids=player_ids
        )
        await mark_game_seeded(game_id, success=True)
    except Exception as e:
        await mark_game_seeded(game_id, success=False, error=str(e))
        await schedule_retry(game_id)
```

### 8.2 Participant Resolution

**Query to Get Participants:**
```python
async def get_game_participants(game_id: int) -> tuple[list[int], list[int]]:
    """Returns (team_ids, player_ids) for a game"""

    # Get teams (always 2)
    teams = await db.query("""
        SELECT home_team_id, away_team_id
        FROM games
        WHERE id = %s
    """, (game_id,))
    team_ids = [teams['home_team_id'], teams['away_team_id']]

    # Get players from pre-resolved rosters
    players = await db.query("""
        SELECT player_id
        FROM game_rosters
        WHERE game_id = %s
        AND status = 'active'
    """, (game_id,))
    player_ids = [p['player_id'] for p in players]

    return team_ids, player_ids
```

### 8.3 Targeted Stats Fetching

**Modified Seeder Methods:**

**Before (Blanket Seeding):**
```python
async def fetch_player_stats(self):
    """Fetch ALL players"""
    all_players = await self.db.get_all_players(self.sport, self.season)
    for player in all_players:
        stats = await self.api_client.get_player_stats(player.id)
        await self.upsert_player_stats(stats)
```

**After (Targeted Seeding):**
```python
async def fetch_player_stats(self, player_ids: list[int] | None = None):
    """Fetch specific players or all if None"""
    if player_ids is None:
        # Blanket mode (backward compatible)
        player_ids = await self.db.get_all_player_ids(self.sport, self.season)

    # Targeted mode
    for player_id in player_ids:
        stats = await self.api_client.get_player_stats(player_id)
        await self.upsert_player_stats(stats)
        await self.db.update_last_seeded(player_id, datetime.now())
```

### 8.4 Error Handling & Retry Logic

**Retry Strategy:**
- Max retries: 3
- Backoff: Exponential (1 min, 5 min, 15 min)
- Failure scenarios: API errors, network timeouts, invalid data

**Implementation:**
```python
async def execute_with_retry(game_id: int, max_retries: int = 3):
    """Execute targeted seed with exponential backoff retry"""

    for attempt in range(max_retries + 1):
        try:
            await execute_targeted_seed(game_id)
            return  # Success
        except Exception as e:
            if attempt == max_retries:
                # Final failure
                await log_final_failure(game_id, str(e))
                await send_alert(f"Game {game_id} seeding failed after {max_retries} retries")
                return

            # Calculate backoff
            backoff_minutes = 2 ** attempt  # 1, 2, 4, 8...
            next_retry = datetime.now() + timedelta(minutes=backoff_minutes)

            # Schedule retry
            await update_task_retry(game_id, attempt + 1, next_retry)
            scheduler.add_job(
                execute_with_retry,
                'date',
                run_date=next_retry,
                args=[game_id, max_retries],
                id=f'retry_seed_game_{game_id}_{attempt}',
                replace_existing=True
            )
            return
```

---

## 9. Error Handling & Monitoring

### 9.1 Failure Scenarios

| Scenario | Handling | Recovery |
|----------|----------|----------|
| **API Rate Limit Exceeded** | Sleep with exponential backoff | Retry after backoff period |
| **Invalid Team Mapping** | Log error, skip game | Manual mapping update required |
| **Network Timeout** | Retry with timeout increase | Max 3 retries, then alert |
| **Empty Roster** | Fetch team roster on-demand | Populate game_rosters, retry seed |
| **Scheduler Crash** | Jobs persist in PostgreSQL | Restart scheduler, resume jobs |
| **Database Connection Lost** | Connection pool retry logic | Automatic reconnection |
| **Partial Success** | Mark completed players, track failures | Retry only failed entities |
| **API Returns Invalid Data** | Skip entity, log warning | Manual data fix, reseed |

### 9.2 Monitoring Queries

**Upcoming Seeds (Next 24 Hours):**
```sql
SELECT
    g.id,
    g.game_datetime,
    st.scheduled_time,
    g.home_team_id,
    g.away_team_id,
    st.status,
    EXTRACT(EPOCH FROM (st.scheduled_time - NOW())) / 3600 AS hours_until_seed
FROM games g
JOIN seeding_tasks st ON st.game_id = g.id
WHERE st.scheduled_time BETWEEN NOW() AND NOW() + INTERVAL '24 hours'
AND st.status = 'pending'
ORDER BY st.scheduled_time;
```

**Failed Seeds (Last 7 Days):**
```sql
SELECT
    g.id,
    g.game_datetime,
    st.status,
    st.error_message,
    st.retry_count,
    st.updated_at
FROM games g
JOIN seeding_tasks st ON st.game_id = g.id
WHERE st.status = 'failed'
AND st.updated_at > NOW() - INTERVAL '7 days'
ORDER BY st.updated_at DESC;
```

**API Usage Stats:**
```sql
SELECT
    DATE(created_at) AS date,
    seeding_type,
    SUM(api_calls_made) AS total_calls,
    COUNT(*) AS total_operations,
    AVG(duration_seconds) AS avg_duration_sec
FROM sync_log
WHERE created_at > NOW() - INTERVAL '30 days'
GROUP BY DATE(created_at), seeding_type
ORDER BY date DESC;
```

**Seeding Latency (Scheduled vs Executed):**
```sql
SELECT
    g.id,
    g.game_datetime,
    st.scheduled_time,
    st.execution_time,
    EXTRACT(EPOCH FROM (st.execution_time - st.scheduled_time)) / 60 AS delay_minutes
FROM games g
JOIN seeding_tasks st ON st.game_id = g.id
WHERE st.status = 'completed'
AND st.execution_time IS NOT NULL
ORDER BY delay_minutes DESC
LIMIT 50;
```

### 9.3 Alerting Thresholds

**Critical Alerts (Immediate Notification):**
- Scheduler process crash (no heartbeat for 5 minutes)
- Database connection failure
- API key invalid/expired
- Failed seed retry limit exceeded (3+ failures)

**Warning Alerts (Daily Digest):**
- API usage >80% of daily quota
- Seeding latency >30 minutes
- Failed seeds >5% of total
- Unmapped team names in schedule

**Implementation:**
```python
class AlertManager:
    async def check_and_alert(self):
        """Run periodic health checks"""

        # Check scheduler health
        if not await self.is_scheduler_alive():
            await self.send_critical_alert("Scheduler process not responding")

        # Check API quota
        usage_pct = await self.get_api_usage_percentage()
        if usage_pct > 80:
            await self.send_warning_alert(f"API usage at {usage_pct}%")

        # Check failure rate
        failure_rate = await self.get_failure_rate(hours=24)
        if failure_rate > 0.05:
            await self.send_warning_alert(f"Failure rate: {failure_rate*100:.1f}%")

    async def send_critical_alert(self, message: str):
        """Send email/Slack for critical issues"""
        # Implementation: email, Slack webhook, PagerDuty, etc.
        pass
```

---

## 10. Testing Strategy

### 10.1 Unit Tests

**Coverage Areas:**
- Schedule parsing (CSV, JSON)
- Team name mapping (exact, fuzzy, override)
- Roster resolution
- Task scheduling (time calculations, timezone handling)
- Targeted seeding (entity filtering)
- Retry logic (backoff calculations)

**Example Tests:**
```python
# tests/test_schedule_ingestion.py
async def test_csv_parsing():
    """Test CSV schedule parsing"""
    ingester = ScheduleIngester()
    games = await ingester.parse_csv('fixtures/nba_schedule.csv')
    assert len(games) == 10
    assert games[0]['home_team'] == 'Lakers'

async def test_team_mapping():
    """Test team name to ID mapping"""
    ingester = ScheduleIngester()
    team_id = await ingester.map_team_name_to_id('Lakers', 'NBA')
    assert team_id == 1  # Assuming Lakers is ID 1

async def test_fuzzy_team_matching():
    """Test fuzzy matching for team names"""
    ingester = ScheduleIngester()
    team_id = await ingester.map_team_name_to_id('LA Lakers', 'NBA')
    assert team_id == 1  # Should match "Los Angeles Lakers"
```

### 10.2 Integration Tests

**Test Scenarios:**
1. **End-to-End Schedule Ingestion:**
   - Ingest sample schedule
   - Verify games table populated
   - Verify seeding_tasks created
   - Verify scheduled times correct (game_time + 4 hours)

2. **Targeted Seeding Execution:**
   - Mock API responses for specific players/teams
   - Trigger targeted seed
   - Verify only specified entities updated
   - Verify last_seeded_at timestamps updated

3. **Retry Logic:**
   - Simulate API failure
   - Verify retry scheduled with correct backoff
   - Verify max retries respected

**Example:**
```python
# tests/test_targeted_seeding_integration.py
async def test_game_seeding_end_to_end():
    """Test full workflow from schedule to seeding"""

    # 1. Ingest schedule
    await ingest_schedule('fixtures/single_game.csv', 'NBA', '2025')

    # 2. Verify game created
    game = await db.get_game_by_teams('Lakers', 'Warriors', '2026-01-15')
    assert game is not None

    # 3. Verify task scheduled
    task = await db.get_seeding_task_by_game(game.id)
    assert task.scheduled_time == game.game_datetime + timedelta(hours=4)

    # 4. Execute seed (with mocked API)
    with mock_api_client():
        await execute_targeted_seed(game.id)

    # 5. Verify stats updated for only participants
    updated_players = await db.get_recently_updated_players(since=datetime.now() - timedelta(minutes=1))
    assert len(updated_players) == 30  # ~15 per team
    assert all(p.team_id in [game.home_team_id, game.away_team_id] for p in updated_players)
```

### 10.3 Performance Tests

**Load Testing:**
- Simulate 50+ concurrent game seeds (typical Saturday in Football)
- Measure API throughput (calls per second)
- Measure database write throughput (inserts per second)
- Verify no rate limit violations

**Example:**
```python
async def test_concurrent_game_seeding():
    """Test system under high load (50 concurrent games)"""

    # Create 50 fake games
    game_ids = await create_test_games(count=50)

    # Trigger all seeds concurrently
    start_time = time.time()
    await asyncio.gather(*[
        execute_targeted_seed(game_id) for game_id in game_ids
    ])
    duration = time.time() - start_time

    # Verify performance
    assert duration < 300  # Should complete within 5 minutes

    # Verify all completed successfully
    tasks = await db.get_seeding_tasks(game_ids)
    assert all(t.status == 'completed' for t in tasks)
```

---

## 11. Deployment & Operations

### 11.1 Deployment Options

**Option A: Systemd Service (Recommended for VM/Bare Metal)**
```ini
# /etc/systemd/system/scoracle-scheduler.service
[Unit]
Description=Scoracle Seeding Scheduler
After=network.target postgresql.service

[Service]
Type=simple
User=scoracle
Group=scoracle
WorkingDirectory=/opt/scoracle-data
Environment="DATABASE_URL=postgresql://..."
Environment="API_SPORTS_KEY=..."
ExecStart=/opt/scoracle-data/venv/bin/python -m scoracle_data.schedulers.daemon
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

**Commands:**
```bash
sudo systemctl enable scoracle-scheduler
sudo systemctl start scoracle-scheduler
sudo systemctl status scoracle-scheduler
journalctl -u scoracle-scheduler -f  # View logs
```

**Option B: Docker Container**
```dockerfile
# Dockerfile
FROM python:3.11-slim

WORKDIR /app
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY src/ /app/src/
ENV PYTHONPATH=/app/src

CMD ["python", "-m", "scoracle_data.schedulers.daemon"]
```

**Docker Compose:**
```yaml
version: '3.8'

services:
  scheduler:
    build: .
    environment:
      - DATABASE_URL=${DATABASE_URL}
      - API_SPORTS_KEY=${API_SPORTS_KEY}
    restart: unless-stopped
    depends_on:
      - postgres
    volumes:
      - ./logs:/app/logs

  postgres:
    image: postgres:15
    environment:
      - POSTGRES_DB=scoracle
      - POSTGRES_PASSWORD=${DB_PASSWORD}
    volumes:
      - postgres_data:/var/lib/postgresql/data

volumes:
  postgres_data:
```

**Commands:**
```bash
docker-compose up -d
docker-compose logs -f scheduler
docker-compose restart scheduler
```

**Option C: Kubernetes CronJob (For Cloud-Native)**
```yaml
apiVersion: batch/v1
kind: CronJob
metadata:
  name: scoracle-scheduler
spec:
  schedule: "*/5 * * * *"  # Run every 5 minutes to check for due tasks
  jobTemplate:
    spec:
      template:
        spec:
          containers:
          - name: scheduler
            image: scoracle/scheduler:latest
            env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: scoracle-secrets
                  key: database-url
            - name: API_SPORTS_KEY
              valueFrom:
                secretKeyRef:
                  name: scoracle-secrets
                  key: api-key
          restartPolicy: OnFailure
```

### 11.2 Configuration Management

**Environment Variables:**
```bash
# .env.production
DATABASE_URL=postgresql://user:pass@host:5432/scoracle
API_SPORTS_KEY=your_api_key_here
LOG_LEVEL=INFO
MAX_CONCURRENT_SEEDS=10
API_RATE_LIMIT_PER_SECOND=10
RETRY_MAX_ATTEMPTS=3
RETRY_BACKOFF_BASE=2
ALERT_EMAIL=admin@example.com
SLACK_WEBHOOK_URL=https://hooks.slack.com/...
```

**Config File (`config/production.yaml`):**
```yaml
scheduler:
  timezone: UTC
  job_store: postgresql
  max_workers: 20

seeding:
  default_delay_hours: 4
  batch_size: 50
  timeout_seconds: 300

api:
  timeout_seconds: 30
  max_retries: 3
  rate_limit_calls_per_second: 10

monitoring:
  health_check_interval_seconds: 60
  alert_on_failure_threshold: 3
  metrics_retention_days: 90
```

### 11.3 Operational Runbook

**Daily Operations:**

1. **Morning Check (9 AM):**
   ```bash
   # Check scheduler status
   python -m scoracle_data.cli scheduler status

   # View upcoming seeds for today
   python -m scoracle_data.cli monitor upcoming --days 1

   # Check for failures overnight
   python -m scoracle_data.cli monitor failures --since "24 hours ago"
   ```

2. **Schedule Ingestion (Weekly or as needed):**
   ```bash
   # Download latest schedule from league website
   # Convert to CSV/JSON format
   # Ingest schedule
   python -m scoracle_data.cli ingest-schedule \
     --file nba_week_15.csv \
     --sport NBA \
     --season 2025

   # Verify ingestion
   python -m scoracle_data.cli query games --sport NBA --week 15
   ```

3. **Manual Seed (if needed):**
   ```bash
   # Trigger seed for specific game
   python -m scoracle_data.cli seed-game --game-id 12345

   # Trigger blanket seed (fallback)
   python -m scoracle_data.cli seed-2phase --sport NBA --season 2025
   ```

**Weekly Operations:**

1. **Review API Usage:**
   ```bash
   python -m scoracle_data.cli monitor stats --days 7
   ```

2. **Check Failure Trends:**
   ```bash
   python -m scoracle_data.cli monitor failures --days 7 --group-by error_type
   ```

3. **Database Maintenance:**
   ```sql
   -- Archive old sync logs
   DELETE FROM sync_log WHERE created_at < NOW() - INTERVAL '90 days';

   -- Vacuum and analyze
   VACUUM ANALYZE games;
   VACUUM ANALYZE seeding_tasks;
   ```

**Monthly Operations:**

1. **Performance Review:**
   - Analyze seeding latency trends
   - Identify slow API endpoints
   - Review retry patterns

2. **Cost Analysis:**
   - API usage vs quota
   - Database storage growth
   - Compute resource utilization

**Incident Response:**

**Scenario: Scheduler Crash**
```bash
# 1. Check process status
sudo systemctl status scoracle-scheduler

# 2. View recent logs
journalctl -u scoracle-scheduler -n 100

# 3. Restart service
sudo systemctl restart scoracle-scheduler

# 4. Verify recovery
python -m scoracle_data.cli scheduler status

# 5. Check for missed seeds
python -m scoracle_data.cli monitor pending-seeds
```

**Scenario: API Rate Limit Exceeded**
```bash
# 1. Check current usage
python -m scoracle_data.cli monitor api-usage --today

# 2. Pause scheduler temporarily
python -m scoracle_data.cli scheduler pause

# 3. Wait for quota reset (usually midnight UTC)

# 4. Resume scheduler
python -m scoracle_data.cli scheduler resume
```

### 11.4 Scaling Considerations

**Horizontal Scaling:**
- APScheduler supports distributed mode with PostgreSQL job store
- Multiple scheduler instances can share job queue
- Use database locks to prevent duplicate execution

**Vertical Scaling:**
- Increase concurrent workers in APScheduler config
- Increase database connection pool size
- Increase API rate limit semaphore

**Database Optimization:**
- Partition `sync_log` table by date
- Archive old `seeding_tasks` and `game_rosters`
- Use read replicas for monitoring queries

---

## Success Metrics

### Phase 1-2 (Foundation + Scheduler)
- ✅ Successfully ingest 1,230-game NBA schedule
- ✅ Scheduler runs for 7 days without crashes
- ✅ 100% of jobs execute within 10 minutes of scheduled time

### Phase 3 (Targeted Seeding)
- ✅ API call reduction of 60%+ compared to blanket seeding
- ✅ Average game seeding completes in <30 seconds
- ✅ Failed seed retry rate <5%

### Phase 4-5 (Monitoring + Multi-Sport)
- ✅ All 3 sports (NBA, NFL, Football) running in production
- ✅ Same-day stats available for 95%+ of games
- ✅ Zero manual intervention required for 30 days

### Phase 6 (Production)
- ✅ 99% uptime over 90-day period
- ✅ API quota utilization <80% of limit
- ✅ Mean time to detection (MTTD) for failures <5 minutes
- ✅ Mean time to recovery (MTTR) for failures <15 minutes

---

## Appendix

### A. CLI Command Reference

```bash
# Schedule ingestion
ingest-schedule --file <path> --sport <sport> --season <year> [--timezone <tz>]

# Task management
generate-tasks --sport <sport> --season <year>
list-tasks --status [pending|running|completed|failed] [--days <n>]
retry-task --task-id <id>
cancel-task --task-id <id>

# Scheduler control
scheduler start
scheduler stop
scheduler restart
scheduler status
scheduler pause
scheduler resume

# Monitoring
monitor upcoming [--days <n>] [--sport <sport>]
monitor failures [--days <n>] [--sport <sport>]
monitor stats [--days <n>]
monitor api-usage [--today|--week|--month]

# Manual seeding
seed-game --game-id <id> [--force]
seed-games --date <date> [--sport <sport>]

# Queries
query games --sport <sport> [--season <year>] [--date <date>] [--week <n>]
query tasks --game-id <id>
```

### B. Database Indexes

```sql
-- Performance-critical indexes
CREATE INDEX idx_games_sport_season_date ON games(sport_id, season_id, game_date);
CREATE INDEX idx_games_seeding_pending ON games(seeding_status) WHERE seeding_status = 'pending';
CREATE INDEX idx_seeding_tasks_due ON seeding_tasks(scheduled_time) WHERE status = 'pending';
CREATE INDEX idx_game_rosters_lookup ON game_rosters(game_id, team_id);
CREATE INDEX idx_players_team_season ON players(current_team_id, season);
CREATE INDEX idx_sync_log_date_type ON sync_log(created_at, seeding_type);
```

### C. API Endpoint Mapping

| Sport | Team Stats Endpoint | Player Stats Endpoint | Roster Endpoint |
|-------|---------------------|----------------------|-----------------|
| NBA | `/teams/statistics` | `/players/statistics` | `/players` (filter by team) |
| NFL | `/teams/statistics` | `/players/statistics` | `/players` (filter by team) |
| Football | `/teams/statistics` | `/players` (includes stats) | `/players/squads` |

### D. Timezone Handling

All times stored in database as `TIMESTAMP WITH TIME ZONE` (UTC).

**Conversion Strategy:**
1. User provides local time + timezone in schedule file
2. Convert to UTC during ingestion: `datetime.astimezone(timezone.utc)`
3. Store UTC in database
4. Scheduler operates in UTC
5. Display in user's local timezone for CLI output

**Example:**
```python
from datetime import datetime
from zoneinfo import ZoneInfo

# User input: "2026-01-15 19:30 America/Los_Angeles"
local_time = datetime(2026, 1, 15, 19, 30, tzinfo=ZoneInfo("America/Los_Angeles"))

# Convert to UTC
utc_time = local_time.astimezone(ZoneInfo("UTC"))

# Store in database
await db.execute("INSERT INTO games (game_datetime) VALUES (%s)", (utc_time,))
```

---

## Next Steps

1. **Review & Approval:** Review this plan with stakeholders
2. **Environment Setup:** Provision PostgreSQL database, API keys
3. **Phase 1 Kickoff:** Begin migration creation and schedule ingestion
4. **Iterative Development:** Build and test each phase incrementally
5. **Production Rollout:** Deploy to production after Phase 6 completion

---

**Document Owner:** Technical Lead
**Last Reviewed:** 2026-01-07
**Next Review:** After Phase 1 completion
