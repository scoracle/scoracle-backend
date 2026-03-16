# Session: Consolidate PostgREST Player/Team Endpoints
**Date:** 2026-03-16

## Goals
- Reduce frontend API calls by combining profile and stats endpoints
- Move from 4 views per sport (`players`, `player_stats`, `teams`, `team_stats`) to 2 (`player`, `team`)
- Serve metadata alongside stats in a single response

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| Replace old views rather than keep both | Avoids stale endpoint sprawl; old views were redundant since new views serve both use cases |
| Use LEFT JOIN to stats tables | Players/teams without stats still appear (with NULL stats fields) |
| Singular naming (`player`/`team`) | Signals "detail endpoint" vs the plural collection pattern |
| Flatten profile fields, nest team/league as JSON | Consistent with existing patterns; allows PostgREST filtering on profile fields |
| Rename `updated_at` to `stats_updated_at` | Avoids ambiguity with player/team record timestamps |

## Accomplishments
### Updated
- `sql/nba.sql` — Replaced `nba.players`, `nba.player_stats`, `nba.teams`, `nba.team_stats` with `nba.player` and `nba.team`
- `sql/nfl.sql` — Replaced `nfl.players`, `nfl.player_stats`, `nfl.teams`, `nfl.team_stats` with `nfl.player` and `nfl.team`
- `sql/football.sql` — Replaced `football.players`, `football.player_stats`, `football.teams`, `football.team_stats` with `football.player` and `football.team` (with league context)

## Quick Reference
Frontend migration — before (2 calls):
```
GET /rest/v1/players?id=eq.237         Accept-Profile: nba
GET /rest/v1/player_stats?player_id=eq.237&season=eq.2025  Accept-Profile: nba
```

After (1 call):
```
GET /rest/v1/player?id=eq.237&season=eq.2025  Accept-Profile: nba
```

Key behaviors:
- No stats yet → 1 row with `season`, `stats`, `percentiles`, `percentile_metadata` all NULL
- Multiple seasons → multiple rows; filter with `&season=eq.2025`

## Not Affected
- `standings`, `stat_leaders()`, `autofill_entities` — all query `public.*` tables directly
- Go API, Python seeder, triggers, derived stats — no changes needed
