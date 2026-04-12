# 2026-04-12 Football Event Seeding Complete (All 5 Leagues)

## Summary

Seeded full 2025-26 football season across all 5 benchmark European leagues.
Fixed multiple bugs in the SportMonks handler and SQL aggregation that were
producing empty or incorrect stats. Simplified the Python handler to a thin
raw pass-through aligned with the project's architecture: Python seeds,
Postgres computes.

## What Got Seeded

| League          | ID  | Meta (teams/players) | Fixtures | Box Score Rows |
|-----------------|-----|----------------------|----------|----------------|
| Premier League  | 8   | 20 / 681             | 764      | ~30,500        |
| Bundesliga      | 82  | 18 / 610             | 258      | ~10,300        |
| Ligue 1         | 301 | 18 / 649             | 255      | ~10,100        |
| Serie A         | 384 | 20 / 751             | 315      | ~13,800        |
| La Liga         | 564 | 20 / 727             | 305      | ~14,500        |
| **Total**       |     | **96 / 3,418**       | **1,897**| **~79,200**    |

All seeded with 0 failures.

## Bugs Found and Fixed

### 1. Stats extraction path (Python)
`detail.get("value")` → `detail.get("data", {}).get("value")`.
SportMonks nests stat values in `{"data": {"value": N}}`. The old path
read the wrong level, producing empty stats for every football box score.

### 2. Goals/assists/cards missing (Python)
SportMonks puts these in the `events` array, not in `lineups.details`.
Added `_extract_event_stats()` to count per-player goals (type 14),
assists (related_player_id on goals), yellow cards (type 19), red cards
(types 20/21), penalty goals (type 16), and missed penalties (type 17).

### 3. Scores not extracted (Python)
SportMonks returns scores as `{"score": {"goals": N}}` not a flat number.
Rewrote `_extract_fixture_scores` to handle nested format and take max
across half-time entries for final score.

### 4. Per-90 stats wildly wrong (SQL)
`aggregate_player_season` stored `minutes_played` as average-per-game
instead of total. The derived stats trigger computed `goals * 90 / avg_mins`
giving nonsense values (e.g., Palmer 9.28 goals/90 instead of 0.44).
Fixed to store total minutes.

### 5. Stale rows on re-seed (Python CLI)
Added DELETE before INSERT in `_seed_fixture_box_scores` so re-processing
a fixture clears old rows instead of leaving orphaned data.

## Architectural Cleanup

- Rewrote handler's `get_box_score` as thin raw pass-through: flatten
  `{type.code: data.value}` for every detail, no type coercion
- Replaced `_normalize_player_stats` with 5-line `_flatten_details`
- Removed dead `_extract_value` function
- Removed Dockerfile, railway.toml, .dockerignore (CLI-only operation)
- Removed dead `seed_football.py` orchestration, unused `SeedResult` model
- Expanded SQL `aggregate_player_season` from 13 to 53 stat keys
- Hardcoded API rate limits in clients

## Verified Payloads

Example: Cole Palmer — 21 apps, 1426 mins, 7 goals, 0.442 goals/90, 45 shots.
Example: Man City — 18W/8D/5L, 60 GF, 28 GA, 62 pts.
All 5 leagues serving correct stats, derived per-90 metrics, and percentiles.

## Key Design Decision

Python is a thin pipe. It fetches raw provider data and upserts it.
Postgres handles all normalization (stat key mapping via triggers),
aggregation (season totals via `aggregate_player_season`), derived stats
(per-90, accuracy via `compute_derived_player_stats`), and percentiles
(position-grouped ranking via `recalculate_percentiles`).
