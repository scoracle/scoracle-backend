# Box Score Endpoints Documentation

This document tracks the current box score and statistics endpoints used by each sport provider. This helps with provider switching and debugging when endpoints change.

## NBA (Balldontlie API)

**Base URL:** `https://api.balldontlie.io`

### Teams
- **Endpoint:** `GET /v1/teams`
- **Returns:** List of all NBA teams
- **Status:** ✓ Working

### Fixtures/Games
- **Endpoint:** `GET /v1/games`
- **Query Parameters:**
  - `seasons[]`: Filter by season year (e.g., `2025`)
  - `start_date`: Start date filter (YYYY-MM-DD)
  - `end_date`: End date filter (YYYY-MM-DD)
- **Returns:** Game schedule with home/away teams and scores
- **Status:** ✓ Working

### Player Box Scores (Per Game)
- **Endpoint:** `GET /v1/stats`
- **Query Parameters:**
  - `game_ids[]`: Array of game IDs to fetch stats for (e.g., `game_ids[]=15907439`)
- **Returns:** Player statistics for specified games including:
  - Player info (name, position, team)
  - Game context (date, opponent)
  - Stats: pts, reb, ast, stl, blk, fg%, 3p%, ft%, etc.
- **Note:** Returns empty `stats` object if data not available
- **Status:** ✓ Working

### Team Stats
- **Endpoint:** Same as player stats (`/v1/stats`)
- **Note:** Player stats include team context; team aggregates calculated in DB
- **Status:** ✓ Working

---

## NFL (Balldontlie API)

**Base URL:** `https://api.balldontlie.io/nfl`

### Teams
- **Endpoint:** `GET /nfl/v1/teams`
- **Returns:** List of all NFL teams
- **Status:** ✓ Working

### Fixtures/Games
- **Endpoint:** `GET /nfl/v1/games`
- **Query Parameters:**
  - `seasons[]`: Filter by season year (e.g., `2024`)
  - `weeks[]`: Filter by week number (e.g., `1`)
- **Returns:** Game schedule with home/away teams, scores, quarter scores
- **Status:** ✓ Working

### Player Box Scores (Per Game)
- **Endpoint:** `GET /nfl/v1/stats`
- **Query Parameters:**
  - `game_ids[]`: Array of game IDs (e.g., `game_ids[]=7001`)
- **Returns:** Player statistics for specified games including:
  - Player info (name, position, team)
  - Passing: completions, attempts, yards, touchdowns, interceptions
  - Rushing: attempts, yards, touchdowns, long
  - Receiving: receptions, yards, touchdowns, targets
  - Defense: tackles, sacks, interceptions, forced fumbles
  - Special Teams: kick/punt returns, field goals
- **Status:** ✓ Working
- **Note:** Previously tried `/box_scores` and `/season_stats` (not available per-game)

### Team Stats
- **Endpoint:** Same as player stats (`/nfl/v1/stats`)
- **Note:** Player stats include team context; team aggregates calculated in DB
- **Status:** ✓ Working

### Season Stats (Aggregate)
- **Endpoint:** `GET /nfl/v1/season_stats`
- **Query Parameters:**
  - `season`: Season year (required)
  - `team_id`: Filter by team
  - `postseason`: Boolean for playoff stats
- **Note:** Returns season-long totals, not per-game box scores
- **Status:** Available but not used for fixture seeding

---

## Football/Soccer (Sportmonks API)

**Base URL:** `https://api.sportmonks.com/v3/football`

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
- **Note:** Need correct season ID for fixtures (e.g., Premier League 2024/25 = 23614)

### Fixtures
- **Endpoint:** `GET /fixtures`
- **Query Parameters:**
  - `filters=fixtureSeasons:{season_id}`: Filter by season (e.g., `fixtureSeasons:23614`)
  - `include=participants`: Include team details
  - `per_page`: Results per page (max 50)
- **Returns:** Match schedule with teams, dates, scores
- **Status:** ✓ Working

### Fixture Details (Box Scores)
- **Endpoint:** `GET /fixtures/{fixture_id}`
- **Query Parameters:**
  - `include=lineups.details.type;events;scores;participants`: Include detailed match data
- **Returns:** Complete fixture data including:
  - Lineups (players, positions, stats)
  - Events (goals, cards, substitutions)
  - Scores (half-time, full-time)
  - Participants (team details)
- **Status:** ✓ Working
- **Note:** This is the primary endpoint for box scores - returns full match data in one call

### Teams
- **Endpoint:** Loaded via fixture participants
- **Note:** Teams upserted when processing fixtures
- **Status:** ✓ Working

---

## Provider Season Mappings

The database stores mappings between league/season combinations and provider-specific season IDs:

```sql
-- Premier League 2024/25
INSERT INTO provider_seasons (league_id, season_year, provider_season_id, provider) 
VALUES (8, 2025, 23614, 'sportmonks');
```

### Current Mappings (Football)
- League 8 (Premier League), 2025 → Provider ID 23614
- League 82 (Bundesliga), 2025 → Provider ID 23445
- League 301 (Ligue 1), 2025 → Provider ID 23444
- League 384 (Serie A), 2025 → Provider ID 23447
- League 564 (La Liga), 2025 → Provider ID 23448

---

## Authentication

### Balldontlie
- **Header:** `Authorization: {api_key}`
- **Key Location:** `.env` → `BALLDONTLIE_API_KEY`

### Sportmonks
- **Query Parameter:** `api_token={token}`
- **Key Location:** `.env` → `SPORTMONKS_API_TOKEN`

---

## Known Issues & Workarounds

### NFL Stats Endpoint
- **Issue:** `/box_scores` endpoint returns 404
- **Solution:** Use `/stats` endpoint with `game_ids[]` parameter
- **Impact:** Same data, different endpoint path

### Football Season IDs
- **Issue:** Season IDs change annually and vary by provider
- **Solution:** Query `/seasons` endpoint or `/leagues/{id}?include=seasons` to get current IDs
- **Impact:** Must update `provider_seasons` table when seasons change

### Rate Limits
- **Balldontlie:** 600 requests/minute
- **Sportmonks:** Check plan limits (varies by tier)

---

## Future Provider Considerations

When switching providers, ensure endpoints support:
1. **Team listings** - Basic team info (name, city, conference/division)
2. **Fixture schedules** - Game listings with dates, teams, and IDs
3. **Per-game box scores** - Player and team stats per fixture
4. **Pagination** - Cursor or offset-based for large result sets

Common alternative providers:
- **NBA:** ESPN API, NBA Stats API, Basketball-Reference
- **NFL:** ESPN API, NFL Game Center API
- **Football:** API-Football, Football-Data.org, OddsAPI

---

*Last updated: 2026-03-29*
