# Trends — unit comparability filter + cumulative-total normalization

Date: 2026-05-23

## Goal

The trends payload was shipping cross-unit comparisons that read as nonsense
on the frontend. Concrete case: Tottenham (football, team_id=18) trends card
displaying

    Tackle Success   494   vs 59.4
    Duel Success     178   vs 50.1
    Aerial Success   189   vs 73.2
    Blocked Shots    2.00  vs 138
    Penalties        0.00  vs 4.79

The CTE chain in `trendsStatement` was `jsonb_each`-ing both sides and
averaging every numeric value without any awareness of what each key actually
represented. Some keys were per-fixture raw counts; others were season
cumulative totals; others were already-aggregated rates. Same key name,
mismatched units, meaningless ratio.

## Decisions

- **Move the unit-awareness into `stat_definitions`, not the trends SQL.**
  The decision of "what kind of number does this key carry" is a property of
  the stat, not the consumer. `stat_definitions` is already the single source
  of truth for stat metadata; extending it lets every future consumer (the
  trends endpoint, planned percentage-delta UI, data.scoracle's PostgREST
  surface) read the same flags instead of recomputing the rules.
- **Two new columns, not four.** `unit` carries the why (so future UI can
  label "+15% vs peers" with the right unit suffix). `comparable` is the
  derived flag the SQL filters on. We did not add `aggregation` (sum / avg /
  weighted_avg) — YAGNI until a consumer actually needs it.
- **Unit taxonomy is intentionally coarse**:
  - `rate_pct` — percentages, accuracies, efficiencies (compare directly)
  - `per_game_avg` — per-game / per-36 / per-90 derived stats
  - `cumulative_total` — season cumulative count; needs divisor to compare
  - `special` — standings columns (wins, points, splits) — never a trend stat
- **Normalize peer-side cumulatives in SQL, not in the seeder.** The
  cumulative-vs-per-game mismatch only matters at trends-comparison time, so
  the divide-by-games-played belongs in the trends CTE. Pushing it into the
  seeder would be derived data living in the wrong table.
- **Comparable for cumulatives = TEAM only.** The peer-side normalization
  divides each peer's value by their own `games_played` (NBA/NFL) or
  `matches_played` (football) inside `AVG()` (so a heavily-played team doesn't
  outweigh a less-played team). Divisor coverage is 100% for the team tables.
  For PLAYER cumulatives, divisor coverage is uneven — football player_stats
  has 0% `matches_played` coverage — and every player cumulative already has
  a `_per_game` / `_per_90` derived sibling that carries the same info in a
  directly comparable unit. So player cumulatives stay `comparable = false`
  and the per-game / per-90 siblings serve the trends payload.
- **Entity-side rate_pct values need a guard.** Several per-fixture rate keys
  in the seeded data are non-normalized aggregates rather than actual
  percentages. SportMonks emits `tackles_won_percentage: 700` for a single
  match; the BDL seeder accumulates NBA team event rows by summing player
  rows, producing `fg_pct ≈ 4.0` per team-game. The trends SQL guards the
  entity side: football keeps rate_pct with a `[0, 100]` sanity check; NBA
  and NFL drop rate_pct from the entity side entirely until the seeder is
  fixed (tracked separately — see proposal doc).
- **Rule-based backfill, not per-key enumeration.** Migration 016 uses a
  `CASE` expression over key-name patterns (`*_pct`, `*_per_game`,
  `*_per_90`, etc.) plus a per-sport default. 470 rows backfilled in one
  pass; the migration log surfaces a per-(sport, entity_type, unit) coverage
  table for review.

## Accomplishments

- Migration `016_stat_unit_metadata.sql` adds `unit` + `comparable` to
  `stat_definitions`, backfills all 470 rows by rule, and emits a coverage
  table via `RAISE NOTICE` for review.
- `trendsStatement` in `go/internal/db/db.go` now JOINs `stat_definitions`
  on both sides and filters by `comparable = true`. Cumulative-total keys
  on the peer side are normalized per-team by dividing by the sport's
  divisor key before averaging. The entity-side rate_pct guard is built
  into the helper as a sport-specific clause.
- The helper's package comment documents the unit-handling pipeline and the
  known player-trends limitation on NFL/football (event-row vs season-row
  schema mismatch).
- `ENDPOINTS.md` trends section now documents the comparability filter, the
  cumulative-total normalization, and the known player-trends limitation.
  The profile-page response field list now mentions `unit` / `comparable`
  on the `stat_definitions` array.
- Live verified end-to-end:
  - **Football team 18 (Spurs):** 57 comparable keys, all unit-aligned.
    `tackles: 17.67 vs 16.73`, `pass_accuracy: 86.67 vs 81.77`,
    `passes: 528 vs 431`. The original Spurs bug is fixed.
  - **NBA team 1:** 15 comparable keys, all in matching per-game units.
    `pts: 119.00 vs 115.48`, `ast: 27.00 vs 26.61`.
  - **NBA player:** 16 comparable keys (rate_pct dropped per known
    limitation; per-game stats intact).

## Quick reference

| Item | Path |
|---|---|
| Migration | `sql/migrations/016_stat_unit_metadata.sql` |
| Trends statement | `go/internal/db/db.go` (`trendsStatement`) |
| Public docs | `ENDPOINTS.md` (Trends section) |
| Original trends endpoint doc | `progress_docs/2026-05-22_trends-endpoint.md` |
| Follow-ups proposal | `progress_docs/2026-05-23_event-derivation-proposal.md` |

## Coverage snapshot (post-migration)

```
NBA       player  per_game_avg : 33 (33 comparable)
NBA       player  rate_pct     :  6  (6 comparable)
NBA       player  special      :  1  (0 comparable)
NBA       team    per_game_avg : 17 (17 comparable)
NBA       team    rate_pct     :  6  (6 comparable)
NBA       team    special      :  5  (0 comparable)
NFL       player  cumulative   : 54  (0 comparable — derived siblings carry the unit)
NFL       player  per_game_avg : 52 (52 comparable)
NFL       player  rate_pct     :  5  (5 comparable)
NFL       player  special      :  1  (0 comparable)
NFL       team    cumulative   : 51 (51 comparable — normalized by games_played)
NFL       team    per_game_avg : 14 (14 comparable)
NFL       team    rate_pct     :  7  (7 comparable)
NFL       team    special      :  8  (0 comparable)
FOOTBALL  player  cumulative   : 68  (0 comparable — per_90 siblings carry the unit)
FOOTBALL  player  per_game_avg : 39 (39 comparable)
FOOTBALL  player  rate_pct     :  9  (9 comparable)
FOOTBALL  team    cumulative   : 64 (64 comparable — normalized by matches_played)
FOOTBALL  team    rate_pct     :  9  (9 comparable)
FOOTBALL  team    special      : 21  (0 comparable)
```

Total: **312 / 470** stat-definition rows comparable. NBA + team trends
across all sports are fully comparable; NFL / football player trends are
partial (per-game derived siblings only) pending the follow-up work
described in the proposal doc.

## Known limitations carried into the proposal

1. **Entity-side rate_pct for NBA / NFL** — currently filtered out because
   the seeder either sums player percentages into team event rows (BDL) or
   surfaces non-normalized provider aggregates (SportMonks).
2. **Player trends on NFL / football** — small or empty key intersection
   because `event_box_scores` writes raw counts (`passing_yards`,
   `tackles`) but `player_stats` writes derived per-game / per-90 keys
   (`passing_yards_per_game`, `tackles_per_90`). The frontend's intersection
   logic drops everything since the keys never match.

Both are addressed in `progress_docs/2026-05-23_event-derivation-proposal.md`.
