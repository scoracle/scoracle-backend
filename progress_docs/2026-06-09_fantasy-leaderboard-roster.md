# 2026-06-09 — Fantasy on Leaderboard + Roster (Phase 2b)

## Goals
Surface the migration-046 fantasy points in the two ranked-list surfaces: a Fantasy
board on the standalone /leaderboard, and a Fantasy column on the team Roster card.

## Decisions
- **No new SQL / migration** — fantasy_points already exists in `stats` (046). Both
  surfaces just read it; this is purely a prepared-statement change in `db.go`.
- **Leaderboard**: reuse the existing `scope` param. `scope = 'fantasy'` adds a third
  ORDER BY branch (`(ps.stats->>'fantasy_points')::numeric DESC`), a passthrough in the
  specialty filter, and a `> 0` guard (excludes the ~9.8k zero-fantasy NFL defenders).
  The row carries `fantasy_points` (value) + `fantasy_rank` (percentile, for the chip
  color). Player-only — teams have no fantasy_points, so the team branch selects NULLs
  and the team specialty filter excludes 'fantasy' naturally.
- **Roster**: one column added to the `roster` statement select
  (`(ps.stats->>'fantasy_points')::numeric AS fantasy_points`). Ordering unchanged
  (Composite+Specialist sum); the column is display-only.

## Accomplishments
- `go/internal/db/db.go` — `leaderboard` statement: fantasy ORDER BY branch +
  `fantasy_points`/`fantasy_rank` columns (player) / NULLs (team) + `'fantasy'` in the
  scope passthrough + `>0` guard. `roster` statement: `fantasy_points` column.

## Verification
- Prod queries: NBA fantasy board → Jokić 62.93 / Luka 59.78 / Wembanyama 51.28 / SGA /
  Giannis; NFL → McCaffrey 458.4 / Nacua 452.6 / Allen 414.8 — correct names + values.
- `gofmt`/`go build`/`go vet` clean. Rebuilt + restarted scoracle-api (no degraded
  mode). Live endpoints: `/nba/leaderboard?scope=fantasy` (count 50, Jokić #1);
  `/nba/team/15/roster` carries fantasy_points. Deployed.

## Quick reference
- Fantasy leaderboard: `GET /api/v1/{sport}/leaderboard?scope=fantasy&type=player`
  (nba/nfl). Ranks by `stats.fantasy_points` (NBA per-game / NFL season-total — the
  sport's profile-default fantasy headline). Rate variants are a fast-follow.
- Roster payload now includes `fantasy_points` per player (null for football).
