# Exclude 0-Minute Events from Season Aggregation

**Date:** 2026-04-12

## Problem

Per-game and per-36/per-90 stats were diluted by DNP/injured events (0 minutes logged). Players like Cade Cunningham who missed games had their averages corrupted because `aggregate_player_season()` counted every box score row regardless of participation.

## Solution

Added a filter to each sport's `aggregate_player_season()` function to discard non-participation events before aggregating:

- **NBA** (`sql/nba.sql`): `AND COALESCE(minutes_played, 0) > 0`
- **Football** (`sql/football.sql`): `AND COALESCE(minutes_played, 0) > 0`
- **NFL** (`sql/nfl.sql`): Excludes events where all counting stats (passing/rushing/receiving yards, tackles, fumbles) are zero

## Impact

- `games_played` / `matches_played` / `appearances` now reflect actual participation only
- Per-game averages and derived per-36/per-90 stats are correctly computed
- Team aggregation functions unaffected (they use `event_team_stats`, not `event_box_scores`)
- Existing `player_stats` rows need re-aggregation after deploy

## Files

- `sql/nba.sql` — line 381
- `sql/football.sql` — line 392
- `sql/nfl.sql` — lines 423–429
