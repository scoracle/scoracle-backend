# 2026-07-05 - Momentum Scores Leaderboard Follow-Up

## Goal

Tighten the DB-first leaderboard/profile split after audit:

- Keep `/profile` as entity drill-down for rich cards.
- Make `/leaderboard` the top-down navigation surface: sport -> league/conference -> division -> team -> player.
- Preserve a full current roster surface even when players have no stats/news product data.
- Move Momentum leaderboard input from request-time derivation into durable DB snapshots.

## Decisions

- Critical design/data-flow distinction:
  - `/leaderboard` surfaces hierarchy. It is the DB navigation surface for moving from sport to league/conference, division, team, and player cohorts.
  - `/profile` surfaces cards. It remains the rich drill-down for one selected entity and should not become the roster/discovery database.
- Full roster visibility lives at the team-scoped player leaderboard:
  `GET /api/v1/{sport}/leaderboard?entity_type=player&team_id={id}`.
- Product boards remain scored projections over that hierarchy. They filter by the same top-down dimensions, but they do not replace the roster surface.
- Momentum is now backed by durable `momentum_scores` snapshots. Upstream Vibe/event-rating writes mark a dirty sport, the API drains that marker, and the leaderboard reads latest stored rows. The refresh path appends snapshots; the cleanup ticker later thins snapshots older than 30 days to the last row per entity per day. Profile `/momentum` still exposes rich per-entity trajectory context.

## Data Flow

Leaderboard hierarchy:

```text
sport
  -> league/conference
  -> division
  -> team
  -> player
```

Football naturally passes through conference/division because those dimensions are
not generally populated there. NBA/NFL can use conference/division when metadata
exists. Team-scoped player discovery is the roster surface, and the default Rating
board is the only board that promises full roster inclusion with null metric/rank
rows appended.

Momentum serving:

```text
vibe_scores / event rating percentiles
  -> mark_momentum_refresh_needed(sport)
  -> momentum_refresh_needed + NOTIFY momentum_refresh_ready
  -> Go maintenance listener drains dirty sports
  -> refresh_momentum_scores(sport)
  -> append momentum_scores snapshot
  -> /leaderboard/momentum reads latest stored rows
```

This is intentionally not a request-time calculation and not a blind timer. The
timer path is only a catch-up drain for missed NOTIFYs; it does nothing without a
dirty marker.

## Changed

- Added migration `128_momentum_scores.sql`:
  - `momentum_scores` table.
  - `refresh_momentum_scores(p_sport text default null)` SQL refresh function.
- `momentum_refresh_needed` dirty queue plus triggers on `vibe_scores`, `event_box_scores`, and `event_team_stats`.
- Added a SQL-only Momentum dirty-queue listener/drain to the Go maintenance worker, with a catch-up drain for missed NOTIFYs.
- The migration marks NBA/NFL/FOOTBALL dirty once for initial post-deploy backfill; after that, refreshes are upstream-triggered.
- Updated `/leaderboard/momentum` prepared statements to read `momentum_scores`.
- Fixed team-scoped Rating roster rows to echo the roster `team_id` instead of a possibly different `player_stats.team_id`.
- Extended Transfers leaderboard parsing/SQL with top-down cohort filters.
- Clarified README/ENDPOINTS wording around roster vs product projections.

## Verification

- `go run github.com/swaggo/swag/cmd/swag@v1.16.6 init -g cmd/api/main.go -o docs`
- `GOCACHE=/tmp/scoracle-go-cache go test ./internal/api ./internal/api/handler ./internal/api/respond ./internal/db ./internal/maintenance`
- `GOCACHE=/tmp/scoracle-go-cache go build -o bin/scoracle-api ./cmd/api`
- Rollback-only Postgres migration validation:
  `BEGIN; \i sql/migrations/128_momentum_scores.sql; ROLLBACK;`

## Production Alignment

Current production state as of the 2026-07-05 hardening pass:

- Migration 128 shipped the DB-first Momentum snapshot path described above.
- Migrations 129 and 130 then hardened it: signed `momentum_score`,
  `idx_momentum_scores_read`, 5-minute refresh coalescing, 30-day-to-daily
  retention thinning, and rating lookback over the entity's last
  `season_bridge_window(sport)` rated games (NBA 8 / NFL 2 / FOOTBALL 4).
- Production has 128, 129, and 130 recorded in `schema_migrations`, and
  `sql/schema/schema.sql` / `sql/schema/schema_migrations.txt` are folded
  through `130_momentum_game_lookback`.
- Production API and Rust cognition are serving commit `925275924b0e`.
- Live smoke checks pass for `/leaderboard?board=momentum`,
  `/leaderboard/momentum`, `/leaderboard/trending`, `/health/db`, and the
  team-scoped player leaderboard roster surface.
