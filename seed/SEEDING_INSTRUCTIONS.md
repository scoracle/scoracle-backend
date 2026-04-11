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

## Provider Endpoint Notes

- NBA metadata profile seeding uses BallDontLie `GET /v1/players` and `GET /v1/teams` (with compatibility fallback to `/nba/v1/...`).
- NFL metadata profile seeding uses `GET /nfl/v1/players` and `GET /nfl/v1/teams`.
- Football metadata profile seeding uses `GET /players/{id}` (SportMonks) and season team/squad endpoints.

## Recommended "Fill Missing Holes" Workflow

1. Load/refresh fixtures for each sport.
2. Run `event process` in batches until failures are near zero.
3. Run `meta seed` for each sport to backfill profile metadata.
4. Re-run `event process` to catch anything that became ready since the prior pass.

## Docker Usage

```bash
# Load fixtures
docker compose run --rm seed event load-fixtures nba --season 2025

# Process event data
docker compose run --rm seed event process --sport nba --season 2025 --max 100

# Seed metadata
docker compose run --rm seed meta seed nba --season 2025
```

## League IDs (Football)

| League | ID |
|---|---|
| Premier League | 8 |
| Bundesliga | 82 |
| Ligue 1 | 301 |
| Serie A | 384 |
| La Liga | 564 |

See `BOX_SCORE_ENDPOINTS.md` and `META_ENDPOINTS.md` for provider endpoint details.
