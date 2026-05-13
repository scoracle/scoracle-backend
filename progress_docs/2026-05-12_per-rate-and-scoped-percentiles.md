# Per-36 / per-90 expansion + scoped percentiles

Date: 2026-05-12

## Goal

Two product gaps in the player/team profile payload:

1. **Per-rate coverage.** The NBA player trigger only emitted per-36 for 7
   stats (pts/reb/ast/stl/blk/turnover/pf), and Football per-90 covered 7 of
   ~30 candidate volume stats. High-usage role players looked identical to
   bench players on raw FGA / passes / duels because nothing normalized for
   minutes. Per-36 / per-90 is the comparison axis users expect.
2. **Percentile granularity.** `recalculate_percentiles()` produced
   sport-wide percentiles only. A Premier League midfielder was being
   compared against the global pool of football players — including lower
   tiers — washing out the comparison meaning. Same for NBA/NFL: no
   conference-scoped view existed.

## Decisions

- **Loop-driven trigger pattern.** Both player triggers (`nba.compute_derived_player_stats`,
  `football.compute_derived_player_stats`) now drive their per-rate
  derivations from a `TEXT[]` array of base keys + a `FOREACH` loop, so
  adding a new per-rate stat is a one-line array edit instead of a new
  inline `IF / jsonb_build_object` block. The accuracy / efficiency / TS%
  formulas stay inline because they read multiple inputs and aren't a
  uniform mapping.
- **`<base>_per_36` and `<base>_per_90` naming convention.** Every new
  derived key follows this. The one exception is the Football
  `shots_total → shots_per_90` legacy alias, which is preserved alongside
  the new `shots_total_per_90` key the loop emits — frontend can migrate
  to the canonical key on its own schedule.
- **`scoped_percentiles` is a sibling JSONB column, not nested in `percentiles`.**
  Keeps the existing `percentiles` shape (and its NOTIFY trigger that
  diffs OLD vs NEW for FCM milestones) untouched. New column has its own
  metadata fields (`scope_type`, `scope_id`, `scope_name`) alongside the
  same `_position_group` / `_sample_size` shape so the response surface
  stays consistent.
- **Scope axis.** Football scope = `league_id` (so a Premier League player
  is ranked vs Premier League peers; a Bundesliga player vs Bundesliga
  peers). NBA/NFL scope = `teams.conference` (Eastern vs Western, AFC vs
  NFC). Players carry two percentile views: the existing sport-wide one
  and the new scope-narrow one.
- **Player partition includes position; team partition does not.** Per the
  product call: position is the most meaningful axis, so scoped player
  percentiles partition by `(position, scope)` — a Premier League striker
  is compared against Premier League strikers, not Premier League
  defenders. Teams have no position so the partition is just `scope`.
- **No sample-size floor.** All buckets emit, regardless of size. Sample
  size is exposed in metadata so the frontend can render a "low sample"
  indicator if/when desired. Trade-off: niche NFL position groups within
  a single conference could land at <10 players where percentile noise is
  high. The product call was that emitting beats omitting; UI handles
  presentation.

## Accomplishments

### NBA per-36
Added 8 new derived keys: `oreb_per_36`, `dreb_per_36`, `fgm_per_36`,
`fga_per_36`, `fg3m_per_36`, `fg3a_per_36`, `ftm_per_36`, `fta_per_36`.
All marked `is_percentile_eligible=true`. Trigger refactored to a single
loop over `per_36_keys` — handles all 15 base stats uniformly.

### Football per-90
Added 31 new derived keys covering xG, shots-on-target, passes (total +
accurate), crosses (total + accurate), clearances, blocks, duels (total +
won), dribbles (attempts + success), saves, saves-inside-box, chances
created, big chances created, long balls (total + won), through balls
(total + won), final-third passes, tackles won, dribbled past,
dispossessed, possession lost, turnovers, ball recovery, aerials (total +
won), fouls (committed + drawn). Inverse propagation matches base stats
(`dispossessed_per_90`, `possession_lost_per_90`, etc. all marked
inverse). `shots_per_90` legacy alias preserved.

### Scoped percentiles
- New columns `player_stats.scoped_percentiles JSONB` and
  `team_stats.scoped_percentiles JSONB`.
- `recalculate_percentiles()` extended with two additional CTE blocks
  that compute the scoped percentiles in the same call. Joins use the
  `(player_id, league_id)` composite to handle Football players who
  appear in multiple leagues — each row gets its own scoped row.
- Six views updated (`nba.player`, `nba.team`, `nfl.player`, `nfl.team`,
  `football.player`, `football.team`) to expose `scoped_percentiles` and
  `scoped_percentile_metadata` alongside the existing `percentiles` /
  `percentile_metadata` pair.

### Migration + backfill
`sql/migrations/012_per_rate_and_scoped_percentiles.sql` runs:
1. Column adds (idempotent `ADD COLUMN IF NOT EXISTS`).
2. Stat definition inserts (39 new rows, idempotent).
3. Trigger function replacements.
4. `recalculate_percentiles` replacement.
5. Six view replacements.
6. `UPDATE player_stats SET stats = stats WHERE sport IN ('NBA','FOOTBALL')`
   to refire the BEFORE INSERT OR UPDATE trigger and emit the new derived
   per-rate keys onto every existing row.
7. Loop over `(sport, season)` distinct tuples calling
   `recalculate_percentiles()` so both `percentiles` and the new
   `scoped_percentiles` are populated for all current data.

The percentile NOTIFY trigger fires on `UPDATE OF percentiles`, so the
`stats = stats` backfill does not spam pg_notify. The subsequent
`recalculate_percentiles()` call does update `percentiles` and will fire
the milestone trigger for stats that crossed thresholds — this is
intentional and expected behavior.

## Files changed

- `sql/shared.sql` — `scoped_percentiles JSONB` columns on `player_stats`
  and `team_stats`; `recalculate_percentiles()` extended.
- `sql/nba.sql` — 8 new stat definitions; player trigger refactored to
  loop; player + team views expose scoped fields.
- `sql/football.sql` — 31 new stat definitions; player trigger refactored
  to loop with legacy `shots_per_90` alias preserved; player + team views
  expose scoped fields.
- `sql/nfl.sql` — player + team views expose scoped fields (no new stats;
  NFL doesn't currently use per-game rate conversions in its trigger).
- `sql/migrations/012_per_rate_and_scoped_percentiles.sql` — atomic
  delta migration + backfill.
- `ENDPOINTS.md` — sample payloads updated with `scoped_percentiles` +
  `scoped_percentile_metadata` examples for both player and team.

## Verification (live, post-migration)

Migration applied to production via `psql "$DATABASE_PRIVATE_URL" -v
ON_ERROR_STOP=1 -f sql/migrations/012_per_rate_and_scoped_percentiles.sql`.
First attempt failed at the view CREATE step with `cannot change name
of view column "stats_updated_at" to "scoped_percentiles"` — Postgres'
`CREATE OR REPLACE VIEW` only permits appending columns at the end, not
inserting in the middle. Fixed by switching to `DROP VIEW IF EXISTS`
+ `CREATE VIEW` for the six profile views (verified no external
matviews/views depend on them via `pg_depend` first). Whole transaction
rolled back on the first failure, so no partial state landed. Second
attempt committed cleanly:

- `UPDATE 5194` rows in player_stats refired the derived-stats trigger
  for NBA + FOOTBALL; trigger emitted the new per-36 / per-90 keys.
- 7 distinct (sport, season) tuples processed by
  `recalculate_percentiles()`.

Live counts after commit:

- `stat_definitions`: NBA per_36 = 15 (7 original + 8 new), FOOTBALL
  per_90 = 38 (7 original + 31 new). ✓
- `player_stats`: 9,689 rows have populated `scoped_percentiles`. ✓
- `team_stats`: 282 rows (all teams) have populated `scoped_percentiles`. ✓
- NBA backfill: 1,723 of 1,785 player_stats rows have `fga_per_36`
  (the 62 deltas are zero-minute / no-FGA rows, expected). ✓
- Football backfill: 2,727 of 3,409 player_stats rows have
  `passes_total_per_90` (similar zero-data deltas). ✓

API surface verified live:

- NBA `/api/v1/nba/player/3` (Steven Adams, C, HOU): 35 per-rate stats
  in payload incl. new `oreb_per_36=7.1`, `dreb_per_36=6.5`,
  `fga_per_36=6.8`. Legacy `tov_per_36=1.7` preserved (not
  `turnover_per_36`). `percentile_metadata` shows 64-player sport-wide
  centers; `scoped_percentile_metadata` shows
  `{scope_type: conference, scope_id: West, sample_size: 32}`. Same
  player ranks 82.5th overall vs 80.6th conference-only on rebounds —
  scope shift visible.
- Football `/api/v1/football/player/<premier-league-mid>` (James
  Milner, Midfielder): 34 per_90 keys present incl. all new ones.
  Overall scope = 838 midfielders; scoped =
  `{scope_type: league, scope_name: Premier League, sample_size: 155}`.

## Result

Per-36 / per-90 coverage is now exhaustive for NBA and Football volume
stats. Every player profile carries two percentile views: the existing
sport-wide (position-partitioned) view and a new scope-narrow view
(position × conference for NBA/NFL, position × league for Football). Team
profiles get a parallel scope-narrow view (no position dimension).
Frontend gets two new fields per profile and chooses how to surface them.

## Follow-up note for next migrations

When a future migration adds JSONB / scalar columns to `player_stats` or
`team_stats` and surfaces them in the six profile views, prefer
`DROP VIEW IF EXISTS` + `CREATE VIEW` over `CREATE OR REPLACE VIEW`
unless the new columns are appended at the end of the view's column
list. The `stats_updated_at` field is canonically last in those views;
adding new columns above it requires the drop. The canonical sql files
under `sql/{nba,nfl,football}.sql` were updated to use the
DROP+CREATE pattern for `nba.player`, `nba.team`, `nfl.player`,
`nfl.team`, `football.player`, `football.team` so re-running the
canonical files against an existing DB doesn't re-trip this issue.
