# Seeding Instructions

This seeder is intentionally CLI-driven and lean:

1. **Event seeding** (`scoracle-seed event ...`) for fixtures + box scores.
2. **Meta seeding** (`scoracle-seed meta ...`) for team/player profiles.

No scheduler, daemon, or LISTEN/NOTIFY runtime is required for these flows.

## Prerequisites

1. Configure environment variables:
   - `DATABASE_URL` (or `DATABASE_PRIVATE_URL` / `RAILWAY_DATABASE_URL`)
   - `BALLDONTLIE_API_KEY` (NBA/NFL)
   - `SPORTMONKS_API_TOKEN` (Football)
2. Install package:

```bash
cd seed
pip install -e .
```

## Event Seeding (Fixtures + Box Scores)

### 1. Load Fixtures

```bash
# NBA
scoracle-seed event load-fixtures nba --season 2025

# NFL
scoracle-seed event load-fixtures nfl --season 2025

# Football (league required; 2025 == 2025/26 season)
scoracle-seed event load-fixtures football --season 2025 --league 8
```

### 2. Process Pending Fixtures

```bash
# Any sport
scoracle-seed event process --max 100

# Scoped by sport + season
scoracle-seed event process --sport nba --season 2025 --max 100
scoracle-seed event process --sport nfl --season 2025 --max 100
scoracle-seed event process --sport football --season 2025 --max 100
```

`event process` writes fixture-level rows to:
- `event_box_scores`
- `event_team_stats`

Then it calls `finalize_fixture()` in Postgres to handle aggregation and percentiles.

## Meta Seeding (Profiles)

Run periodically (for example, season start + a couple refreshes per year):

```bash
# NBA profiles
scoracle-seed meta seed nba --season 2025

# NFL profiles
scoracle-seed meta seed nfl --season 2025

# Football profiles (league required; 2025 == 2025/26 season)
scoracle-seed meta seed football --season 2025 --league 8
```

Optional throttle for controlled runs:

```bash
scoracle-seed meta seed nfl --season 2025 --max-teams 2 --max-players 500
```

## Quick Smoke Validation (Minimal Run)

Use this to verify end-to-end seeding without doing a full pass:

```bash
# One team + one player metadata seed per sport
scoracle-seed meta seed nba --season 2025 --max-teams 1 --max-players 1
scoracle-seed meta seed nfl --season 2025 --max-teams 1 --max-players 1
scoracle-seed meta seed football --season 2025 --league 8 --max-teams 1 --max-players 1

# One event per sport (load fixtures first, then process 1)
scoracle-seed event load-fixtures nba --season 2025
scoracle-seed event process --sport nba --season 2025 --max 1

scoracle-seed event load-fixtures nfl --season 2025
scoracle-seed event process --sport nfl --season 2025 --max 1

scoracle-seed event load-fixtures football --season 2025 --league 8
scoracle-seed event process --sport football --season 2025 --max 1
```

## Architecture: Python Seeder Role

Python is a **thin pipe**. It fetches raw data from provider APIs and upserts it to Postgres.
It does not compute derived stats, per-90 metrics, or percentiles — that is Postgres's job.

For football event seeding, the handler:
1. Flattens lineup `details[].type.code → details[].data.value` into a raw stats JSONB
2. Counts goals/assists/cards from the `events` array (structural transformation, not filtering)
3. Extracts scores from the `scores` array
4. Upserts to `event_box_scores` and `event_team_stats`
5. Calls `finalize_fixture()` which triggers Postgres aggregation, derived stats, and percentiles

## Provider Endpoint Notes

- NBA/NFL: BallDontLie API (`/nba/v1/...`, `/nfl/v1/...`)
- Football: SportMonks API (`/v3/football/...`)

## Recommended Seeding Workflow

1. `meta seed` for each sport/league (profiles first — players need to exist for box scores).
2. `event load-fixtures` for each sport/league.
3. `event process` to seed box scores. Re-run to catch newly completed fixtures.

## Full Football Season Seed (all 5 leagues)

```bash
# 1. Metadata
for LEAGUE in 8 82 301 384 564; do
  scoracle-seed meta seed football --season 2026 --league $LEAGUE
done

# 2. Fixtures
for LEAGUE in 8 82 301 384 564; do
  scoracle-seed event load-fixtures football --season 2026 --league $LEAGUE
done

# 3. Box scores
scoracle-seed event process --sport football
```

## League IDs (Football)

| League | ID |
|---|---|
| Premier League | 8 |
| Bundesliga | 82 |
| Ligue 1 | 301 |
| Serie A | 384 |
| La Liga | 564 |
