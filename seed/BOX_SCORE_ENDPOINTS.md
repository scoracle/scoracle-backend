# Box Score Endpoints Documentation

This document tracks the current box score and statistics endpoints used by each sport provider. This helps with provider switching and debugging when endpoints change.

## NBA (Balldontlie API) - GOAT Tier

**Base URL:** `https://api.balldontlie.io`

**Authentication:** Header `Authorization: {api_key}`

### Teams
- **Endpoint:** `GET /nba/v1/teams`
- **Returns:** List of all NBA teams (IDs 1-30 only - current teams only)
- **Status:** ✓ Working
- **Note:** Non-current teams (historical BAA/NFL teams) are filtered out in code

### Fixtures/Games
- **Endpoint:** `GET /nba/v1/games`
- **Query Parameters:**
  - `seasons[]`: Filter by season year (e.g., `2025`)
  - `start_date`: Start date filter (YYYY-MM-DD)
  - `end_date`: End date filter (YYYY-MM-DD)
- **Returns:** Game schedule with home/away teams and scores
- **Status:** ✓ Working

### Player Stats (Per Game)
- **Endpoint:** `GET /nba/v1/stats`
- **Query Parameters:**
  - `game_ids[]`: Array of game IDs (e.g., `game_ids[]=18446819`)
- **Returns:** Player box score statistics including:
  - Player info (name, position, team)
  - Basic stats: pts, reb, ast, stl, blk, fg%, 3p%, ft%
  - Plus/minus
- **Status:** ✓ Working
- **Note:** Primary endpoint for player data - no GOAT tier required

### Advanced Stats V2 (GOAT Tier Only)
- **Endpoint:** `GET /nba/v2/stats/advanced`
- **Query Parameters:**
  - `game_ids[]`: Array of game IDs
- **Returns:** Advanced metrics per player including:
  - PIE (Player Impact Estimate)
  - Pace, usage percentage
  - Hustle stats: box outs, deflections, contested shots
  - Tracking data: speed, distance, touches
  - Defensive metrics: matchup stats, switches
- **Status:** ✓ Working with GOAT tier
- **Note:** Used for rich metadata during seeding

### Lineups (GOAT Tier Only)
- **Endpoint:** `GET /nba/v1/lineups`
- **Query Parameters:**
  - `game_ids[]`: Array of game IDs
- **Returns:** Lineup information including:
  - Starters vs bench players
  - Positions
  - Player info
- **Status:** ✓ Working with GOAT tier
- **Note:** Captured during seeding process

### Team Box Scores (GOAT Tier Only)
- **Endpoint:** `GET /nba/v1/box_scores`
- **Query Parameters:**
  - `date`: Single date (YYYY-MM-DD) - **REQUIRED** format
- **Returns:** Team-level box scores including:
  - Quarter-by-quarter scores
  - Overtime scores
  - Timeouts remaining
  - Bonus status
- **Status:** ✓ Working
- **Note:** Cannot use `game_ids[]` parameter - requires date parameter

### Play-by-Play (GOAT Tier Only)
- **Endpoint:** `GET /nba/v1/plays`
- **Query Parameters:**
  - `game_ids[]`: Array of game IDs
- **Returns:** Every play from the game:
  - Play type and description
  - Scoring plays
  - Shot coordinates (when available)
  - Period and clock
- **Status:** ⚠️ Parameter issues - needs game_id format investigation

### Team Standings
- **Endpoint:** `GET /nba/v1/standings`
- **Query Parameters:**
  - `season`: Season year (required)
- **Returns:** Conference and division rankings
- **Status:** ✓ Working

---

## NFL (Balldontlie API) - GOAT Tier

**Base URL:** `https://api.balldontlie.io`

**Authentication:** Header `Authorization: {api_key}`

### Teams
- **Endpoint:** `GET /nfl/v1/teams`
- **Returns:** List of all NFL teams
- **Status:** ✓ Working

### Fixtures/Games
- **Endpoint:** `GET /nfl/v1/games`
- **Query Parameters:**
  - `seasons[]`: Filter by season year (e.g., `2025`)
  - `weeks[]`: Filter by week number (e.g., `1`)
- **Returns:** Game schedule with home/away teams, scores, quarter scores
- **Status:** ✓ Working

### Player Stats (Per Game)
- **Endpoint:** `GET /nfl/v1/stats`
- **Query Parameters:**
  - `game_ids[]`: Array of game IDs (e.g., `game_ids[]=7001`)
- **Returns:** Player statistics including:
  - Player info (name, position, team)
  - Passing: completions, attempts, yards, touchdowns, interceptions
  - Rushing: attempts, yards, touchdowns, long
  - Receiving: receptions, yards, touchdowns, targets
  - Defense: tackles, sacks, interceptions, forced fumbles
  - Special Teams: kick/punt returns, field goals
- **Status:** ✓ Working

### Team Stats
- **Endpoint:** Same as player stats (`/nfl/v1/stats`)
- **Note:** Player stats include team context; team aggregates calculated in DB
- **Status:** ✓ Working

### Play-by-Play (GOAT Tier Only)
- **Endpoint:** `GET /nfl/v1/plays`
- **Query Parameters:**
  - `game_ids[]`: Array of game IDs
- **Returns:** Every play with descriptions and yardage
- **Status:** ⚠️ Needs parameter format verification

---

## Football/Soccer (Sportmonks API)

**Base URL:** `https://api.sportmonks.com/v3/football`

**Authentication:** Query parameter `api_token={token}`

### Leagues
- **Endpoint:** `GET /leagues/{id}`
- **Query Parameters:**
  - `include=seasons`: Include available seasons
- **Returns:** League information with season mappings
- **Status:** ✓ Working

### Seasons
- **Endpoint:** `GET /seasons`
- **Returns:** All available seasons with IDs
- **Status:** ✓ Working

### Fixtures
- **Endpoint:** `GET /fixtures`
- **Query Parameters:**
  - `filters=fixtureSeasons:{season_id}`: Filter by season
  - `include=participants`: Include team details
  - `per_page`: Results per page (max 50)
- **Returns:** Match schedule with teams, dates, scores
- **Status:** ✓ Working

### Fixture Details (Box Scores)
- **Endpoint:** `GET /fixtures/{fixture_id}`
- **Query Parameters:**
  - `include=lineups.details.type;events;scores;participants`
- **Returns:** Complete fixture data including:
  - Lineups (players, positions, stats)
  - Events (goals, cards, substitutions)
  - Scores (half-time, full-time)
  - Participants (team details)
- **Status:** ✓ Working
- **Note:** This is the primary endpoint for box scores

---

## Provider Season Mappings (Current)

The database stores mappings between league/season combinations and provider-specific season IDs:

### NBA/NFL
Season year matches API directly (e.g., `2025` for 2025-26 season).

### Football (Sportmonks)

**2025/2026 Season (Current):**
| League | League ID | Season ID |
|--------|-----------|-----------|
| Premier League | 8 | 25583 |
| Bundesliga | 82 | 25646 |
| Ligue 1 | 301 | 25651 |
| Serie A | 384 | 25533 |
| La Liga | 564 | 25659 |

**2024/2025 Season (Previous):**
| League | League ID | Season ID |
|--------|-----------|-----------|
| Premier League | 8 | 23614 |
| Bundesliga | 82 | 23744 |
| Ligue 1 | 301 | 23643 |
| Serie A | 384 | 23746 |
| La Liga | 564 | 23621 |

**SQL to add new season:**
```sql
INSERT INTO provider_seasons (league_id, season_year, provider_season_id, provider) 
VALUES (8, 2026, 25583, 'sportmonks');
```

---

## Authentication

### Balldontlie (NBA/NFL)
- **Header:** `Authorization: {api_key}`
- **Key Location:** `.env` → `BALLDONTLIE_API_KEY`
- **Tiers:** Free (5 req/min), All-Star (60 req/min), GOAT (600 req/min)
- **Required for:** Advanced stats, lineups, box scores, play-by-play

### Sportmonks (Football)
- **Query Parameter:** `api_token={token}`
- **Key Location:** `.env` → `SPORTMONKS_API_TOKEN`

---

## Known Issues & Workarounds

### NBA Box Scores Parameter Format
- **Issue:** `/nba/v1/box_scores` requires `date` parameter, not `game_ids[]`
- **Solution:** Use game date from fixture record
- **Impact:** Requires date lookup before fetching

### NBA/NFL Play-by-Play
- **Issue:** Some endpoints return 400 with certain parameter formats
- **Status:** Under investigation - may require specific game_id formats
- **Workaround:** Basic stats endpoint provides core data

### Football Season IDs
- **Issue:** Season IDs change annually and vary by provider
- **Solution:** Query `/leagues/{id}?include=seasons` to get current IDs
- **Impact:** Must update `provider_seasons` table when seasons change

### Rate Limits
- **Balldontlie:** 600 requests/minute (GOAT tier)
- **Sportmonks:** Check plan limits (varies by tier)

---

## Code Implementation Notes

### Endpoint Paths in Code
All endpoints use full paths including sport prefix:
- `/nba/v1/teams` (not `/v1/teams`)
- `/nfl/v1/games` (not `/v1/games`)

### Team ID Filtering (NBA)
Only current NBA teams (IDs 1-30) are accepted. Historical BAA/NFL teams are filtered out in:
- `bdl_nba.py:get_teams()`
- `bdl_nba.py:get_games()`

### Rich Metadata Capture
When processing fixtures with GOAT tier:
1. Basic stats from `/stats` endpoint (all tiers)
2. Advanced stats from `/v2/stats/advanced` (GOAT only)
3. Lineups from `/lineups` endpoint (GOAT only)
4. Team box scores from `/box_scores` by date (GOAT only)

---

*Last updated: 2026-04-04*
