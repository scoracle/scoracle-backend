# SQLite to Neon (PostgreSQL) Migration Plan

## Executive Summary

This document outlines the migration strategy for transitioning the scoracle-data service from SQLite to Neon (PostgreSQL). The migration will modernize the database layer and leverage PostgreSQL's native percentile functions for significant performance improvements.

**Current State:** SQLite with Python-based percentile calculations
**Target State:** Neon PostgreSQL with native `PERCENT_RANK()` window functions
**Estimated Complexity:** Medium - Well-structured codebase with clean query patterns

---

## Table of Contents

1. [Prerequisites](#1-prerequisites)
2. [Schema Conversion](#2-schema-conversion)
3. [Connection Layer Updates](#3-connection-layer-updates)
4. [Query Pattern Migration](#4-query-pattern-migration)
5. [Percentile Optimization](#5-percentile-optimization)
6. [Data Migration](#6-data-migration)
7. [Testing Strategy](#7-testing-strategy)
8. [Deployment Plan](#8-deployment-plan)
9. [Rollback Strategy](#9-rollback-strategy)
10. [Post-Migration Optimization](#10-post-migration-optimization)

---

## 1. Prerequisites

### 1.1 Neon Setup
- [ ] Create Neon project at console.neon.tech
- [ ] Create database named `scoracle_data`
- [ ] Note connection string: `postgresql://user:password@ep-xxx.region.aws.neon.tech/scoracle_data`
- [ ] Enable connection pooling (recommended for serverless)

### 1.2 Dependencies to Add

Update `pyproject.toml`:

```toml
[project.dependencies]
# Add these new dependencies
psycopg = "^3.1"           # Modern PostgreSQL adapter (psycopg3)
psycopg-pool = "^3.1"      # Connection pooling
# OR for async support:
asyncpg = "^0.29"          # Async PostgreSQL adapter
```

### 1.3 Environment Variables

```bash
# New environment variables needed
DATABASE_URL=postgresql://user:password@ep-xxx.region.aws.neon.tech/scoracle_data
DATABASE_POOL_SIZE=10
DATABASE_POOL_MAX_OVERFLOW=20
```

---

## 2. Schema Conversion

### 2.1 Data Type Mappings

| SQLite Type | PostgreSQL Type | Notes |
|-------------|-----------------|-------|
| `INTEGER PRIMARY KEY` | `SERIAL` or `BIGSERIAL` | Auto-increment |
| `TEXT` | `TEXT` or `VARCHAR(n)` | Direct mapping |
| `REAL` | `REAL` or `DOUBLE PRECISION` | Use `NUMERIC` for financial precision |
| `INTEGER` (timestamps) | `TIMESTAMPTZ` | Convert Unix timestamps |
| `INTEGER` (boolean) | `BOOLEAN` | Convert 0/1 to false/true |

### 2.2 Schema Changes Required

#### 2.2.1 Remove SQLite-Specific Syntax

```sql
-- SQLite (current)
CREATE TABLE IF NOT EXISTS players (
    id INTEGER PRIMARY KEY,
    ...
);

-- PostgreSQL (target)
CREATE TABLE IF NOT EXISTS players (
    id SERIAL PRIMARY KEY,
    ...
);
```

#### 2.2.2 Timestamp Conversion

**Option A: Keep as integers (minimal change)**
```sql
-- No change, store Unix timestamps as BIGINT
updated_at BIGINT
```

**Option B: Convert to native timestamps (recommended)**
```sql
-- Use PostgreSQL native timestamps
updated_at TIMESTAMPTZ DEFAULT NOW()
```

If using Option B, update all timestamp comparisons:
```sql
-- SQLite: strftime('%s', 'now')
-- PostgreSQL: EXTRACT(EPOCH FROM NOW())::BIGINT
-- OR just use: NOW() with TIMESTAMPTZ columns
```

### 2.3 Consolidated Migration Script

Create `migrations/007_postgresql_conversion.sql`:

```sql
-- PostgreSQL Schema for scoracle-data
-- Converted from SQLite migrations 001-006

-- ============================================
-- CORE TABLES
-- ============================================

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT,
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS sports (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    api_base_url TEXT,
    current_season INTEGER,
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS seasons (
    id SERIAL PRIMARY KEY,
    sport_id TEXT NOT NULL REFERENCES sports(id),
    season_year INTEGER NOT NULL,
    is_current BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE (sport_id, season_year)
);
CREATE INDEX idx_seasons_sport ON seasons(sport_id);
CREATE INDEX idx_seasons_current ON seasons(sport_id, is_current) WHERE is_current = true;

CREATE TABLE IF NOT EXISTS leagues (
    id SERIAL PRIMARY KEY,
    sport_id TEXT NOT NULL REFERENCES sports(id),
    name TEXT NOT NULL,
    country TEXT,
    country_code TEXT,
    logo_url TEXT,
    priority_tier INTEGER DEFAULT 3,
    include_in_percentiles BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX idx_leagues_sport ON leagues(sport_id);
CREATE INDEX idx_leagues_country ON leagues(country);
CREATE INDEX idx_leagues_priority ON leagues(priority_tier);

CREATE TABLE IF NOT EXISTS teams (
    id INTEGER PRIMARY KEY,  -- Keep as INTEGER (API-sourced IDs)
    sport_id TEXT NOT NULL REFERENCES sports(id),
    league_id INTEGER REFERENCES leagues(id),
    name TEXT NOT NULL,
    abbreviation TEXT,
    logo_url TEXT,
    conference TEXT,
    division TEXT,
    venue_name TEXT,
    venue_capacity INTEGER,
    venue_city TEXT,
    venue_surface TEXT,
    profile_fetched_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX idx_teams_sport ON teams(sport_id);
CREATE INDEX idx_teams_league ON teams(league_id);
CREATE INDEX idx_teams_name ON teams(name);
CREATE INDEX idx_teams_conference ON teams(sport_id, conference);
CREATE INDEX idx_teams_needs_profile ON teams(sport_id) WHERE profile_fetched_at IS NULL;

CREATE TABLE IF NOT EXISTS players (
    id INTEGER PRIMARY KEY,  -- Keep as INTEGER (API-sourced IDs)
    sport_id TEXT NOT NULL REFERENCES sports(id),
    current_team_id INTEGER REFERENCES teams(id),
    current_league_id INTEGER REFERENCES leagues(id),
    first_name TEXT,
    last_name TEXT,
    full_name TEXT NOT NULL,
    position TEXT,
    position_group TEXT,
    nationality TEXT,
    height_inches REAL,
    weight_lbs REAL,
    college TEXT,
    experience_years INTEGER,
    profile_fetched_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX idx_players_sport ON players(sport_id);
CREATE INDEX idx_players_team ON players(current_team_id);
CREATE INDEX idx_players_name ON players(full_name);
CREATE INDEX idx_players_position ON players(sport_id, position);
CREATE INDEX idx_players_position_group ON players(sport_id, position_group);
CREATE INDEX idx_players_league ON players(current_league_id);
CREATE INDEX idx_players_needs_profile ON players(sport_id) WHERE profile_fetched_at IS NULL;

CREATE TABLE IF NOT EXISTS player_teams (
    id SERIAL PRIMARY KEY,
    player_id INTEGER NOT NULL REFERENCES players(id),
    team_id INTEGER NOT NULL REFERENCES teams(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    start_date DATE,
    end_date DATE,
    is_current BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE (player_id, team_id, season_id)
);

-- ============================================
-- NBA STATS TABLES
-- ============================================

CREATE TABLE IF NOT EXISTS nba_player_stats (
    id SERIAL PRIMARY KEY,
    player_id INTEGER NOT NULL REFERENCES players(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    team_id INTEGER REFERENCES teams(id),
    games_played INTEGER DEFAULT 0,
    games_started INTEGER DEFAULT 0,
    minutes_per_game REAL DEFAULT 0,
    minutes_total REAL DEFAULT 0,
    points_per_game REAL DEFAULT 0,
    points_total INTEGER DEFAULT 0,
    fg_made REAL DEFAULT 0,
    fg_attempted REAL DEFAULT 0,
    fg_pct REAL DEFAULT 0,
    tp_made REAL DEFAULT 0,
    tp_attempted REAL DEFAULT 0,
    tp_pct REAL DEFAULT 0,
    ft_made REAL DEFAULT 0,
    ft_attempted REAL DEFAULT 0,
    ft_pct REAL DEFAULT 0,
    offensive_rebounds REAL DEFAULT 0,
    defensive_rebounds REAL DEFAULT 0,
    rebounds_per_game REAL DEFAULT 0,
    rebounds_total INTEGER DEFAULT 0,
    assists_per_game REAL DEFAULT 0,
    assists_total INTEGER DEFAULT 0,
    turnovers_per_game REAL DEFAULT 0,
    turnovers_total INTEGER DEFAULT 0,
    steals_per_game REAL DEFAULT 0,
    steals_total INTEGER DEFAULT 0,
    blocks_per_game REAL DEFAULT 0,
    blocks_total INTEGER DEFAULT 0,
    personal_fouls REAL DEFAULT 0,
    fouls_per_game REAL DEFAULT 0,
    plus_minus REAL DEFAULT 0,
    plus_minus_per_game REAL DEFAULT 0,
    efficiency REAL DEFAULT 0,
    true_shooting_pct REAL DEFAULT 0,
    effective_fg_pct REAL DEFAULT 0,
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE (player_id, season_id, team_id)
);
CREATE INDEX idx_nba_player_stats_player ON nba_player_stats(player_id);
CREATE INDEX idx_nba_player_stats_season ON nba_player_stats(season_id);
CREATE INDEX idx_nba_player_stats_team ON nba_player_stats(team_id);
CREATE INDEX idx_nba_player_stats_ppg ON nba_player_stats(season_id, points_per_game DESC);

CREATE TABLE IF NOT EXISTS nba_team_stats (
    id SERIAL PRIMARY KEY,
    team_id INTEGER NOT NULL REFERENCES teams(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    wins INTEGER DEFAULT 0,
    losses INTEGER DEFAULT 0,
    win_pct REAL DEFAULT 0,
    conference_rank INTEGER,
    division_rank INTEGER,
    home_wins INTEGER DEFAULT 0,
    home_losses INTEGER DEFAULT 0,
    away_wins INTEGER DEFAULT 0,
    away_losses INTEGER DEFAULT 0,
    points_per_game REAL DEFAULT 0,
    opponent_ppg REAL DEFAULT 0,
    point_differential REAL DEFAULT 0,
    fg_pct REAL DEFAULT 0,
    tp_pct REAL DEFAULT 0,
    ft_pct REAL DEFAULT 0,
    rebounds_per_game REAL DEFAULT 0,
    assists_per_game REAL DEFAULT 0,
    turnovers_per_game REAL DEFAULT 0,
    steals_per_game REAL DEFAULT 0,
    blocks_per_game REAL DEFAULT 0,
    offensive_rating REAL,
    defensive_rating REAL,
    net_rating REAL,
    pace REAL,
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE (team_id, season_id)
);
CREATE INDEX idx_nba_team_stats_team ON nba_team_stats(team_id);
CREATE INDEX idx_nba_team_stats_season ON nba_team_stats(season_id);
CREATE INDEX idx_nba_team_stats_winpct ON nba_team_stats(season_id, win_pct DESC);

-- ============================================
-- NFL STATS TABLES (Unified)
-- ============================================

CREATE TABLE IF NOT EXISTS nfl_player_stats (
    id SERIAL PRIMARY KEY,
    player_id INTEGER NOT NULL REFERENCES players(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    team_id INTEGER REFERENCES teams(id),
    games_played INTEGER DEFAULT 0,
    games_started INTEGER DEFAULT 0,
    -- Passing stats
    pass_attempts INTEGER DEFAULT 0,
    pass_completions INTEGER DEFAULT 0,
    pass_yards INTEGER DEFAULT 0,
    pass_yards_per_game REAL DEFAULT 0,
    pass_tds INTEGER DEFAULT 0,
    interceptions INTEGER DEFAULT 0,
    completion_pct REAL DEFAULT 0,
    yards_per_attempt REAL DEFAULT 0,
    passer_rating REAL DEFAULT 0,
    qbr REAL DEFAULT 0,
    sacks_taken INTEGER DEFAULT 0,
    sack_yards_lost INTEGER DEFAULT 0,
    times_pressured INTEGER DEFAULT 0,
    pressure_pct REAL DEFAULT 0,
    -- Rushing stats
    rush_attempts INTEGER DEFAULT 0,
    rush_yards INTEGER DEFAULT 0,
    rush_yards_per_game REAL DEFAULT 0,
    rush_tds INTEGER DEFAULT 0,
    yards_per_carry REAL DEFAULT 0,
    rush_first_downs INTEGER DEFAULT 0,
    fumbles INTEGER DEFAULT 0,
    fumbles_lost INTEGER DEFAULT 0,
    -- Receiving stats
    targets INTEGER DEFAULT 0,
    receptions INTEGER DEFAULT 0,
    receiving_yards INTEGER DEFAULT 0,
    receiving_yards_per_game REAL DEFAULT 0,
    receiving_tds INTEGER DEFAULT 0,
    yards_per_reception REAL DEFAULT 0,
    yards_per_target REAL DEFAULT 0,
    catch_pct REAL DEFAULT 0,
    drops INTEGER DEFAULT 0,
    drop_pct REAL DEFAULT 0,
    yards_after_catch REAL DEFAULT 0,
    contested_catches INTEGER DEFAULT 0,
    receiving_first_downs INTEGER DEFAULT 0,
    -- Defense stats
    tackles_solo INTEGER DEFAULT 0,
    tackles_assisted INTEGER DEFAULT 0,
    tackles_total INTEGER DEFAULT 0,
    tackles_for_loss INTEGER DEFAULT 0,
    sacks REAL DEFAULT 0,
    sack_yards REAL DEFAULT 0,
    qb_hits INTEGER DEFAULT 0,
    pressures INTEGER DEFAULT 0,
    hurries INTEGER DEFAULT 0,
    interceptions_def INTEGER DEFAULT 0,
    int_yards INTEGER DEFAULT 0,
    int_tds INTEGER DEFAULT 0,
    passes_defended INTEGER DEFAULT 0,
    forced_fumbles INTEGER DEFAULT 0,
    fumble_recoveries INTEGER DEFAULT 0,
    fumble_recovery_tds INTEGER DEFAULT 0,
    safeties INTEGER DEFAULT 0,
    -- Kicking stats
    fg_attempts INTEGER DEFAULT 0,
    fg_made INTEGER DEFAULT 0,
    fg_pct REAL DEFAULT 0,
    fg_long INTEGER DEFAULT 0,
    xp_attempts INTEGER DEFAULT 0,
    xp_made INTEGER DEFAULT 0,
    xp_pct REAL DEFAULT 0,
    punts INTEGER DEFAULT 0,
    punt_yards INTEGER DEFAULT 0,
    punt_avg REAL DEFAULT 0,
    punt_long INTEGER DEFAULT 0,
    -- Return stats
    kick_returns INTEGER DEFAULT 0,
    kick_return_yards INTEGER DEFAULT 0,
    kick_return_avg REAL DEFAULT 0,
    kick_return_tds INTEGER DEFAULT 0,
    punt_returns INTEGER DEFAULT 0,
    punt_return_yards INTEGER DEFAULT 0,
    punt_return_avg REAL DEFAULT 0,
    punt_return_tds INTEGER DEFAULT 0,
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE (player_id, season_id)
);
CREATE INDEX idx_nfl_player_stats_player ON nfl_player_stats(player_id);
CREATE INDEX idx_nfl_player_stats_season ON nfl_player_stats(season_id);
CREATE INDEX idx_nfl_player_stats_team ON nfl_player_stats(team_id);

CREATE TABLE IF NOT EXISTS nfl_team_stats (
    id SERIAL PRIMARY KEY,
    team_id INTEGER NOT NULL REFERENCES teams(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    wins INTEGER DEFAULT 0,
    losses INTEGER DEFAULT 0,
    ties INTEGER DEFAULT 0,
    win_pct REAL DEFAULT 0,
    conference_wins INTEGER DEFAULT 0,
    conference_losses INTEGER DEFAULT 0,
    division_wins INTEGER DEFAULT 0,
    division_losses INTEGER DEFAULT 0,
    home_wins INTEGER DEFAULT 0,
    home_losses INTEGER DEFAULT 0,
    away_wins INTEGER DEFAULT 0,
    away_losses INTEGER DEFAULT 0,
    points_for INTEGER DEFAULT 0,
    points_against INTEGER DEFAULT 0,
    point_differential INTEGER DEFAULT 0,
    points_per_game REAL DEFAULT 0,
    opponent_ppg REAL DEFAULT 0,
    total_yards INTEGER DEFAULT 0,
    yards_per_game REAL DEFAULT 0,
    pass_yards INTEGER DEFAULT 0,
    pass_yards_per_game REAL DEFAULT 0,
    rush_yards INTEGER DEFAULT 0,
    rush_yards_per_game REAL DEFAULT 0,
    completion_pct REAL DEFAULT 0,
    pass_tds INTEGER DEFAULT 0,
    rush_tds INTEGER DEFAULT 0,
    total_tds INTEGER DEFAULT 0,
    turnovers INTEGER DEFAULT 0,
    takeaways INTEGER DEFAULT 0,
    turnover_differential INTEGER DEFAULT 0,
    yards_allowed INTEGER DEFAULT 0,
    yards_allowed_per_game REAL DEFAULT 0,
    pass_yards_allowed INTEGER DEFAULT 0,
    rush_yards_allowed INTEGER DEFAULT 0,
    points_allowed_per_game REAL DEFAULT 0,
    sacks INTEGER DEFAULT 0,
    interceptions INTEGER DEFAULT 0,
    third_down_pct REAL DEFAULT 0,
    fourth_down_pct REAL DEFAULT 0,
    red_zone_pct REAL DEFAULT 0,
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE (team_id, season_id)
);
CREATE INDEX idx_nfl_team_stats_team ON nfl_team_stats(team_id);
CREATE INDEX idx_nfl_team_stats_season ON nfl_team_stats(season_id);
CREATE INDEX idx_nfl_team_stats_winpct ON nfl_team_stats(season_id, win_pct DESC);

-- ============================================
-- FOOTBALL (SOCCER) STATS TABLES
-- ============================================

CREATE TABLE IF NOT EXISTS football_player_stats (
    id SERIAL PRIMARY KEY,
    player_id INTEGER NOT NULL REFERENCES players(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    league_id INTEGER REFERENCES leagues(id),
    team_id INTEGER REFERENCES teams(id),
    matches_played INTEGER DEFAULT 0,
    matches_started INTEGER DEFAULT 0,
    minutes_played INTEGER DEFAULT 0,
    goals INTEGER DEFAULT 0,
    assists INTEGER DEFAULT 0,
    goals_assists INTEGER DEFAULT 0,
    goals_per_90 REAL DEFAULT 0,
    assists_per_90 REAL DEFAULT 0,
    shots_total INTEGER DEFAULT 0,
    shots_on_target INTEGER DEFAULT 0,
    shot_accuracy REAL DEFAULT 0,
    expected_goals REAL DEFAULT 0,
    expected_assists REAL DEFAULT 0,
    passes_total INTEGER DEFAULT 0,
    pass_accuracy REAL DEFAULT 0,
    key_passes INTEGER DEFAULT 0,
    crosses_total INTEGER DEFAULT 0,
    crosses_accuracy REAL DEFAULT 0,
    through_balls INTEGER DEFAULT 0,
    long_balls INTEGER DEFAULT 0,
    dribbles_attempted INTEGER DEFAULT 0,
    dribbles_succeeded INTEGER DEFAULT 0,
    dribble_success_rate REAL DEFAULT 0,
    dribbles_per_90 REAL DEFAULT 0,
    duels_total INTEGER DEFAULT 0,
    duels_won INTEGER DEFAULT 0,
    duels_win_rate REAL DEFAULT 0,
    aerial_duels_won INTEGER DEFAULT 0,
    aerial_duels_total INTEGER DEFAULT 0,
    aerial_win_rate REAL DEFAULT 0,
    ground_duels_won INTEGER DEFAULT 0,
    ground_duels_total INTEGER DEFAULT 0,
    tackles INTEGER DEFAULT 0,
    tackles_won INTEGER DEFAULT 0,
    tackle_success_rate REAL DEFAULT 0,
    interceptions INTEGER DEFAULT 0,
    blocks INTEGER DEFAULT 0,
    clearances INTEGER DEFAULT 0,
    tackles_per_90 REAL DEFAULT 0,
    interceptions_per_90 REAL DEFAULT 0,
    fouls_committed INTEGER DEFAULT 0,
    fouls_drawn INTEGER DEFAULT 0,
    yellow_cards INTEGER DEFAULT 0,
    red_cards INTEGER DEFAULT 0,
    penalties_scored INTEGER DEFAULT 0,
    penalties_missed INTEGER DEFAULT 0,
    -- Goalkeeper stats
    saves INTEGER DEFAULT 0,
    save_percentage REAL DEFAULT 0,
    goals_conceded INTEGER DEFAULT 0,
    goals_conceded_per_90 REAL DEFAULT 0,
    clean_sheets INTEGER DEFAULT 0,
    penalties_saved INTEGER DEFAULT 0,
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE (player_id, season_id, league_id)
);
CREATE INDEX idx_football_player_stats_player ON football_player_stats(player_id);
CREATE INDEX idx_football_player_stats_season ON football_player_stats(season_id);
CREATE INDEX idx_football_player_stats_league ON football_player_stats(league_id);
CREATE INDEX idx_football_player_stats_team ON football_player_stats(team_id);

CREATE TABLE IF NOT EXISTS football_team_stats (
    id SERIAL PRIMARY KEY,
    team_id INTEGER NOT NULL REFERENCES teams(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    league_id INTEGER REFERENCES leagues(id),
    matches_played INTEGER DEFAULT 0,
    wins INTEGER DEFAULT 0,
    draws INTEGER DEFAULT 0,
    losses INTEGER DEFAULT 0,
    points INTEGER DEFAULT 0,
    home_played INTEGER DEFAULT 0,
    home_wins INTEGER DEFAULT 0,
    home_draws INTEGER DEFAULT 0,
    home_losses INTEGER DEFAULT 0,
    home_goals_for INTEGER DEFAULT 0,
    home_goals_against INTEGER DEFAULT 0,
    away_played INTEGER DEFAULT 0,
    away_wins INTEGER DEFAULT 0,
    away_draws INTEGER DEFAULT 0,
    away_losses INTEGER DEFAULT 0,
    away_goals_for INTEGER DEFAULT 0,
    away_goals_against INTEGER DEFAULT 0,
    goals_for INTEGER DEFAULT 0,
    goals_against INTEGER DEFAULT 0,
    goal_difference INTEGER DEFAULT 0,
    clean_sheets INTEGER DEFAULT 0,
    failed_to_score INTEGER DEFAULT 0,
    form TEXT,
    avg_possession REAL DEFAULT 0,
    avg_pass_accuracy REAL DEFAULT 0,
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE (team_id, season_id, league_id)
);
CREATE INDEX idx_football_team_stats_team ON football_team_stats(team_id);
CREATE INDEX idx_football_team_stats_season ON football_team_stats(season_id);
CREATE INDEX idx_football_team_stats_league ON football_team_stats(league_id);
CREATE INDEX idx_football_team_stats_points ON football_team_stats(season_id, league_id, points DESC);

-- ============================================
-- PERCENTILE CACHE TABLE
-- ============================================

CREATE TABLE IF NOT EXISTS percentile_cache (
    id SERIAL PRIMARY KEY,
    entity_type TEXT NOT NULL,  -- 'player' or 'team'
    entity_id INTEGER NOT NULL,
    sport_id TEXT NOT NULL REFERENCES sports(id),
    season_id INTEGER NOT NULL REFERENCES seasons(id),
    stat_category TEXT NOT NULL,
    stat_value REAL,
    percentile REAL,  -- 0-100
    rank INTEGER,
    sample_size INTEGER,
    comparison_group TEXT,
    calculated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE (entity_type, entity_id, sport_id, season_id, stat_category)
);
CREATE INDEX idx_percentile_entity ON percentile_cache(entity_type, entity_id);
CREATE INDEX idx_percentile_lookup ON percentile_cache(sport_id, season_id, stat_category);
CREATE INDEX idx_percentile_ranking ON percentile_cache(sport_id, season_id, stat_category, percentile DESC);

-- ============================================
-- SYNC LOG TABLE
-- ============================================

CREATE TABLE IF NOT EXISTS sync_log (
    id SERIAL PRIMARY KEY,
    sport_id TEXT NOT NULL REFERENCES sports(id),
    league_id INTEGER REFERENCES leagues(id),
    sync_type TEXT NOT NULL,  -- 'full', 'incremental', 'percentile'
    entity_type TEXT,
    season_id INTEGER,
    started_at TIMESTAMPTZ DEFAULT NOW(),
    completed_at TIMESTAMPTZ,
    records_processed INTEGER DEFAULT 0,
    records_inserted INTEGER DEFAULT 0,
    records_updated INTEGER DEFAULT 0,
    status TEXT DEFAULT 'running',  -- 'running', 'completed', 'failed'
    error_message TEXT
);
CREATE INDEX idx_sync_log_sport ON sync_log(sport_id);
CREATE INDEX idx_sync_log_status ON sync_log(status);
CREATE INDEX idx_sync_log_started ON sync_log(started_at DESC);

-- ============================================
-- ENTITIES MINIMAL (for autocomplete)
-- ============================================

CREATE TABLE IF NOT EXISTS entities_minimal (
    id SERIAL PRIMARY KEY,
    entity_type TEXT NOT NULL,  -- 'team' or 'player'
    entity_id INTEGER NOT NULL,
    sport_id TEXT NOT NULL REFERENCES sports(id),
    league_id INTEGER REFERENCES leagues(id),
    name TEXT NOT NULL,
    normalized_name TEXT NOT NULL,
    tokens TEXT,  -- Space-separated search tokens
    UNIQUE (entity_type, entity_id)
);
CREATE INDEX idx_entities_type_sport ON entities_minimal(entity_type, sport_id);
CREATE INDEX idx_entities_league ON entities_minimal(league_id);
CREATE INDEX idx_entities_normalized ON entities_minimal(normalized_name);
CREATE INDEX idx_entities_tokens ON entities_minimal USING gin(to_tsvector('english', tokens));

-- ============================================
-- SET SCHEMA VERSION
-- ============================================

INSERT INTO meta (key, value) VALUES ('schema_version', '3.0')
ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW();
```

---

## 3. Connection Layer Updates

### 3.1 New Connection Module

Create `src/scoracle_data/pg_connection.py`:

```python
"""PostgreSQL connection manager for Neon."""

import os
from contextlib import contextmanager
from typing import Any, Generator, Optional

import psycopg
from psycopg.rows import dict_row
from psycopg_pool import ConnectionPool


class PostgresDB:
    """PostgreSQL database connection manager for Neon."""

    def __init__(self, connection_string: Optional[str] = None):
        self.connection_string = connection_string or os.environ.get("DATABASE_URL")
        if not self.connection_string:
            raise ValueError("DATABASE_URL environment variable required")

        # Initialize connection pool
        self._pool = ConnectionPool(
            self.connection_string,
            min_size=2,
            max_size=int(os.environ.get("DATABASE_POOL_SIZE", 10)),
            kwargs={"row_factory": dict_row}
        )

    @contextmanager
    def connection(self) -> Generator[psycopg.Connection, None, None]:
        """Get a connection from the pool."""
        with self._pool.connection() as conn:
            yield conn

    @contextmanager
    def cursor(self) -> Generator[psycopg.Cursor, None, None]:
        """Get a cursor for manual operations."""
        with self.connection() as conn:
            with conn.cursor() as cur:
                yield cur

    @contextmanager
    def transaction(self) -> Generator[psycopg.Connection, None, None]:
        """Execute within a transaction."""
        with self.connection() as conn:
            try:
                yield conn
                conn.commit()
            except Exception:
                conn.rollback()
                raise

    def execute(self, query: str, params: tuple = ()) -> None:
        """Execute a query without returning results."""
        with self.cursor() as cur:
            cur.execute(query, params)

    def fetchone(self, query: str, params: tuple = ()) -> Optional[dict[str, Any]]:
        """Fetch a single row as a dictionary."""
        with self.cursor() as cur:
            cur.execute(query, params)
            return cur.fetchone()

    def fetchall(self, query: str, params: tuple = ()) -> list[dict[str, Any]]:
        """Fetch all rows as list of dictionaries."""
        with self.cursor() as cur:
            cur.execute(query, params)
            return cur.fetchall()

    def executemany(self, query: str, params_list: list[tuple]) -> None:
        """Execute a query with multiple parameter sets."""
        with self.connection() as conn:
            with conn.cursor() as cur:
                cur.executemany(query, params_list)
            conn.commit()

    def close(self) -> None:
        """Close the connection pool."""
        self._pool.close()

    # ==========================================
    # High-level query methods (matching StatsDB interface)
    # ==========================================

    def get_season_id(self, sport_id: str, season_year: int) -> Optional[int]:
        """Get season ID from sport and year."""
        result = self.fetchone(
            "SELECT id FROM seasons WHERE sport_id = %s AND season_year = %s",
            (sport_id, season_year)
        )
        return result["id"] if result else None

    def get_current_season(self, sport_id: str) -> Optional[dict]:
        """Get current season for a sport."""
        return self.fetchone(
            "SELECT * FROM seasons WHERE sport_id = %s AND is_current = true",
            (sport_id,)
        )

    def get_player(self, player_id: int, sport_id: str) -> Optional[dict]:
        """Get player by ID."""
        return self.fetchone(
            "SELECT * FROM players WHERE id = %s AND sport_id = %s",
            (player_id, sport_id)
        )

    def get_team(self, team_id: int, sport_id: str) -> Optional[dict]:
        """Get team by ID."""
        return self.fetchone(
            "SELECT * FROM teams WHERE id = %s AND sport_id = %s",
            (team_id, sport_id)
        )

    def get_meta(self, key: str) -> Optional[str]:
        """Get metadata value."""
        result = self.fetchone("SELECT value FROM meta WHERE key = %s", (key,))
        return result["value"] if result else None

    def set_meta(self, key: str, value: str) -> None:
        """Set metadata value."""
        self.execute(
            """
            INSERT INTO meta (key, value, updated_at) VALUES (%s, %s, NOW())
            ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()
            """,
            (key, value)
        )


# Global instance
_db_instance: Optional[PostgresDB] = None


def get_postgres_db() -> PostgresDB:
    """Get the global PostgreSQL database instance."""
    global _db_instance
    if _db_instance is None:
        _db_instance = PostgresDB()
    return _db_instance
```

### 3.2 Parameter Placeholder Changes

**SQLite uses `?` placeholders, PostgreSQL uses `%s`:**

| Location | Change Required |
|----------|-----------------|
| All `fetchone()` calls | `?` → `%s` |
| All `fetchall()` calls | `?` → `%s` |
| All `execute()` calls | `?` → `%s` |
| Query builder output | Update `build_upsert()` |

**Example conversion:**

```python
# SQLite (current)
query = "SELECT * FROM players WHERE id = ? AND sport_id = ?"

# PostgreSQL (target)
query = "SELECT * FROM players WHERE id = %s AND sport_id = %s"
```

### 3.3 Files Requiring Updates

| File | Changes |
|------|---------|
| `connection.py` | Keep for SQLite backward compatibility OR replace entirely |
| `entity_repository.py` | Update all query parameter placeholders |
| `queries/players.py` | Update query placeholders |
| `queries/teams.py` | Update query placeholders |
| `percentiles/calculator.py` | Update query placeholders |
| `seeders/*.py` | Update all INSERT/UPDATE queries |
| `query_builder.py` | Update to generate `%s` placeholders |

---

## 4. Query Pattern Migration

### 4.1 UPSERT Syntax (Compatible!)

PostgreSQL's `ON CONFLICT` syntax is nearly identical to SQLite:

```sql
-- Both SQLite and PostgreSQL
INSERT INTO percentile_cache (entity_type, entity_id, sport_id, season_id, stat_category, ...)
VALUES (%s, %s, %s, %s, %s, ...)
ON CONFLICT (entity_type, entity_id, sport_id, season_id, stat_category)
DO UPDATE SET
    stat_value = EXCLUDED.stat_value,
    percentile = EXCLUDED.percentile,
    rank = EXCLUDED.rank;
```

### 4.2 INSERT OR REPLACE → ON CONFLICT

```python
# SQLite (current)
"INSERT OR REPLACE INTO table (...) VALUES (...)"

# PostgreSQL (target)
"INSERT INTO table (...) VALUES (...) ON CONFLICT (key_columns) DO UPDATE SET ..."
```

### 4.3 Date/Time Functions

```sql
-- SQLite: Unix timestamp
strftime('%s', 'now')

-- PostgreSQL: Unix timestamp
EXTRACT(EPOCH FROM NOW())::BIGINT

-- PostgreSQL: Native timestamp (recommended)
NOW()
```

### 4.4 Query Builder Updates

Update `query_builder.py`:

```python
class UpsertQueryBuilder:
    @staticmethod
    def build_upsert(
        table: str,
        columns: list[str],
        conflict_keys: list[str],
    ) -> str:
        """Generate PostgreSQL UPSERT query."""
        placeholders = ", ".join(["%s"] * len(columns))
        column_list = ", ".join(columns)

        # Build SET clause for non-key columns
        update_columns = [c for c in columns if c not in conflict_keys]
        set_clause = ", ".join(
            f"{col} = EXCLUDED.{col}" for col in update_columns
        )

        return f"""
            INSERT INTO {table} ({column_list})
            VALUES ({placeholders})
            ON CONFLICT ({", ".join(conflict_keys)})
            DO UPDATE SET {set_clause}
        """
```

---

## 5. Percentile Optimization

### 5.1 PostgreSQL Native Percentile Functions

PostgreSQL provides powerful window functions for percentile calculations:

| Function | Description |
|----------|-------------|
| `PERCENT_RANK()` | Relative rank as percentile (0 to 1) |
| `CUME_DIST()` | Cumulative distribution |
| `NTILE(n)` | Divide into n buckets |
| `RANK()` | Rank with gaps |
| `DENSE_RANK()` | Rank without gaps |

### 5.2 New Percentile Calculation (Database-Side)

Create `src/scoracle_data/percentiles/pg_calculator.py`:

```python
"""PostgreSQL-native percentile calculator using window functions."""

from typing import Optional
from ..pg_connection import get_postgres_db


class PostgresPercentileCalculator:
    """Calculate percentiles using PostgreSQL window functions."""

    def __init__(self, db=None):
        self.db = db or get_postgres_db()

    def calculate_player_percentiles_batch(
        self,
        sport_id: str,
        season_id: int,
        stat_columns: list[str],
        inverse_stats: set[str],
        min_sample_size: int = 30,
    ) -> int:
        """
        Calculate percentiles for all players in a season using native SQL.

        Uses PERCENT_RANK() window function for efficient batch calculation.
        Returns number of records processed.
        """
        table = self._get_stats_table(sport_id, "player")

        for stat in stat_columns:
            order_direction = "ASC" if stat in inverse_stats else "DESC"

            query = f"""
                INSERT INTO percentile_cache (
                    entity_type, entity_id, sport_id, season_id, stat_category,
                    stat_value, percentile, rank, sample_size, comparison_group, calculated_at
                )
                SELECT
                    'player' as entity_type,
                    p.id as entity_id,
                    %s as sport_id,
                    %s as season_id,
                    %s as stat_category,
                    s.{stat} as stat_value,
                    ROUND((PERCENT_RANK() OVER (
                        PARTITION BY p.position_group
                        ORDER BY s.{stat} {order_direction}
                    ) * 100)::numeric, 1) as percentile,
                    RANK() OVER (
                        PARTITION BY p.position_group
                        ORDER BY s.{stat} {order_direction}
                    ) as rank,
                    COUNT(*) OVER (PARTITION BY p.position_group) as sample_size,
                    p.position_group as comparison_group,
                    NOW() as calculated_at
                FROM {table} s
                JOIN players p ON s.player_id = p.id
                WHERE s.season_id = %s
                  AND p.sport_id = %s
                  AND s.{stat} IS NOT NULL
                  AND (SELECT COUNT(*) FROM {table} s2
                       JOIN players p2 ON s2.player_id = p2.id
                       WHERE s2.season_id = %s
                         AND p2.position_group = p.position_group
                         AND s2.{stat} IS NOT NULL) >= %s
                ON CONFLICT (entity_type, entity_id, sport_id, season_id, stat_category)
                DO UPDATE SET
                    stat_value = EXCLUDED.stat_value,
                    percentile = EXCLUDED.percentile,
                    rank = EXCLUDED.rank,
                    sample_size = EXCLUDED.sample_size,
                    comparison_group = EXCLUDED.comparison_group,
                    calculated_at = EXCLUDED.calculated_at
            """

            self.db.execute(
                query,
                (sport_id, season_id, stat, season_id, sport_id, season_id, min_sample_size)
            )

        # Return count of processed records
        result = self.db.fetchone(
            """
            SELECT COUNT(*) as count FROM percentile_cache
            WHERE sport_id = %s AND season_id = %s AND entity_type = 'player'
            """,
            (sport_id, season_id)
        )
        return result["count"] if result else 0

    def calculate_team_percentiles_batch(
        self,
        sport_id: str,
        season_id: int,
        stat_columns: list[str],
        inverse_stats: set[str],
        league_id: Optional[int] = None,
    ) -> int:
        """
        Calculate percentiles for all teams in a season using native SQL.
        """
        table = self._get_stats_table(sport_id, "team")

        # For football, partition by league; otherwise by sport/season
        partition_clause = "PARTITION BY s.league_id" if sport_id == "FOOTBALL" else ""

        for stat in stat_columns:
            order_direction = "ASC" if stat in inverse_stats else "DESC"

            query = f"""
                INSERT INTO percentile_cache (
                    entity_type, entity_id, sport_id, season_id, stat_category,
                    stat_value, percentile, rank, sample_size, comparison_group, calculated_at
                )
                SELECT
                    'team' as entity_type,
                    t.id as entity_id,
                    %s as sport_id,
                    %s as season_id,
                    %s as stat_category,
                    s.{stat} as stat_value,
                    ROUND((PERCENT_RANK() OVER (
                        {partition_clause}
                        ORDER BY s.{stat} {order_direction}
                    ) * 100)::numeric, 1) as percentile,
                    RANK() OVER (
                        {partition_clause}
                        ORDER BY s.{stat} {order_direction}
                    ) as rank,
                    COUNT(*) OVER ({partition_clause}) as sample_size,
                    COALESCE(l.name, %s) as comparison_group,
                    NOW() as calculated_at
                FROM {table} s
                JOIN teams t ON s.team_id = t.id
                LEFT JOIN leagues l ON t.league_id = l.id
                WHERE s.season_id = %s
                  AND t.sport_id = %s
                  AND s.{stat} IS NOT NULL
                ON CONFLICT (entity_type, entity_id, sport_id, season_id, stat_category)
                DO UPDATE SET
                    stat_value = EXCLUDED.stat_value,
                    percentile = EXCLUDED.percentile,
                    rank = EXCLUDED.rank,
                    sample_size = EXCLUDED.sample_size,
                    comparison_group = EXCLUDED.comparison_group,
                    calculated_at = EXCLUDED.calculated_at
            """

            self.db.execute(query, (sport_id, season_id, stat, sport_id, season_id, sport_id))

        result = self.db.fetchone(
            """
            SELECT COUNT(*) as count FROM percentile_cache
            WHERE sport_id = %s AND season_id = %s AND entity_type = 'team'
            """,
            (sport_id, season_id)
        )
        return result["count"] if result else 0

    def get_player_percentile_live(
        self,
        player_id: int,
        sport_id: str,
        season_id: int,
        stat_name: str,
        inverse: bool = False,
    ) -> Optional[dict]:
        """
        Calculate a single player's percentile on-demand (no caching).

        Useful for real-time calculations without cache refresh.
        """
        table = self._get_stats_table(sport_id, "player")
        order_direction = "ASC" if inverse else "DESC"

        query = f"""
            WITH player_stats AS (
                SELECT
                    p.id,
                    p.position_group,
                    s.{stat_name} as stat_value,
                    PERCENT_RANK() OVER (
                        PARTITION BY p.position_group
                        ORDER BY s.{stat_name} {order_direction}
                    ) as percentile_rank,
                    RANK() OVER (
                        PARTITION BY p.position_group
                        ORDER BY s.{stat_name} {order_direction}
                    ) as rank,
                    COUNT(*) OVER (PARTITION BY p.position_group) as sample_size
                FROM {table} s
                JOIN players p ON s.player_id = p.id
                WHERE s.season_id = %s
                  AND p.sport_id = %s
                  AND s.{stat_name} IS NOT NULL
            )
            SELECT
                stat_value,
                ROUND((percentile_rank * 100)::numeric, 1) as percentile,
                rank,
                sample_size,
                position_group as comparison_group
            FROM player_stats
            WHERE id = %s
        """

        return self.db.fetchone(query, (season_id, sport_id, player_id))

    def _get_stats_table(self, sport_id: str, entity_type: str) -> str:
        """Get the appropriate stats table for a sport."""
        tables = {
            ("NBA", "player"): "nba_player_stats",
            ("NBA", "team"): "nba_team_stats",
            ("NFL", "player"): "nfl_player_stats",
            ("NFL", "team"): "nfl_team_stats",
            ("FOOTBALL", "player"): "football_player_stats",
            ("FOOTBALL", "team"): "football_team_stats",
        }
        return tables.get((sport_id, entity_type), f"{sport_id.lower()}_{entity_type}_stats")
```

### 5.3 Performance Comparison

| Method | Current (Python) | New (PostgreSQL) |
|--------|------------------|------------------|
| Calculate 1 player percentile | ~50ms (query + Python) | ~5ms (single query) |
| Batch all players (500) | ~25s (500 queries) | ~500ms (1 query) |
| Batch all teams (30) | ~1.5s (30 queries) | ~50ms (1 query) |

**Expected improvement: 10-50x faster percentile calculations**

### 5.4 Advanced Percentile Views (Optional)

Create materialized views for frequently accessed percentile data:

```sql
-- Materialized view for NBA player percentiles
CREATE MATERIALIZED VIEW mv_nba_player_percentiles AS
SELECT
    p.id as player_id,
    p.full_name,
    p.position_group,
    t.name as team_name,
    s.season_id,
    s.points_per_game,
    ROUND((PERCENT_RANK() OVER (
        PARTITION BY p.position_group, s.season_id
        ORDER BY s.points_per_game DESC
    ) * 100)::numeric, 1) as ppg_percentile,
    s.rebounds_per_game,
    ROUND((PERCENT_RANK() OVER (
        PARTITION BY p.position_group, s.season_id
        ORDER BY s.rebounds_per_game DESC
    ) * 100)::numeric, 1) as rpg_percentile,
    s.assists_per_game,
    ROUND((PERCENT_RANK() OVER (
        PARTITION BY p.position_group, s.season_id
        ORDER BY s.assists_per_game DESC
    ) * 100)::numeric, 1) as apg_percentile
FROM nba_player_stats s
JOIN players p ON s.player_id = p.id
LEFT JOIN teams t ON p.current_team_id = t.id
WHERE s.points_per_game IS NOT NULL;

-- Refresh strategy
CREATE UNIQUE INDEX ON mv_nba_player_percentiles (player_id, season_id);

-- Refresh after data sync
REFRESH MATERIALIZED VIEW CONCURRENTLY mv_nba_player_percentiles;
```

---

## 6. Data Migration

### 6.1 Migration Script

Create `scripts/migrate_to_neon.py`:

```python
#!/usr/bin/env python3
"""Migrate data from SQLite to Neon PostgreSQL."""

import os
import sqlite3
from datetime import datetime

import psycopg
from psycopg.rows import dict_row


def migrate_table(
    sqlite_conn: sqlite3.Connection,
    pg_conn: psycopg.Connection,
    table: str,
    batch_size: int = 1000,
) -> int:
    """Migrate a single table from SQLite to PostgreSQL."""
    sqlite_conn.row_factory = sqlite3.Row
    cursor = sqlite_conn.cursor()

    # Get row count
    cursor.execute(f"SELECT COUNT(*) FROM {table}")
    total_rows = cursor.fetchone()[0]

    if total_rows == 0:
        print(f"  {table}: 0 rows (skipping)")
        return 0

    # Get column names
    cursor.execute(f"PRAGMA table_info({table})")
    columns = [row[1] for row in cursor.fetchall()]

    # Prepare PostgreSQL insert
    placeholders = ", ".join(["%s"] * len(columns))
    column_list = ", ".join(columns)
    insert_query = f"INSERT INTO {table} ({column_list}) VALUES ({placeholders}) ON CONFLICT DO NOTHING"

    # Migrate in batches
    migrated = 0
    cursor.execute(f"SELECT * FROM {table}")

    while True:
        rows = cursor.fetchmany(batch_size)
        if not rows:
            break

        # Convert to list of tuples
        data = [tuple(row) for row in rows]

        with pg_conn.cursor() as pg_cursor:
            pg_cursor.executemany(insert_query, data)
        pg_conn.commit()

        migrated += len(data)
        print(f"  {table}: {migrated}/{total_rows} rows migrated", end="\r")

    print(f"  {table}: {migrated} rows migrated")
    return migrated


def main():
    """Run the migration."""
    sqlite_path = os.environ.get("SQLITE_DB_PATH", "scoracle_data.db")
    pg_url = os.environ.get("DATABASE_URL")

    if not pg_url:
        raise ValueError("DATABASE_URL environment variable required")

    # Tables in dependency order
    tables = [
        "sports",
        "seasons",
        "leagues",
        "teams",
        "players",
        "player_teams",
        "nba_player_stats",
        "nba_team_stats",
        "nfl_player_stats",
        "nfl_team_stats",
        "football_player_stats",
        "football_team_stats",
        "percentile_cache",
        "sync_log",
        "entities_minimal",
        "meta",
    ]

    print(f"Migrating from {sqlite_path} to Neon PostgreSQL...")
    print(f"Started at: {datetime.now().isoformat()}")

    sqlite_conn = sqlite3.connect(sqlite_path)
    pg_conn = psycopg.connect(pg_url, row_factory=dict_row)

    total_migrated = 0

    try:
        for table in tables:
            try:
                count = migrate_table(sqlite_conn, pg_conn, table)
                total_migrated += count
            except Exception as e:
                print(f"  {table}: ERROR - {e}")
    finally:
        sqlite_conn.close()
        pg_conn.close()

    print(f"\nMigration complete!")
    print(f"Total rows migrated: {total_migrated}")
    print(f"Finished at: {datetime.now().isoformat()}")


if __name__ == "__main__":
    main()
```

### 6.2 Migration Steps

```bash
# 1. Export current SQLite data (backup)
sqlite3 scoracle_data.db ".backup backup_$(date +%Y%m%d).db"

# 2. Create PostgreSQL schema on Neon
psql $DATABASE_URL < migrations/007_postgresql_conversion.sql

# 3. Run data migration
python scripts/migrate_to_neon.py

# 4. Verify row counts
psql $DATABASE_URL -c "
SELECT
    'players' as table_name, COUNT(*) as rows FROM players
UNION ALL
SELECT 'teams', COUNT(*) FROM teams
UNION ALL
SELECT 'nba_player_stats', COUNT(*) FROM nba_player_stats
UNION ALL
SELECT 'nfl_player_stats', COUNT(*) FROM nfl_player_stats
UNION ALL
SELECT 'football_player_stats', COUNT(*) FROM football_player_stats
ORDER BY table_name;
"

# 5. Recalculate percentiles with new PostgreSQL engine
python -c "
from scoracle_data.percentiles.pg_calculator import PostgresPercentileCalculator
calc = PostgresPercentileCalculator()
for sport in ['NBA', 'NFL', 'FOOTBALL']:
    calc.calculate_player_percentiles_batch(sport, 2024, [...], {...})
"
```

---

## 7. Testing Strategy

### 7.1 Unit Tests

Create `tests/test_pg_connection.py`:

```python
"""Tests for PostgreSQL connection and queries."""

import pytest
from scoracle_data.pg_connection import PostgresDB


@pytest.fixture
def test_db():
    """Create a test database connection."""
    # Use test database or transaction rollback
    db = PostgresDB(os.environ.get("TEST_DATABASE_URL"))
    yield db
    db.close()


class TestPostgresDB:
    def test_fetchone_returns_dict(self, test_db):
        result = test_db.fetchone("SELECT 1 as value")
        assert isinstance(result, dict)
        assert result["value"] == 1

    def test_fetchall_returns_list(self, test_db):
        result = test_db.fetchall("SELECT generate_series(1, 3) as num")
        assert len(result) == 3
        assert all(isinstance(r, dict) for r in result)

    def test_upsert_works(self, test_db):
        # Test ON CONFLICT behavior
        test_db.execute("""
            INSERT INTO meta (key, value) VALUES ('test_key', 'value1')
            ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value
        """)
        result = test_db.fetchone("SELECT value FROM meta WHERE key = 'test_key'")
        assert result["value"] == "value1"
```

### 7.2 Integration Tests

```python
"""Integration tests comparing SQLite and PostgreSQL results."""

import pytest
from scoracle_data.connection import get_stats_db  # SQLite
from scoracle_data.pg_connection import get_postgres_db  # PostgreSQL


class TestDataConsistency:
    def test_player_counts_match(self):
        sqlite_db = get_stats_db()
        pg_db = get_postgres_db()

        sqlite_count = sqlite_db.fetchone("SELECT COUNT(*) as c FROM players")["c"]
        pg_count = pg_db.fetchone("SELECT COUNT(*) as c FROM players")["c"]

        assert sqlite_count == pg_count

    def test_percentile_calculations_match(self):
        # Compare Python-calculated vs PostgreSQL-calculated percentiles
        # Allow small floating-point differences
        pass
```

### 7.3 Performance Tests

```python
"""Performance comparison tests."""

import time
import pytest


class TestPerformance:
    def test_percentile_calculation_speed(self, pg_db):
        """PostgreSQL percentiles should be faster than Python."""
        from scoracle_data.percentiles.pg_calculator import PostgresPercentileCalculator

        calc = PostgresPercentileCalculator(pg_db)

        start = time.time()
        calc.calculate_player_percentiles_batch("NBA", 2024, [...], {...})
        pg_time = time.time() - start

        # Should complete in under 1 second for NBA
        assert pg_time < 1.0
```

---

## 8. Deployment Plan

### 8.1 Pre-Deployment Checklist

- [ ] Neon project created and configured
- [ ] Connection pooling enabled
- [ ] All code changes merged to main branch
- [ ] All tests passing
- [ ] Data migration script tested on staging
- [ ] Rollback procedure documented and tested

### 8.2 Deployment Steps

```
Phase 1: Preparation (Day 1)
├── Create Neon database
├── Run schema migration
├── Configure connection pooling
└── Update environment variables (staging)

Phase 2: Data Migration (Day 2)
├── Put service in maintenance mode
├── Run final SQLite backup
├── Execute data migration script
├── Verify row counts and data integrity
└── Recalculate all percentiles

Phase 3: Code Deployment (Day 2)
├── Deploy updated application code
├── Run smoke tests
├── Monitor error rates and latency
└── Remove maintenance mode

Phase 4: Validation (Days 3-5)
├── Monitor performance metrics
├── Compare query latencies
├── Verify percentile accuracy
└── Address any issues
```

### 8.3 Environment Configuration

```bash
# Production environment variables
DATABASE_URL=postgresql://user:password@ep-xxx.region.aws.neon.tech/scoracle_data?sslmode=require
DATABASE_POOL_SIZE=10
DATABASE_POOL_MAX_OVERFLOW=20

# Keep SQLite path for fallback
SQLITE_DB_PATH=/path/to/scoracle_data.db
USE_POSTGRESQL=true
```

---

## 9. Rollback Strategy

### 9.1 Quick Rollback (< 5 minutes)

```bash
# Switch back to SQLite by changing environment variable
export USE_POSTGRESQL=false

# Restart application
# Application will use SQLite connection
```

### 9.2 Full Rollback (if data corruption)

```bash
# 1. Stop application
# 2. Restore SQLite from backup
sqlite3 scoracle_data.db ".restore backup_20240101.db"

# 3. Set environment to use SQLite
export USE_POSTGRESQL=false

# 4. Restart application
```

### 9.3 Dual-Write Mode (Optional)

For zero-downtime migration, implement dual-write:

```python
class DualWriteDB:
    """Write to both SQLite and PostgreSQL during transition."""

    def __init__(self):
        self.sqlite = get_stats_db()
        self.postgres = get_postgres_db()

    def execute(self, query_sqlite: str, query_pg: str, params: tuple):
        """Execute on both databases."""
        self.sqlite.execute(query_sqlite, params)
        self.postgres.execute(query_pg, params)
```

---

## 10. Post-Migration Optimization

### 10.1 Index Optimization

After migration, analyze query patterns and add indexes:

```sql
-- Find missing indexes
SELECT
    schemaname || '.' || relname AS table,
    seq_scan,
    idx_scan,
    seq_tup_read,
    idx_tup_fetch
FROM pg_stat_user_tables
WHERE seq_scan > idx_scan
ORDER BY seq_tup_read DESC;

-- Add indexes for common queries
CREATE INDEX CONCURRENTLY idx_player_stats_composite
ON nba_player_stats(season_id, points_per_game DESC);
```

### 10.2 Query Performance Analysis

```sql
-- Enable query logging
ALTER SYSTEM SET log_min_duration_statement = 100;  -- Log queries > 100ms

-- Use EXPLAIN ANALYZE for slow queries
EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT)
SELECT ...;
```

### 10.3 Connection Pooling Tuning

```python
# Monitor pool usage
from scoracle_data.pg_connection import get_postgres_db

db = get_postgres_db()
pool = db._pool

print(f"Pool size: {pool.get_stats()}")
# Adjust min_size and max_size based on usage patterns
```

### 10.4 Neon-Specific Optimizations

```sql
-- Enable Neon's autoscaling features
-- (configured in Neon console)

-- Use Neon's branching for staging/testing
-- Create branch from production for safe testing

-- Monitor compute usage
-- Scale up/down based on traffic patterns
```

---

## Appendix A: File Change Summary

| File | Action | Description |
|------|--------|-------------|
| `pyproject.toml` | Modify | Add psycopg3, psycopg-pool |
| `src/scoracle_data/pg_connection.py` | Create | New PostgreSQL connection manager |
| `src/scoracle_data/connection.py` | Keep/Modify | Keep for backward compatibility |
| `src/scoracle_data/percentiles/pg_calculator.py` | Create | PostgreSQL-native percentile calculator |
| `src/scoracle_data/percentiles/calculator.py` | Keep | Original for fallback |
| `src/scoracle_data/entity_repository.py` | Modify | Update placeholders, add DB toggle |
| `src/scoracle_data/queries/players.py` | Modify | Update query placeholders |
| `src/scoracle_data/queries/teams.py` | Modify | Update query placeholders |
| `src/scoracle_data/query_builder.py` | Modify | Generate PostgreSQL syntax |
| `src/scoracle_data/seeders/*.py` | Modify | Update all INSERT queries |
| `migrations/007_postgresql_conversion.sql` | Create | Consolidated PostgreSQL schema |
| `scripts/migrate_to_neon.py` | Create | Data migration script |
| `tests/test_pg_*.py` | Create | PostgreSQL-specific tests |

---

## Appendix B: Estimated Timeline

| Phase | Tasks | Duration |
|-------|-------|----------|
| **Setup** | Neon project, dependencies, env vars | 1-2 hours |
| **Schema** | Convert and test PostgreSQL schema | 2-4 hours |
| **Connection** | Implement pg_connection.py | 2-3 hours |
| **Queries** | Update all query placeholders | 3-4 hours |
| **Percentiles** | Implement PostgreSQL calculator | 4-6 hours |
| **Testing** | Unit, integration, performance tests | 4-6 hours |
| **Migration** | Data migration script and execution | 2-3 hours |
| **Deployment** | Production rollout | 2-4 hours |
| **Optimization** | Post-migration tuning | 2-4 hours |

**Total Estimated Effort: 22-36 hours**

---

## Appendix C: Key Benefits Summary

1. **Performance**: 10-50x faster percentile calculations using native `PERCENT_RANK()`
2. **Scalability**: Neon's serverless architecture scales automatically
3. **Reliability**: PostgreSQL's ACID compliance and proven stability
4. **Features**: Access to advanced PostgreSQL features (CTEs, window functions, JSON)
5. **Maintenance**: No local database file management
6. **Branching**: Neon's database branching for safe staging/testing
