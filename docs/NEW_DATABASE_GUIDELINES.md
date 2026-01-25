# New Database Guidelines: Preventing Cross-Sport Data Contamination

## Overview

This document provides guidelines for the new database to prevent cross-sport data contamination that occurred with the unified views approach.

## Problem Summary

The original database architecture created unified views (`players`, `teams`) that UNION ALL sport-specific tables. When queried without a `sport_id` filter, all matching IDs across sports were returned, causing data contamination:

- NBA player ID 2801 returning data mixed with FOOTBALL player attributes
- NFL team ID 29 returning data mixed with NBA team attributes

## Root Cause

Migration `016_unified_views.sql` created problematic views:

```sql
CREATE VIEW players AS
  SELECT *, 'NBA' as sport_id FROM nba_player_profiles
  UNION ALL
  SELECT *, 'NFL' as sport_id FROM nfl_player_profiles
  UNION ALL
  SELECT *, 'FOOTBALL' as sport_id FROM football_player_profiles;
```

## New Database Requirements

### 1. NO Unified Views

**DO NOT** create `players` or `teams` views that UNION across sports. Each sport must have isolated tables.

```sql
-- WRONG: Creates cross-sport contamination risk
CREATE VIEW players AS
  SELECT *, 'NBA' as sport_id FROM nba_player_profiles
  UNION ALL ...;

-- RIGHT: Sport-specific tables only
-- Query nba_player_profiles directly for NBA players
-- Query nfl_player_profiles directly for NFL players
```

### 2. Mandatory Sport Context via Table Selection

Every entity query must specify sport context through table selection, not WHERE clauses.

```python
# WRONG: Relies on filter that can be forgotten
db.fetchone("SELECT * FROM players WHERE id = %s AND sport_id = %s", (player_id, sport_id))

# RIGHT: Table name enforces sport context
PLAYER_PROFILE_TABLES = {
    "NBA": "nba_player_profiles",
    "NFL": "nfl_player_profiles",
    "FOOTBALL": "football_player_profiles",
}
table = PLAYER_PROFILE_TABLES[sport_id]
db.fetchone(f"SELECT * FROM {table} WHERE id = %s", (player_id,))
```

### 3. Standard Table Mapping Constants

All modules should import and use consistent table mappings:

```python
# In a shared module like scoracle_data/tables.py
PLAYER_PROFILE_TABLES = {
    "NBA": "nba_player_profiles",
    "NFL": "nfl_player_profiles",
    "FOOTBALL": "football_player_profiles",
}

TEAM_PROFILE_TABLES = {
    "NBA": "nba_team_profiles",
    "NFL": "nfl_team_profiles",
    "FOOTBALL": "football_team_profiles",
}

PLAYER_STATS_TABLES = {
    "NBA": "nba_player_stats",
    "NFL": "nfl_player_stats",  # or specific tables for passing/rushing/etc
    "FOOTBALL": "football_player_stats",
}

TEAM_STATS_TABLES = {
    "NBA": "nba_team_stats",
    "NFL": "nfl_team_stats",
    "FOOTBALL": "football_team_stats",
}
```

### 4. Composite Keys for Shared Tables

Tables that store data across sports (like caches) must use composite keys:

```sql
-- percentile_cache uses (entity_id, sport_id) as logical key
CREATE TABLE percentile_cache (
    id SERIAL PRIMARY KEY,
    entity_type VARCHAR(10) NOT NULL,  -- 'player' or 'team'
    entity_id INTEGER NOT NULL,
    sport_id VARCHAR(10) NOT NULL,     -- 'NBA', 'NFL', 'FOOTBALL'
    season_id INTEGER NOT NULL,
    stat_category VARCHAR(50) NOT NULL,
    -- ... other columns ...
    UNIQUE(entity_type, entity_id, sport_id, season_id, stat_category)
);
```

### 5. Foreign Key Constraints Within Sport

Foreign keys should only reference tables within the same sport:

```sql
-- nba_player_profiles.current_team_id ONLY references nba_team_profiles
ALTER TABLE nba_player_profiles
ADD CONSTRAINT fk_nba_player_team
FOREIGN KEY (current_team_id) REFERENCES nba_team_profiles(id);

-- DO NOT create cross-sport foreign keys
```

### 6. Function Signatures Must Include sport_id

Functions that look up entities must require sport_id:

```python
# WRONG: Missing sport context
def get_entity_name(entity_type: str, entity_id: int) -> str:
    ...

# RIGHT: Sport context required
def get_entity_name(entity_type: str, entity_id: int, sport_id: str) -> str:
    ...
```

## CI Validation

Add a grep-based check to CI to catch dangerous patterns:

```bash
#!/bin/bash
# scripts/check_unified_views.sh

# Check for queries on unified views without sport_id filter
patterns=(
    "FROM players WHERE id = "
    "FROM teams WHERE id = "
    "FROM players WHERE.*id.*=.*[^AND]*$"
    "FROM teams WHERE.*id.*=.*[^AND]*$"
)

found=0
for pattern in "${patterns[@]}"; do
    if grep -rn "$pattern" src/ --include="*.py" | grep -v "sport_id"; then
        echo "WARNING: Found unified view query without sport_id filter"
        found=1
    fi
done

exit $found
```

## Migration Checklist

When setting up the new database:

- [ ] Do NOT include `016_unified_views.sql` migration
- [ ] Create sport-specific tables only
- [ ] Add foreign key constraints within each sport
- [ ] Update all code to use table mapping constants
- [ ] Add CI check for dangerous query patterns
- [ ] Run integration tests with IDs that exist in multiple sports

## Files Updated in Fix

The following files were updated to use sport-specific tables:

| File | Changes |
|------|---------|
| `ml/jobs/vibe_calculator.py` | Added sport_id to `_get_entity_name()`, uses sport-specific tables |
| `percentiles/calculator.py` | Uses sport-specific profile tables in batch recalculation and distribution queries |
| `roster_diff/engine.py` | Uses sport-specific tables in `_get_db_players()` and `_get_db_teams()` |
| `cli.py` | Uses sport-specific tables in `cmd_status()` and `cmd_export()` |

## Testing

### Unit Test Example

```python
def test_no_cross_sport_contamination():
    """Verify that querying NBA player doesn't return FOOTBALL data."""
    # Setup: Both NBA and FOOTBALL have player ID 2801
    nba_player = db.get_player(2801, "NBA")
    football_player = db.get_player(2801, "FOOTBALL")

    # NBA player should have basketball position
    assert nba_player["position"] in ["PG", "SG", "SF", "PF", "C"]

    # FOOTBALL player should have soccer position
    assert football_player["position"] in ["Goalkeeper", "Defender", "Midfielder", "Attacker"]
```

### Integration Test

1. Query NBA player 2801 via API
2. Verify response contains NBA-specific fields (ppg, rpg, apg)
3. Verify response does NOT contain FOOTBALL fields (goals, assists, appearances)

## Summary

The key principle is: **Sport context must be enforced through table selection, not WHERE filters.** This makes it impossible to accidentally query the wrong sport's data because the table name itself encodes the sport.
