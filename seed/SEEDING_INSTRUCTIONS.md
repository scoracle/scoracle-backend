# Seeding Instructions

This guide explains how to seed the database with sports data using the two-service architecture.

## Architecture Overview

The seeding system has two separate services:

1. **Event Service** (`scoracle-seed event`) - Box scores, fixtures, raw game data
2. **Meta Service** (`scoracle-seed meta`) - Player/team profiles, photos, bio data

This separation keeps event data (critical) provider-agnostic while allowing flexible metadata sourcing.

## Prerequisites

1. Set up environment variables in `.env.local`:
   - `DATABASE_URL` - PostgreSQL connection string
   - `BALLDONTLIE_API_KEY` - For NBA and NFL data
   - `SPORTMONKS_API_TOKEN` - For Football/Soccer data

2. Install the seeder package:
   ```bash
   cd seed
   pip install -e .
   ```

## Event Service (Box Scores)

### Load Fixtures

Load the season schedule:

**NBA:**
```bash
scoracle-seed event load-fixtures nba --season 2025
```

**NFL:**
```bash
scoracle-seed event load-fixtures nfl --season 2025
```

**Football:**
```bash
# Premier League
scoracle-seed event load-fixtures football --season 2026 --league 8

# Bundesliga
scoracle-seed event load-fixtures football --season 2026 --league 82

# La Liga
scoracle-seed event load-fixtures football --season 2026 --league 564

# Serie A
scoracle-seed event load-fixtures football --season 2026 --league 384

# Ligue 1
scoracle-seed event load-fixtures football --season 2026 --league 301
```

### Process Fixtures (Box Scores)

Fetch player and team statistics from completed games:

```bash
# Process NBA
scoracle-seed event process --sport NBA --season 2025 --max 50

# Process NFL
scoracle-seed event process --sport NFL --season 2025 --max 50

# Process Football
scoracle-seed event process --sport FOOTBALL --season 2026 --max 50
```

## Meta Service (Profiles)

### Seed Metadata

Populate player/team profiles at season start:

```bash
# Seed NBA player profiles
scoracle-seed meta seed --sport nba --season 2025

# Seed NFL player profiles
scoracle-seed meta seed --sport nfl --season 2025

# Seed Football player profiles
scoracle-seed meta seed --sport football --season 2026
```

### Refresh Single Player

Update metadata for specific player:

```bash
scoracle-seed meta refresh --sport nba --player-id 237
```

## Current Season Data

### NBA
- **Season:** 2025 (2025-26)
- **Dates:** October 2025 - April 2026
- **Fixtures:** ~1,200 games

### NFL
- **Season:** 2025 (2025-26)
- **Dates:** September 2025 - February 2026
- **Fixtures:** 285 games

### Football
- **Season:** 2025/2026
- **Dates:** August 2025 - May 2026
- **Leagues:** 5 major leagues

**League IDs:**
| League | ID | Season ID |
|--------|-----|-----------|
| Premier League | 8 | 25583 |
| Bundesliga | 82 | 25646 |
| Ligue 1 | 301 | 25651 |
| Serie A | 384 | 25533 |
| La Liga | 564 | 25659 |

## Service Separation

### Event Service
- **Data:** Box scores, fixtures, game events
- **Priority:** Critical
- **Runs:** On-demand (CLI)
- **Goal:** Provider-agnostic raw event data

### Meta Service
- **Data:** Photos, bio, jersey numbers, positions
- **Priority:** Important for UX
- **Runs:** Background daemon (LISTEN/NOTIFY)
- **Goal:** Rich profiles from standard endpoints

## Architecture

```
Event Seeding -> API (box scores) -> DB (event_box_scores)
                                              |
                                       Postgres Trigger
                                              |
                                       NOTIFY 'team_change'
                                              |
                                       Meta Service (LISTEN)
                                              |
                                       API (profile) -> DB (players)
```

## Authentication

### BallDontlie (NBA/NFL)
- **Header:** `Authorization: {api_key}`
- **Rate Limit:** 600 requests/minute (GOAT tier)

### SportMonks (Football)
- **Query Parameter:** `api_token={token}`
- **Rate Limit:** 3,000 requests/day

## Docker

```bash
# Run event seeding
docker compose run --rm seed event load-fixtures nba --season 2025

# Run meta service (continuous)
docker compose up meta-service
```

## See Also

- `BOX_SCORE_ENDPOINTS.md` - Event/box score API endpoints
- `META_ENDPOINTS.md` - Profile/metadata API endpoints

---

*Last updated: 2026-04-06*
