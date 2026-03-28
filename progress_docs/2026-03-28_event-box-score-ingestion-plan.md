# Event Box Score Ingestion Plan

**Date:** 2026-03-28
**Status:** In progress (schema + fixture-level seeding flow implemented; provider payload hardening and parity validation pending)

## Problem Statement

The current ingestion pipeline upserts **pre-aggregated season averages** (or season totals broken down per game) into `player_stats` and `team_stats`. This has fundamental limitations:

1. **Fragile dependency on provider curation** — we rely on BDL/SportMonks to compute per-game averages and deliver them in a pre-shaped stat array. If the provider changes how they aggregate, our data silently breaks.
2. **No game-level granularity** — we cannot show per-game performance, streaks, form, or recent-N-game trends because we only store the season-level rollup.
3. **Re-seeding is all-or-nothing** — when a fixture completes, we re-fetch the *entire* season average for every player/team in the sport, even though only 20-30 players were involved in that game.
4. **Cannot go back in time** — backfilling historical seasons requires the provider to still serve those season averages. Box scores are more widely available historically.
5. **Season averages hide the true source data** — averages are derived; box scores are the atomic, immutable record of what happened.

## Goal

Shift to **event-level box score ingestion** as the core data primitive. Box scores are the true CORE data. Scoracle then provides value by using its schema (SQL triggers, views, functions) to curate that raw data into the aggregated stats, percentiles, and page payloads the frontend consumes.

```
Provider API  --(box score)--> Python Seeder --(upsert raw)--> Postgres
                                                                  |
                                                        SQL triggers/views
                                                                  |
                                                   Aggregated stats, percentiles,
                                                   standings, leaders, player pages
                                                                  |
                                                            Go API --> Frontend
```

## Current Architecture (What Changes)

### Current Data Flow

```
Provider /season_averages  -->  Python  -->  player_stats (season-level JSONB)
Provider /standings        -->  Python  -->  team_stats (season-level JSONB)
                                               |
                                        SQL triggers compute derived stats
                                        SQL function computes percentiles
```

**Tables affected by this migration:**

| Table | Current Role | New Role |
|-------|-------------|----------|
| `player_stats` | Stores season averages from provider | Stores **aggregated** stats computed from box scores (SQL materialization) |
| `team_stats` | Stores season standings from provider | Stores **aggregated** stats computed from box scores + standings still provider-fed |
| `fixtures` | Scheduling + seed tracking | Scheduling + seed tracking + **links to box scores** |
| **`event_box_scores`** (NEW) | N/A | Stores per-game, per-player raw stat lines |
| **`event_team_stats`** (NEW) | N/A | Stores per-game, per-team raw stat lines (score, team-level stats) |

### What Stays the Same

- `teams`, `players` tables (unchanged)
- `stat_definitions`, `provider_stat_mappings` (extended, not replaced)
- `leagues`, `sports`, `provider_seasons` (unchanged)
- All Go API prepared statements (they read from views, which we'll rebuild on top of aggregated data)
- Sport-specific views (`nba.player`, `nfl.standings`, etc.) — their source changes but their output shape stays identical
- `finalize_fixture()` orchestration pattern (still the handoff point)
- Notification/percentile infrastructure

---

## Phase 1: New Schema — `event_box_scores` and `event_team_stats`

### 1a. `event_box_scores` Table

The atomic record: one row per player per fixture.

```sql
CREATE TABLE IF NOT EXISTS event_box_scores (
    id BIGSERIAL PRIMARY KEY,
    fixture_id INTEGER NOT NULL REFERENCES fixtures(id),
    player_id INTEGER NOT NULL,
    team_id INTEGER NOT NULL,
    sport TEXT NOT NULL REFERENCES sports(id),
    season INTEGER NOT NULL,
    league_id INTEGER NOT NULL DEFAULT 0,
    minutes_played NUMERIC,         -- common enough to be a column
    stats JSONB NOT NULL DEFAULT '{}',   -- all other stat key/values
    raw_response JSONB,             -- full provider response for audit
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(fixture_id, player_id)   -- one line per player per game
);

CREATE INDEX idx_ebs_player_season ON event_box_scores(player_id, sport, season);
CREATE INDEX idx_ebs_fixture ON event_box_scores(fixture_id);
CREATE INDEX idx_ebs_team_season ON event_box_scores(team_id, sport, season);
CREATE INDEX idx_ebs_sport_season ON event_box_scores(sport, season);
```

### 1b. `event_team_stats` Table

Per-game team-level stats (score, team shooting splits, etc.).

```sql
CREATE TABLE IF NOT EXISTS event_team_stats (
    id BIGSERIAL PRIMARY KEY,
    fixture_id INTEGER NOT NULL REFERENCES fixtures(id),
    team_id INTEGER NOT NULL,
    sport TEXT NOT NULL REFERENCES sports(id),
    season INTEGER NOT NULL,
    league_id INTEGER NOT NULL DEFAULT 0,
    score INTEGER,
    stats JSONB NOT NULL DEFAULT '{}',
    raw_response JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(fixture_id, team_id)
);

CREATE INDEX idx_ets_team_season ON event_team_stats(team_id, sport, season);
CREATE INDEX idx_ets_fixture ON event_team_stats(fixture_id);
```

### 1c. Stat Key Normalization

Reuse the existing `normalize_stat_keys()` trigger on the new tables:

```sql
CREATE TRIGGER trg_a_normalize_ebs_stats
    BEFORE INSERT OR UPDATE OF stats ON event_box_scores
    FOR EACH ROW EXECUTE FUNCTION normalize_stat_keys();

CREATE TRIGGER trg_a_normalize_ets_stats
    BEFORE INSERT OR UPDATE OF stats ON event_team_stats
    FOR EACH ROW EXECUTE FUNCTION normalize_stat_keys();
```

The existing `provider_stat_mappings` table works as-is. New box-score-specific raw keys can be added there as discovered.

---

## Phase 2: Aggregation Layer (SQL)

This is where Scoracle provides value. Raw box scores go in; curated season stats come out.

### 2a. Aggregation Functions

Create sport-specific aggregation functions that compute season averages from box scores. These replace the current "upsert provider averages" approach.

**Example — NBA player season stats from box scores:**

```sql
CREATE OR REPLACE FUNCTION nba.aggregate_player_season(
    p_player_id INTEGER,
    p_season INTEGER,
    p_league_id INTEGER DEFAULT 0
)
RETURNS JSONB AS $$
    SELECT jsonb_build_object(
        'games_played', COUNT(*),
        'minutes', ROUND(AVG(minutes_played), 1),
        'pts', ROUND(AVG((stats->>'pts')::numeric), 1),
        'reb', ROUND(AVG((stats->>'reb')::numeric), 1),
        'ast', ROUND(AVG((stats->>'ast')::numeric), 1),
        'stl', ROUND(AVG((stats->>'stl')::numeric), 1),
        'blk', ROUND(AVG((stats->>'blk')::numeric), 1),
        'turnover', ROUND(AVG((stats->>'turnover')::numeric), 1),
        'fgm', ROUND(AVG((stats->>'fgm')::numeric), 1),
        'fga', ROUND(AVG((stats->>'fga')::numeric), 1),
        'fg3m', ROUND(AVG((stats->>'fg3m')::numeric), 1),
        'fg3a', ROUND(AVG((stats->>'fg3a')::numeric), 1),
        'ftm', ROUND(AVG((stats->>'ftm')::numeric), 1),
        'fta', ROUND(AVG((stats->>'fta')::numeric), 1),
        'fg_pct', ROUND(SUM((stats->>'fgm')::numeric) /
                        NULLIF(SUM((stats->>'fga')::numeric), 0) * 100, 1),
        'fg3_pct', ROUND(SUM((stats->>'fg3m')::numeric) /
                         NULLIF(SUM((stats->>'fg3a')::numeric), 0) * 100, 1),
        'ft_pct', ROUND(SUM((stats->>'ftm')::numeric) /
                        NULLIF(SUM((stats->>'fta')::numeric), 0) * 100, 1),
        'pf', ROUND(AVG((stats->>'pf')::numeric), 1),
        'plus_minus', ROUND(AVG((stats->>'plus_minus')::numeric), 1),
        'oreb', ROUND(AVG((stats->>'oreb')::numeric), 1),
        'dreb', ROUND(AVG((stats->>'dreb')::numeric), 1)
    )
    FROM event_box_scores
    WHERE player_id = p_player_id
      AND sport = 'NBA'
      AND season = p_season
      AND league_id = p_league_id;
$$ LANGUAGE sql STABLE;
```

Similar functions for NFL and Football, each respecting sport-specific stat keys and aggregation logic (totals for NFL passing yards, per-90 for football, etc.).

### 2b. Reaggregation on Fixture Finalization

Update `finalize_fixture()` to reaggregate stats for only the players/teams involved in that fixture:

```sql
-- Pseudocode for the updated finalize_fixture():
-- 1. Get all player_ids from event_box_scores WHERE fixture_id = p_fixture_id
-- 2. For each player, recompute their season aggregation and upsert into player_stats
-- 3. Get both team_ids from the fixture
-- 4. For each team, recompute their season aggregation and upsert into team_stats
-- 5. Recalculate percentiles (existing logic)
-- 6. Refresh materialized views (existing logic)
-- 7. Mark fixture seeded (existing logic)
```

This is a major improvement: instead of re-fetching the entire league's season averages from the provider, we only reaggregate the ~20-30 players involved in one game.

### 2c. `player_stats` Becomes a Materialized Aggregate

`player_stats` and `team_stats` continue to exist with their current schema and are still the source for views, leaders, percentiles, and the Go API. The difference is that they are now **populated by SQL aggregation** from `event_box_scores` rather than directly by the Python seeder.

The existing derived-stat triggers (`nba.compute_derived_player_stats`, etc.) continue to fire on `player_stats` upsert and compute per-36, true shooting %, efficiency, etc. No changes needed.

---

## Phase 3: Python Seeder Changes

### 3a. New Provider Handlers — Box Score Endpoints

Each provider needs a new handler method to fetch box scores for a specific game.

**BDL (NBA + NFL):**
- NBA: `GET /box_scores?date=YYYY-MM-DD` or `GET /box_scores?game_ids[]=123`
  - Returns player stat lines for each game
- NFL: `GET /box_scores?game_ids[]=123` (similar pattern)

**SportMonks (Football):**
- `GET /fixtures/{fixture_id}?include=lineups.details.type;events;scores`
  - Player lineups with detailed stats per match
  - Already partially explored via the existing statistics include pattern

### 3b. New Canonical Models

```python
@dataclass
class EventBoxScore:
    """One player's stat line for one game."""
    fixture_id: int
    player_id: int
    team_id: int
    player: Player | None = None
    minutes_played: float | None = None
    stats: dict[str, Any] = field(default_factory=dict)
    raw: dict[str, Any] | None = None

@dataclass
class EventTeamStats:
    """One team's stat line for one game."""
    fixture_id: int
    team_id: int
    score: int | None = None
    stats: dict[str, Any] = field(default_factory=dict)
    raw: dict[str, Any] | None = None
```

### 3c. New Upsert Functions

```python
def upsert_event_box_score(
    conn, sport, season, league_id, data: EventBoxScore
) -> None:
    """Upsert a single player box score line."""
    conn.execute("""
        INSERT INTO event_box_scores (
            fixture_id, player_id, team_id, sport, season,
            league_id, minutes_played, stats, raw_response
        ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s)
        ON CONFLICT (fixture_id, player_id) DO UPDATE SET
            team_id = EXCLUDED.team_id,
            minutes_played = EXCLUDED.minutes_played,
            stats = EXCLUDED.stats,
            raw_response = EXCLUDED.raw_response,
            updated_at = NOW()
    """, (...))

def upsert_event_team_stats(
    conn, sport, season, league_id, data: EventTeamStats
) -> None:
    """Upsert a single team game stat line."""
    conn.execute("""
        INSERT INTO event_team_stats (
            fixture_id, team_id, sport, season, league_id,
            score, stats, raw_response
        ) VALUES (%s,%s,%s,%s,%s,%s,%s,%s)
        ON CONFLICT (fixture_id, team_id) DO UPDATE SET
            score = EXCLUDED.score,
            stats = EXCLUDED.stats,
            raw_response = EXCLUDED.raw_response,
            updated_at = NOW()
    """, (...))
```

### 3d. Updated Seed Orchestration

The seed flow for a fixture changes from:

```
Old: fixture ready -> fetch ALL season averages -> upsert player_stats/team_stats -> finalize
```

To:

```
New: fixture ready -> fetch box score for THIS game -> upsert event_box_scores/event_team_stats
     -> finalize (SQL reaggregates player_stats/team_stats from box scores) -> percentiles
```

**Updated `seed_nba.py` (conceptual):**

```python
def seed_nba_fixture(conn, handler, fixture):
    """Seed a single NBA game's box score."""
    result = SeedResult()

    # Fetch box score for this specific game
    box_score = handler.get_box_score(fixture.external_id)

    # Upsert each player's game line
    for player_line in box_score.player_lines:
        upsert_player(conn, "NBA", player_line.player)
        upsert_event_box_score(conn, "NBA", fixture.season, 0, player_line)
        result.player_stats_upserted += 1

    # Upsert team game stats
    for team_line in box_score.team_lines:
        upsert_event_team_stats(conn, "NBA", fixture.season, 0, team_line)
        result.team_stats_upserted += 1

    return result
```

### 3e. CLI Changes

- **`process`** command: same flow, but now calls fixture-specific box score fetch instead of full-season fetch
- **`seed-fixture`** command: now truly seeds only that one fixture's box score
- **New: `backfill`** command: fetch historical box scores for a date range or season
  ```
  scoracle-seed backfill nba --season 2024 --from 2024-10-22 --to 2025-04-13
  ```

---

## Phase 4: Fixture Loading (implement `load-fixtures`)

The currently-stubbed `load-fixtures` CLI command becomes critical because box score ingestion is fixture-driven.

### 4a. BDL Games Endpoint

```python
# NBA: GET /games?seasons[]=2025&dates[]=2025-03-28
# NFL: GET /games?seasons[]=2025&weeks[]=1
# Returns: game_id, home_team, away_team, date, status, score
```

Map each game to a `fixtures` row using `upsert_fixture()`.

### 4b. SportMonks Fixtures Endpoint

```python
# GET /fixtures?filters=fixtureSeasons:{season_id}&include=participants
# Returns: fixture_id, localteam_id, visitorteam_id, starting_at, etc.
```

### 4c. Schedule

Run `load-fixtures` daily (or weekly) to populate the `fixtures` table. The existing `process` cron then picks up completed fixtures and fetches their box scores.

---

## Phase 5: Migration Strategy

### 5a. Parallel Run (Recommended)

Run both pipelines simultaneously during transition:

1. Deploy new tables (`event_box_scores`, `event_team_stats`)
2. Deploy new box score handlers alongside existing season-average handlers
3. For each fixture processed:
   - Fetch box score and upsert into `event_box_scores` (new)
   - Also fetch season averages and upsert into `player_stats` (old, as safety net)
4. Build aggregation functions and validate:
   - Compare SQL-aggregated stats from box scores vs. provider season averages
   - Identify and resolve discrepancies
5. Once validated, cut over:
   - `player_stats` populated exclusively from box score aggregation
   - Remove season-average fetching from seeders

### 5b. Backfill Plan

For historical data:

1. Use `load-fixtures` to populate fixtures for past seasons
2. Run `backfill` command to fetch box scores for each historical fixture
3. Reaggregate `player_stats` from the backfilled box scores
4. Validate against known season totals/averages

### 5c. Cutover Checklist

- [ ] `event_box_scores` table deployed and indexed
- [ ] `event_team_stats` table deployed and indexed
- [ ] Normalization triggers attached to new tables
- [ ] Box score provider handlers implemented (BDL NBA, BDL NFL, SportMonks Football)
- [ ] `load-fixtures` implemented for all three providers
- [ ] Aggregation functions implemented per sport
- [ ] `finalize_fixture()` updated to reaggregate from box scores
- [ ] Parallel run validates aggregated stats match provider averages
- [ ] Backfill completed for current season
- [ ] Season-average fetch code removed from seeders
- [ ] Old `seed_nba`/`seed_nfl`/`seed_football` full-season flows deprecated

---

## Phase 6: Future Capabilities Unlocked

With event-level box scores as the foundation, these become straightforward:

| Capability | How |
|-----------|-----|
| **Last N games form** | `SELECT stats FROM event_box_scores WHERE player_id = ? ORDER BY fixture.start_time DESC LIMIT 5` |
| **Head-to-head history** | Join `event_box_scores` with `fixtures` to compare player performance in matchups |
| **Game log pages** | New API endpoint serving raw game logs directly from `event_box_scores` |
| **Hot/cold streaks** | Window functions over `event_box_scores` ordered by date |
| **Home vs. away splits** | Join with `fixtures` to partition by `home_team_id` / `away_team_id` |
| **Per-round/week stats** | Filter by `fixtures.round` |
| **Custom date-range aggregation** | Frontend requests stats for any date window, SQL aggregates on the fly |
| **Historical seasons** | Backfill box scores as far back as providers support |

---

## Files to Create/Modify

### New Files
| File | Purpose |
|------|---------|
| `sql/event_tables.sql` | `event_box_scores` + `event_team_stats` DDL, indexes, triggers |
| `seed/scoracle_seed/event_models.py` | `EventBoxScore`, `EventTeamStats` dataclasses |
| `seed/scoracle_seed/event_upsert.py` | Upsert functions for event tables |
| `seed/scoracle_seed/bdl_box_scores.py` | BDL box score handler (NBA + NFL shared or separate) |
| `seed/scoracle_seed/sportmonks_box_scores.py` | SportMonks fixture-level stat handler |

### Modified Files
| File | Changes |
|------|---------|
| `sql/shared.sql` | Add `event_box_scores`, `event_team_stats` tables; update `finalize_fixture()` |
| `sql/nba.sql` | Add `nba.aggregate_player_season()`, `nba.aggregate_team_season()` |
| `sql/nfl.sql` | Add `nfl.aggregate_player_season()`, `nfl.aggregate_team_season()` |
| `sql/football.sql` | Add `football.aggregate_player_season()`, `football.aggregate_team_season()` |
| `seed/scoracle_seed/models.py` | Add `EventBoxScore`, `EventTeamStats` (or new file) |
| `seed/scoracle_seed/upsert.py` | Add event upsert functions (or new file) |
| `seed/scoracle_seed/seed_nba.py` | Replace full-season fetch with per-fixture box score fetch |
| `seed/scoracle_seed/seed_nfl.py` | Same |
| `seed/scoracle_seed/seed_football.py` | Same |
| `seed/scoracle_seed/cli.py` | Implement `load-fixtures`, add `backfill` command |
| `seed/scoracle_seed/fixtures.py` | Minor updates to support new flow |
| `seed/scoracle_seed/bdl_nba.py` | Add `get_box_score(game_id)` method |
| `seed/scoracle_seed/bdl_nfl.py` | Add `get_box_score(game_id)` method |
| `seed/scoracle_seed/sportmonks_football.py` | Add `get_fixture_stats(fixture_id)` method |

### Unchanged Files
| File | Why Unchanged |
|------|--------------|
| `go/internal/db/db.go` | Reads from views; views change source but keep shape |
| `go/internal/api/handler/data.go` | JSON passthrough, unaffected |
| `go/internal/api/server.go` | Routes unchanged |
| All Go files | API layer is decoupled from ingestion |

---

## Implementation Order

1. **Phase 1** — Schema: Create event tables, indexes, triggers (~1 session)
2. **Phase 4** — Fixture loading: Implement `load-fixtures` for all providers (~1-2 sessions)
3. **Phase 3a-c** — Seeder: Box score handlers + new upsert functions (~2 sessions)
4. **Phase 2** — Aggregation: SQL functions to compute season stats from box scores (~1-2 sessions)
5. **Phase 3d** — Wire up: Connect new seeder flow to fixture processing (~1 session)
6. **Phase 5** — Migration: Parallel run, validation, backfill, cutover (~2-3 sessions)

**Total estimated scope: 8-11 focused sessions**

---

## Design Principles (Carried Forward)

1. **Postgres-as-serializer** — aggregation happens in SQL, not Python or Go
2. **No derived stats in Go/Python** — derived stats computed by triggers on `player_stats` (unchanged)
3. **Seeder is ingestion-only** — fetch box score, normalize minimally, upsert raw
4. **`finalize_fixture()` remains the single handoff point** — seeder calls it, Postgres does the rest
5. **Views maintain backward compatibility** — Go API reads same views, gets same JSON shape

---

## Implementation Snapshot (2026-03-28 update)

The following components are now implemented:

- Shared SQL additions in `sql/shared.sql`:
  - `provider_entity_map` and `provider_fixture_map`
  - `event_box_scores` and `event_team_stats`
  - normalization triggers on event tables
  - `resolve_provider_fixture_id()`
  - `box_score_coverage_report()` for completeness checks
  - `fixtures` uniqueness moved to `(sport, external_id)` for provider safety
  - `finalize_fixture()` now reaggregates impacted player/team rows from event tables before percentile refresh

- Sport SQL aggregation functions:
  - `nba.aggregate_player_season`, `nba.aggregate_team_season`
  - `nfl.aggregate_player_season`, `nfl.aggregate_team_season`
  - `football.aggregate_player_season`, `football.aggregate_team_season`

- Seeder flow in `seed/scoracle_seed/`:
  - `load-fixtures` implemented for NBA/NFL/Football
  - `process` and `seed-fixture` now seed per-fixture event box scores
  - `backfill` command added
  - provider handlers now include fixture schedule + fixture box score methods
  - event models and upsert functions added
  - provider mapping upserts added for teams, players, fixtures
  - `percentiles` now reports event box score coverage + missing required keys

Remaining validation work:

- Validate provider endpoint assumptions in live environments (NBA/NFL BDL variants + SportMonks fixture includes)
- Run parity checks against prior season aggregates for target sports/leagues
- Harden any provider-specific key remapping discovered during live pulls
