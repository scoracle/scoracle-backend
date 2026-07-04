# Seeding Instructions

This seeder is CLI-driven and lean:

1. **Event seeding** (`scoracle-seed event ...`) — fixtures + box scores
2. **Roster seeding** (`scoracle-seed roster seed ...`) — season-scoped team
   membership in `team_rosters`
3. **Meta seeding** (`scoracle-seed meta seed ...`) — team + player profile
   enrichment for the roster-scoped universe
4. **Image seeding** (`scoracle-seed meta images ...`) — team logos + player
   headshots from api-sports, NBA + NFL only

No scheduler, daemon, or LISTEN/NOTIFY runtime is required.

## Critical Seeding-Layer Boundary

Roster membership owns player discovery. Metadata is an enrichment pass.

For NBA/NFL, BDL's player-list endpoints can return historical league-wide
payloads. Do not use those lists as the set of players Scoracle covers, and do
not fix that mistake with a broad statless purge. Run `roster seed` first; it
writes active season membership into `team_rosters`. Then run `meta seed`; it
fetches profiles only for players already present on active `team_rosters` rows
for the requested season. Statless rostered players are valid and must stay.

## Prerequisites

Real values live in `.env.local` (gitignored). `.env` is the committed
template with placeholders. Required keys:

- `DATABASE_PRIVATE_URL` (or `DATABASE_URL`)
- `BALLDONTLIE_API_KEY` — NBA + NFL
- `SPORTMONKS_API_TOKEN` — football
- `API_SPORTS_KEY` — NBA + NFL image metadata only

Install:

```bash
cd seed
pip install -e .
```

Activate + load env once per shell:

```bash
source .venv/bin/activate
set -a; source .env.local; set +a
```

## Event Seeding (Fixtures + Box Scores)

### 1. Load Fixtures

```bash
# NBA
scoracle-seed event load-fixtures nba --season 2025

# NFL
scoracle-seed event load-fixtures nfl --season 2025

# Football — one league
scoracle-seed event load-fixtures football --season 2025 --league 8

# Football — all leagues with a provider_seasons row (recommended)
scoracle-seed event load-fixtures football --season 2025
```

### 2. Process Pending Fixtures

```bash
# Drain everything pending
scoracle-seed event process

# Scoped by sport + season
scoracle-seed event process --sport nba --season 2025
scoracle-seed event process --sport nfl --season 2025
scoracle-seed event process --sport football --season 2025

# Optional cap
scoracle-seed event process --sport nba --season 2025 --max 100
```

`event process` writes to `event_box_scores` + `event_team_stats`, then
calls `finalize_fixture()` in Postgres for aggregation + percentiles.
Once a fixture's status is `'seeded'` it won't be picked up again.

## Meta Seeding (Team + Player Profiles)

Run after roster seeding at season start and on a weekly refresh (see
`planning_docs/CRON_SEEDING_STRATEGY.md`). The order matters:

```bash
# NBA
scoracle-seed roster seed nba --season 2025
scoracle-seed meta seed nba --season 2025

# NFL
scoracle-seed roster seed nfl --season 2025
scoracle-seed meta seed nfl --season 2025

# Football — one league
scoracle-seed roster seed football --season 2025 --league 8
scoracle-seed meta seed football --season 2025 --league 8

# Football — all configured leagues (recommended)
scoracle-seed roster seed football --season 2025
scoracle-seed meta seed football --season 2025
```

Optional scoping:

```bash
scoracle-seed roster seed nfl --season 2025 --max-teams 2 --max-players 500
scoracle-seed meta seed nfl --season 2025 --max-teams 2 --max-players 500
```

## Image Seeding (Logos + Headshots)

api-sports fills the `logo_url` / `photo_url` gap that BDL leaves for
NBA + NFL. Image CDN requests don't count toward the 100/day free
quota. Run once per season.

```bash
# Dry run first — matches + logs would-be writes without touching DB
scoracle-seed meta images nba --season 2025 --dry-run

# Real runs
scoracle-seed meta images nba --season 2025
scoracle-seed meta images nfl --season 2025
```

Per-run cost: ~31 calls (NBA), 33 (NFL). Football images already
come from SportMonks `image_path`, no separate seed needed.

## Football: provider_seasons setup

Football seeding needs a `provider_seasons` row per (league, season)
to map our league IDs to SportMonks season IDs. Run once, then the
pipeline picks up those leagues automatically.

**2025/26 season (current):**

```sql
INSERT INTO provider_seasons (league_id, season_year, provider_season_id, provider) VALUES
  (8,   2025, 25583, 'sportmonks'),  -- Premier League
  (82,  2025, 25646, 'sportmonks'),  -- Bundesliga
  (301, 2025, 25651, 'sportmonks'),  -- Ligue 1
  (384, 2025, 25533, 'sportmonks'),  -- Serie A
  (564, 2025, 25659, 'sportmonks')   -- La Liga
ON CONFLICT (league_id, season_year, provider) DO NOTHING;
```

**2024/25 season (prior):**

```sql
INSERT INTO provider_seasons (league_id, season_year, provider_season_id, provider) VALUES
  (8,   2024, 23614, 'sportmonks'),
  (82,  2024, 23744, 'sportmonks'),
  (301, 2024, 23643, 'sportmonks'),
  (384, 2024, 23746, 'sportmonks'),
  (564, 2024, 23621, 'sportmonks')
ON CONFLICT (league_id, season_year, provider) DO NOTHING;
```

For older seasons, pull the SportMonks season IDs from
`GET /v3/football/leagues/{id}?include=seasons&api_token=...` and
insert rows with the matching `season_year`.

## Quick Smoke Validation

Minimal end-to-end verification:

```bash
# Tiny roster + meta seed per sport. Meta requires active team_rosters rows.
scoracle-seed roster seed nba --season 2025 --max-teams 1 --max-players 1
scoracle-seed meta seed nba --season 2025 --max-teams 1 --max-players 1

scoracle-seed roster seed nfl --season 2025 --max-teams 1 --max-players 1
scoracle-seed meta seed nfl --season 2025 --max-teams 1 --max-players 1

scoracle-seed roster seed football --season 2025 --league 8 --max-teams 1 --max-players 1
scoracle-seed meta seed football --season 2025 --league 8 --max-teams 1 --max-players 1

# One event per sport
scoracle-seed event load-fixtures nba --season 2025 --from-date 2026-01-15 --to-date 2026-01-15
scoracle-seed event process --sport nba --season 2025 --max 1

scoracle-seed event load-fixtures nfl --season 2025 --from-date 2026-01-04 --to-date 2026-01-04
scoracle-seed event process --sport nfl --season 2025 --max 1

scoracle-seed event load-fixtures football --season 2025 --league 8
scoracle-seed event process --sport football --season 2025 --max 1
```

## Full Season Seed

### All five football leagues, current season

```bash
# 1. Ensure provider_seasons rows exist (see SQL above)
# 2. Run the pipeline — --league omitted iterates every configured league
scoracle-seed roster seed football --season 2025
scoracle-seed meta seed football --season 2025
scoracle-seed event load-fixtures football --season 2025
scoracle-seed event process --sport football --season 2025
```

### NBA + NFL, current season

```bash
scoracle-seed roster seed nba --season 2025
scoracle-seed meta seed nba --season 2025
scoracle-seed event load-fixtures nba --season 2025
scoracle-seed event process --sport nba --season 2025

scoracle-seed roster seed nfl --season 2025
scoracle-seed meta seed nfl --season 2025
scoracle-seed event load-fixtures nfl --season 2025
scoracle-seed event process --sport nfl --season 2025
```

## Seeding Previous Years (Backfill)

### NBA / NFL — BDL

BDL historical event data works the same way. Roster still comes first for
each backfilled season, because metadata enriches `team_rosters` and must not
page through BDL's historical player universe.

```bash
# NBA 2024-25 season
scoracle-seed roster seed nba --season 2024
scoracle-seed meta seed nba --season 2024
scoracle-seed event load-fixtures nba --season 2024
scoracle-seed event process --sport nba --season 2024

# NBA 2023-24 season
scoracle-seed roster seed nba --season 2023
scoracle-seed meta seed nba --season 2023
scoracle-seed event load-fixtures nba --season 2023
scoracle-seed event process --sport nba --season 2023

# NFL 2024 season
scoracle-seed roster seed nfl --season 2024
scoracle-seed meta seed nfl --season 2024
scoracle-seed event load-fixtures nfl --season 2024
scoracle-seed event process --sport nfl --season 2024
```

BDL's GOAT tier has full history. Free / All-Star tiers may only go
back a few seasons — confirm on your plan before backfilling deep.

### Football — SportMonks

Each prior season needs its own `provider_seasons` row (see SQL
blocks above). Once that's in place:

```bash
# 2024/25 season — after adding provider_seasons rows above
scoracle-seed roster seed football --season 2024
scoracle-seed meta seed football --season 2024
scoracle-seed event load-fixtures football --season 2024
scoracle-seed event process --sport football --season 2024
```

Loop multiple seasons:

```bash
for Y in 2023 2024; do
  scoracle-seed roster seed football --season $Y
  scoracle-seed meta seed football --season $Y
  scoracle-seed event load-fixtures football --season $Y
  scoracle-seed event process --sport football --season $Y
done
```

### Backfill cost awareness

Backfills are expensive on API quota. Rough per-season call counts:

| Pipeline      | BDL (per season)         | SportMonks (per league/season) |
|---------------|--------------------------|--------------------------------|
| Roster + meta | active roster + profiles | squad + profiles               |
| Load fixtures | ~13 pages                | ~8 pages                       |
| Event process | ~1,300 (NBA) / ~285 (NFL)| ~380                           |

Plan backfills during quiet provider-side windows and monitor quota
usage as you go.

## Architecture: Python Seeder Role

Python is a **thin pipe**. It fetches raw data from provider APIs and
upserts it to Postgres. It does not compute derived stats, per-90
metrics, or percentiles — that's Postgres's job.

For football event seeding, the handler:

1. Flattens `lineups.details[].type.code → details[].data.value` into
   raw stats JSONB
2. Counts goals / assists / cards from the `events` array (structural
   transformation, not filtering)
3. Extracts scores from the `scores` array
4. Upserts to `event_box_scores` + `event_team_stats`
5. Calls `finalize_fixture()` — Postgres handles aggregation,
   derived stats, and percentiles

## Provider Endpoint Notes

| Provider     | Base URL                                  | Used for            |
|--------------|-------------------------------------------|---------------------|
| BallDontLie  | `https://api.balldontlie.io`              | NBA + NFL           |
| SportMonks   | `https://api.sportmonks.com/v3/football`  | Football            |
| api-sports   | `https://v2.nba.api-sports.io` / `https://v1.american-football.api-sports.io` | Image metadata only |

## Recommended Seeding Workflow

1. **`roster seed`** each sport/league — writes season-scoped
   `team_rosters` membership.
2. **`meta seed`** each sport/league — enriches only active
   `team_rosters` players for that season.
3. **`event load-fixtures`** each sport/league.
4. **`event process`** to seed box scores. Re-run idempotently to
   catch newly completed fixtures.
5. **`meta images`** once per season, NBA + NFL only (optional).

## League IDs (Football)

| League | ID | 2024/25 SportMonks season | 2025/26 SportMonks season |
|---|---|---|---|
| Premier League | 8 | 23614 | 25583 |
| Bundesliga | 82 | 23744 | 25646 |
| Ligue 1 | 301 | 23643 | 25651 |
| Serie A | 384 | 23746 | 25533 |
| La Liga | 564 | 23621 | 25659 |

## Scheduling

See `planning_docs/CRON_SEEDING_STRATEGY.md` for the recommended
cadence:

- **Daily 23:00 ET** — `event process --sport football --season 2025`
- **Weekly 23:00 ET Monday** — full refresh
  (`roster seed` + `meta seed` + `load-fixtures`)
- **Future** — NBA + NFL will move off cron when BDL webhooks are wired
