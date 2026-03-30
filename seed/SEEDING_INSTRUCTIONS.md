# Seeding Instructions

This guide explains how to seed the database with sports data from various providers.

## Prerequisites

1. Set up your environment variables in `.env`:
   - `DATABASE_PRIVATE_URL` - Your Railway Postgres connection string
   - `BALLDONTLIE_API_KEY` - For NBA and NFL data
   - `SPORTMONKS_API_TOKEN` - For Football/Soccer data

2. Install the seeder package:
   ```bash
   cd seed
   pip install -e .
   ```

## Quick Start

### 1. Bootstrap Teams (One-time per season)

Load team rosters at the start of each season:

**NBA:**
```bash
scoracle-seed bootstrap-teams nba --season 2025
```

**NFL:**
```bash
scoracle-seed bootstrap-teams nfl --season 2025
```

**Football (5 major leagues):**
```bash
# Premier League (England)
scoracle-seed bootstrap-teams football --season 2025 --league 8

# Bundesliga (Germany)
scoracle-seed bootstrap-teams football --season 2025 --league 82

# Ligue 1 (France)
scoracle-seed bootstrap-teams football --season 2025 --league 301

# Serie A (Italy)
scoracle-seed bootstrap-teams football --season 2025 --league 384

# La Liga (Spain)
scoracle-seed bootstrap-teams football --season 2025 --league 564
```

### 2. Load Fixtures

Load the season schedule:

**NBA:**
```bash
scoracle-seed load-fixtures nba --season 2025
```

**NFL:**
```bash
scoracle-seed load-fixtures nfl --season 2025
```

**Football:**
```bash
# Load all 5 leagues
scoracle-seed load-fixtures football --season 2025 --league 8
scoracle-seed load-fixtures football --season 2025 --league 82
scoracle-seed load-fixtures football --season 2025 --league 301
scoracle-seed load-fixtures football --season 2025 --league 384
scoracle-seed load-fixtures football --season 2025 --league 564
```

### 3. Process Fixtures (Fetch Box Scores)

This fetches player and team statistics from completed games:

**Process all pending fixtures (unlimited):**
```bash
scoracle-seed process --sport NBA
scoracle-seed process --sport NFL
scoracle-seed process --sport FOOTBALL
```

**Process specific number of fixtures:**
```bash
scoracle-seed process --sport NBA --max 100
```

**Process all sports at once:**
```bash
scoracle-seed process
```

## Historical Season Seeding

To seed data from past seasons:

### NBA Historical

```bash
# Load 2024 season fixtures
scoracle-seed load-fixtures nba --season 2024 --from-date 2023-10-01 --to-date 2024-06-30

# Process all 2024 games
scoracle-seed process --sport NBA --max 1000
```

### NFL Historical

```bash
# Load 2024 season fixtures
scoracle-seed load-fixtures nfl --season 2024

# Process all 2024 games
scoracle-seed process --sport NFL --max 500
```

### Football Historical

```bash
# Premier League 2023/24 season
# First update provider_seasons table with correct season ID
# Then load fixtures
scoracle-seed load-fixtures football --season 2024 --league 8

# Process
scoracle-seed process --sport FOOTBALL --max 500
```

**Note:** For historical football seasons, you need to map the provider season ID first. Query the Sportmonks API to get the correct season ID for the target year.

## Backfill Command (Historical in One Step)

The `backfill` command combines loading fixtures and processing them:

```bash
# Backfill NBA 2024 season
scoracle-seed backfill nba --season 2024 --from-date 2023-10-01 --to-date 2024-06-30

# Backfill NFL 2024 season  
scoracle-seed backfill nfl --season 2024

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

# Football
scoracle-seed percentiles --sport FOOTBALL --season 2025 --league 8
```

## Data Flow

The seeding process follows this pipeline:

```
1. bootstrap-teams → teams table
2. load-fixtures → fixtures table
3. process → event_box_scores + event_team_stats tables
4. Postgres triggers → player_stats + team_stats (aggregated)
5. percentiles → Percentile calculations + derived stats (PER, QBR, xG, etc.)
6. API Views → nba.player, nba.team, etc. (served to frontend)
```

## Rate Limits

- **Balldontlie:** 600 requests/minute
- **Sportmonks:** 300 requests/minute (on current plan)

The seeder automatically respects these limits with built-in throttling.

## Provider Season IDs (Football)

For football, you need the correct Sportmonks season ID:

| League | League ID | 2024/25 Season ID | 2025/26 Season ID |
|--------|-----------|-------------------|-------------------|
| Premier League | 8 | 23614 | 25583 |
| Bundesliga | 82 | 23744 | 25646 |
| Ligue 1 | 301 | 23643 | 25651 |
| Serie A | 384 | 23746 | 25533 |
| La Liga | 564 | 23621 | 25659 |

These are stored in the `provider_seasons` table and mapped automatically.

## Tips

1. **Run `process` regularly:** Set up a cron job to run every 30 minutes during the season
2. **Process by sport:** During playoffs, you may want to process specific sports more frequently
3. **Monitor coverage:** Use the `percentiles` command to see box score coverage reports
4. **Incremental seeding:** The process command is idempotent - you can run it repeatedly safely

## Troubleshooting

**No fixtures found:**
- Check that teams are bootstrapped first
- Verify provider season IDs are correct in `provider_seasons` table

**Empty box scores:**
- Games may not have started or completed yet
- Check game status in the fixtures table

**API errors:**
- Verify API keys are valid
- Check rate limits haven't been exceeded
- Ensure you're using correct season years for each sport

## Docker

To run via Docker Compose:

```bash
# Full stack
docker compose up --build

# Run seeder
 docker compose run --rm seed process
 docker compose run --rm seed process --sport NBA
```
