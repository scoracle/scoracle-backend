# 2026-04-19 — Expose all box-score datapoints in sport payloads

## Goals

Frontend charts were hitting gaps: Football team payload had no fouls, cards,
tackles, passing, or possession-related stats; NBA team payload only surfaced
~10 of 19 available box-score keys; NFL team payload was missing defense,
special teams, and turnover-differential. Goal: expose every datapoint the
box scores already carry, with percentiles, so the frontend can cherry-pick
without requiring schema changes later.

## Decisions

- **Postgres-only changes.** All new stats derive from `event_box_scores` /
  `event_team_stats` via the existing `aggregate_*_season()` functions.
  No new provider calls, no Python or Go changes.
- **Derived rate stats computed from sums, not from averaging per-match
  percentages.** e.g. team `pass_accuracy = SUM(accurate_passes) / SUM(passes)`,
  never `AVG(accurate_passes_percentage)`.
- **Percentile eligibility** toggled per key based on directional meaning
  (`is_inverse=true` for cards, turnovers, fouls committed, big chances missed,
  etc.). Frontend can ignore the flag if it wants raw ranking.
- **Possession % for football is genuinely unavailable** — SportMonks does not
  send `ball_possession` in the fixture payload. Only `possession_lost` (a
  negative counter) flows through. Noted here so future devs don't hunt for
  it. Proxy via `passes / total_passes_both_teams` if ever needed.
- **`jsonb_build_object` 100-argument cap** forced splitting the Football team
  aggregate and NFL player/team aggregates into `obj1 || obj2` concatenation.
  Kept each half semantically grouped (core → advanced, offense → special
  teams).

## Accomplishments

### Football (`sql/football.sql`)

- Added 55+ team-level stat_definitions spanning defensive, passing, shooting,
  attacking, duels, possession, goalkeeping, discipline.
- Rewrote `aggregate_team_season()` to pull every numeric key from
  `event_team_stats.stats`, including derived team-season percentages
  (`pass_accuracy`, `shot_accuracy`, `cross_accuracy`, `long_ball_accuracy`,
  `dribble_success_rate`, `duels_won_percentage`, `aerials_won_percentage`,
  `tackles_won_percentage`).
- Added 40+ player-level stat_definitions for previously unregistered
  (but aggregated) keys plus new ones (`duels_lost`, `aerials_lost`, `turnovers`,
  `offsides_provoked`, `through_balls_won`, `error_lead_to_shot/goal`,
  `last_man_tackle`, `clearance_offline`, `penalties_committed`,
  `yellowred_cards`, `motm_awards`, `rating_avg`).
- Extended `aggregate_player_season()` with the new sums + `rating_avg`
  (the only AVG, to express average match rating).
- Added derived trigger computations: `tackles_won_percentage`,
  `cross_accuracy`, `long_ball_accuracy`, `aerials_won_percentage`.

### NBA (`sql/nba.sql`)

- Added player-level advanced stat_definitions: `efg_pct`, `ast_to_tov`,
  `tov_per_36`, `pf_per_36`.
- Added trigger computations for the four new derived stats.
- Expanded team-level stat_definitions from 10 → 28 keys, adding defensive
  (`stl`, `blk`, `oreb`, `dreb`, `pf`), raw shooting totals (`fgm`-`fta`),
  `pts_allowed`, `point_differential`, and advanced (`true_shooting_pct`,
  `efg_pct`, `ast_to_tov`, `efficiency`).
- Rewrote `aggregate_team_season()` to pull opponent score/stats (via the
  existing opp self-join) for `pts_allowed` and to produce the full 19-key
  per-game average set.
- Expanded `compute_derived_team_stats()` to compute win_pct, point
  differential, TS%, eFG%, ast/tov, and efficiency at team level.

### NFL (`sql/nfl.sql`)

- Added player-level stat_definitions for `qb_rating`, `yards_per_pass_attempt`,
  `sacks_taken`, `sack_yards_lost`, `long_pass`, `long_rushing`, `long_reception`,
  `long_field_goal_made`, `long_punt`, `long_kick_return`, `long_punt_return`,
  `avg_punt_yards`, `yards_per_kick_return`, `yards_per_punt_return`,
  `interception_yards`, `fumbles_touchdowns`, `fumbles_recovered`.
- Added team-level stat_definitions for ~30 new keys: full defensive set
  (`solo_tackles`, `tackles_for_loss`, `qb_hits`, `interception_touchdowns`,
  `fumbles_recovered`, `fumbles_touchdowns`), special teams (punts, kick/punt
  returns and their per-return averages), per-game rates, completion %,
  YPA/YPC, `takeaways`, `turnover_differential`, `qbr`, `qb_rating`.
- Extended `aggregate_player_season()` with `MAX()` aggregations for all
  season-longest fields (`long_*`) and derived rate keys
  (`avg_punt_yards`, `yards_per_kick_return`, `yards_per_punt_return`,
  `yards_per_pass_attempt`).
- Rewrote `aggregate_team_season()` with opponent joins for
  `turnover_differential` and `takeaways`, plus per-game scoring, YPA/YPC,
  completion %, and all special-teams sums.

## Quick Reference

| Sport     | Team stats | Team pct | Player stats | Player pct |
|-----------|------------|----------|--------------|------------|
| Football  | 76         | 77       | 75           | 74         |
| NBA       | 28         | 26       | 24           | 22         |
| NFL       | 57         | 54       | 55           | 35         |

(Team percentile count exceeds stats by one because the payload also carries
`_sample_size` metadata; views strip it before returning to the client.)

### Backfill commands (safe to rerun)

```sql
-- Regenerate team_stats payloads after aggregate function change
UPDATE team_stats ts
SET stats = football.aggregate_team_season(ts.team_id, ts.season, ts.league_id),
    updated_at = NOW()
WHERE ts.sport = 'FOOTBALL';
-- (repeat for NBA/NFL with nba./nfl. prefix)

-- Regenerate player_stats + trigger-derived fields
UPDATE player_stats ps
SET stats = football.aggregate_player_season(ps.player_id, ps.season, ps.league_id),
    updated_at = NOW()
WHERE ps.sport = 'FOOTBALL';

-- Recalculate percentiles per season
SELECT season, (recalculate_percentiles('FOOTBALL', season)).*
FROM (SELECT DISTINCT season FROM team_stats WHERE sport='FOOTBALL') s;
```

## Watch-outs / Follow-ups

- **SportMonks key inconsistency.** Football team stats store `passes` /
  `accurate_passes` / `total_crosses` / `accurate_crosses` while player stats
  (same lineup detail source) store `passes_total` / `passes_accurate` /
  `crosses_total` / `crosses_accurate`. Aggregator handles the mapping, but
  if the provider shape changes, both halves need an audit.
- **SportMonks typo preserved.** Keys `aeriels_won` / `aeriels_lost` are the
  literal codes SportMonks emits; we read those but expose `aerials_*` in the
  payload. Do not "fix" the source-side spelling — it will break the pipeline.
- **Possession %.** Still unavailable. If added in a future provider upgrade,
  register `ball_possession` as a team stat_def and pull `AVG(...)` from
  `event_team_stats.stats`.
- **NFL player percentile count (35) is lower than stat count (55)** because
  many new keys (long_*, totals for per-game kickers/punters) are registered
  with `is_percentile_eligible=false`. Intentional — ranking a kicker's
  longest-punt distance vs. a QB's longest-pass would be meaningless.

## Files Changed

- `sql/football.sql`
- `sql/nba.sql`
- `sql/nfl.sql`
