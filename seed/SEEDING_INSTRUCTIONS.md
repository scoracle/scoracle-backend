# Seeding Instructions

This guide explains how to seed the database with sports data from various providers.

## Prerequisites

1. Set up your environment variables in `.env.local`:
   - `DATABASE_URL` - Your PostgreSQL connection string
   - `BALLDONTLIE_API_KEY` - For NBA and NFL data (GOAT tier recommended)
   - `SPORTMONKS_API_TOKEN` - For Football/Soccer data

2. Install the seeder package:
   ```bash
   cd seed
   pip install -e .
   ```

## Quick Start

### 1. Load Fixtures

Load the season schedule (teams are now loaded automatically with fixtures):

**NBA (2025-26 season):**
```bash
scoracle-seed load-fixtures nba --season 2025
```

**NFL (2025-26 season):**
```bash
scoracle-seed load-fixtures nfl --season 2025
```

**Football (2025-26 season - all 5 major leagues):**
```bash
# Premier League (England)
scoracle-seed load-fixtures football --season 2026 --league 8

# Bundesliga (Germany)
scoracle-seed load-fixtures football --season 2026 --league 82

# La Liga (Spain)
scoracle-seed load-fixtures football --season 2026 --league 564

# Serie A (Italy)
scoracle-seed load-fixtures football --season 2026 --league 384

# Ligue 1 (France)
scoracle-seed load-fixtures football --season 2026 --league 301
```

### 2. Process Fixtures (Fetch Box Scores)

This fetches player and team statistics from completed games:

**Process with season filter (recommended):**
```bash
# Process NBA 2025 season fixtures
scoracle-seed process --sport NBA --season 2025 --max 50

# Process NFL 2025 season fixtures
scoracle-seed process --sport NFL --season 2025 --max 50

# Process Football 2025/26 season fixtures
scoracle-seed process --sport FOOTBALL --season 2026 --max 50
```

**Process without season filter (processes all pending):**
```bash
scoracle-seed process --sport NBA --max 100
```

**Process all sports:**
```bash
scoracle-seed process --max 100
```

### 3. Check Database State

View current seeding progress:

```bash
# Check fixture counts by sport and status
psql $DATABASE_URL -c "
SELECT sport, season, status, COUNT(*) 
FROM fixtures 
GROUP BY sport, season, status 
ORDER BY sport, season, status;
"
```

## Season Filtering

The `--season` flag allows you to process specific seasons only:

```bash
# Process only 2025 season NBA games
scoracle-seed process --sport NBA --season 2025 --max 50

# Process only 2026 season Football games
scoracle-seed process --sport FOOTBALL --season 2026 --max 50
```

**Why use season filtering?**
- Keeps data organized by season
- Prevents cross-contamination between seasons
- Allows incremental seeding as seasons progress

## Current Season Data

### NBA
- **Current Season:** 2025 (2025-26 season)
- **Dates:** October 2025 - April 2026
- **Fixtures:** ~1,200 games

### NFL
- **Current Season:** 2025 (2025-26 season)
- **Dates:** September 2025 - February 2026
- **Fixtures:** 285 games (regular season)

### Football (5 Major Leagues)
- **Current Season:** 2025/2026
- **Dates:** August 2025 - May 2026
- **Fixtures:** ~1,750 games across 5 leagues

**League IDs:**
| League | ID | 2025/26 Season ID |
|--------|-----|------------------|
| Premier League | 8 | 25583 |
| Bundesliga | 82 | 25646 |
| La Liga | 564 | 25659 |
| Serie A | 384 | 25533 |
| Ligue 1 | 301 | 25651 |

## Historical Season Seeding

To seed data from past seasons:

### NBA Historical

```bash
# Load 2024 season fixtures
scoracle-seed load-fixtures nba --season 2024

# Process 2024 season games
scoracle-seed process --sport NBA --season 2024 --max 100
```

### NFL Historical

```bash
# Load 2024 season fixtures
scoracle-seed load-fixtures nfl --season 2024

# Process 2024 season games
scoracle-seed process --sport NFL --season 2024 --max 100
```

### Football Historical

```bash
# First ensure provider_seasons table has the mapping
# Then load fixtures for specific season
scoracle-seed load-fixtures football --season 2025 --league 8  # 2024/25 season

# Process
scoracle-seed process --sport FOOTBALL --season 2025 --max 100
```

## Backfill Command (Historical in One Step)

The `backfill` command combines loading fixtures and processing them:

```bash
# Backfill NBA with date range
scoracle-seed backfill nba --season 2024 --from-date 2023-10-01 --to-date 2024-06-30

# Backfill with limits
scoracle-seed backfill nba --season 2024 --max 200
```

## Advanced Commands

### Seed a Single Fixture

```bash
scoracle-seed seed-fixture --id 123
```

### Recalculate Percentiles

After seeding box scores, recalculate percentiles for derived stats:

```bash
# NBA
scoracle-seed percentiles --sport NBA --season 2025

# NFL
scoracle-seed percentiles --sport NFL --season 2025

# Football (requires league for percentiles)
scoracle-seed percentiles --sport FOOTBALL --season 2026 --league 8
```

## Data Flow

The seeding process follows this pipeline:

```
1. load-fixtures → fixtures table (includes teams via participants)
2. process --season N → event_box_scores + event_team_stats tables
3. Postgres triggers → player_stats + team_stats (aggregated)
4. percentiles → Percentile calculations + derived stats (PER, QBR, xG, etc.)
5. API Views → nba.player, nba.team, etc. (served to frontend)
```

## GOAT Tier Features

With BallDontlie GOAT tier, the seeding process captures:

### NBA
- ✓ Basic box scores (points, rebounds, assists, etc.)
- ✓ Advanced stats V2 (PIE, pace, usage%, hustle stats, tracking data)
- ✓ Lineups (starters, bench, positions)
- ✓ Team box scores (quarter scores, timeouts, bonus status)

### NFL
- ✓ Player stats (passing, rushing, receiving, defense)
- ✓ Team aggregates

### Football
- ✓ Lineups with detailed player stats
- ✓ Match events (goals, cards, substitutions)
- ✓ Player match statistics

## Rate Limits

- **Balldontlie:** 600 requests/minute (GOAT tier)
- **Sportmonks:** 300 requests/minute (on current plan)

The seeder automatically respects these limits with built-in throttling.

## Provider Season IDs (Football)

Season IDs change annually. Current mappings:

**2025/2026 Season:**
| League | League ID | Sportmonks Season ID |
|--------|-----------|---------------------|
| Premier League | 8 | 25583 |
| Bundesliga | 82 | 25646 |
| Ligue 1 | 301 | 25651 |
| Serie A | 384 | 25533 |
| La Liga | 564 | 25659 |

**2024/2025 Season:**
| League | League ID | Sportmonks Season ID |
|--------|-----------|---------------------|
| Premier League | 8 | 23614 |
| Bundesliga | 82 | 23744 |
| Ligue 1 | 301 | 23643 |
| Serie A | 384 | 23746 |
| La Liga | 564 | 23621 |

These are stored in the `provider_seasons` table and mapped automatically during fixture loading.

## Tips

1. **Always use `--season` filter:** Keeps data clean and prevents processing wrong seasons
2. **Process regularly:** Set up a cron job to run every 30 minutes during the season
3. **Start with small batches:** Use `--max 10` for initial testing
4. **Monitor coverage:** Use the `percentiles` command to see box score coverage reports
5. **Incremental seeding:** The process command is idempotent - you can run it repeatedly safely

## Troubleshooting

**No fixtures found:**
- Check that provider_seasons table has correct mappings
- Verify season year format (NBA/NFL use 2025, Football uses 2026 for 2025/26 season)

**Empty box scores:**
- Games may not have started or completed yet
- Check game status in the fixtures table

**API errors (400/401):**
- Verify API keys are valid in `.env.local`
- Check rate limits haven't been exceeded
- Ensure you're using GOAT tier for advanced features

**"No pending fixtures" message:**
- All fixtures may already be processed (status = 'seeded')
- Check with: `SELECT sport, season, status, COUNT(*) FROM fixtures GROUP BY sport, season, status;`

**Season filter returns no results:**
- Verify the season exists in fixtures table
- Check season year format (Football uses 2026 for 2025/26 season)

## Docker

To run via Docker Compose:

```bash
# Full stack
docker compose up --build

# Run seeder commands
docker compose run --rm seed process --sport NBA --season 2025 --max 50
docker compose run --rm seed process --sport NFL --season 2025 --max 50
docker compose run --rm seed process --sport FOOTBALL --season 2026 --max 50
```

---

*Last updated: 2026-04-04*
