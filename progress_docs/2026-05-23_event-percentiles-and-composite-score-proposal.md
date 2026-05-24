# Proposal — per-event percentiles + composite score per game

Date: 2026-05-23
Status: Proposal — not yet implemented.

## Context

Today's percentile pipeline (`recalculate_percentiles()` in
`sql/shared.sql:803`, called from `finalize_fixture()`) computes one
percentile per stat per entity per season, stored on `player_stats` /
`team_stats` as the `percentiles` JSONB column. That gives "where does
player X rank this season vs peers" but no "how did player X perform in
last night's game vs their own peer cohort."

The proposal: extend the same percent_rank pattern down to the EVENT
level. For every event row, compute a percentile per stat key against
the season's distribution of single-game values for the same partition,
then average those into a single `composite_score` per event. Both land
on the event row alongside the raw stats.

This gives:

- A **single-number rating per game**, the kind users instinctively look
  for ("what was player X's score last night?").
- **Trend signal as a 1D series** — `[78, 82, 71]` is way easier to
  reason about than a 50-key dictionary of per-stat averages.
- **Form-streak displays** computable client-side from the score array
  with no new endpoints.
- A coherent place to activate `stat_definitions.is_percentile_eligible`,
  which currently exists but isn't read by any function.

This sits parallel to the season-level percentile pipeline; the two
coexist and serve different consumers.

## Locked decisions

Captured during the proposal conversation:

| Decision | Choice | Rationale |
|---|---|---|
| Cohort for per-event percentile | Same-season events partitioned by (sport, season, position) for players; (sport, season) for teams | Matches the existing season-percentile pattern. Stable, well-defined, lots of data per partition. Accepts that an early-season game's score drifts slightly as more events are added. |
| Composite score formula | Unweighted mean of per-stat percentiles, across keys flagged `is_percentile_eligible = true` where the player has a non-zero value | Simple, transparent, fully data-driven. Activates the long-vestigial `is_percentile_eligible` flag. Inverse stats are already handled at percentile time (`is_inverse` → `1 − percent_rank()`), so high score always = good. |
| Stat filter | `stat_definitions.is_percentile_eligible = true` | Currently unused. This is what the flag is for. Backfill it per sport with sensible defaults. |
| Storage shape | New `percentiles JSONB` + `composite_score NUMERIC` columns on `event_box_scores` and `event_team_stats` | Mirrors the existing season-row pattern. Per-stat percentiles preserved per event (~400 bytes per row) so the composite can be re-weighted later without re-querying cohort. |
| Football low-minute appearances | Treat the same as any other event; no minimum-minutes filter. Frontend gets `minutes_played` alongside each composite score so it can render a "limited sample size" disclaimer for short appearances. | One of the values of per-90 data is illuminating players who deserve more minutes — a sub with a strong per-90 game IS signal, not noise to throw away. |

## Design

### Storage

```
event_box_scores                event_team_stats
├── stats: jsonb (raw)          ├── stats: jsonb (raw)
├── percentiles: jsonb    NEW   ├── percentiles: jsonb    NEW
└── composite_score: numeric NEW └── composite_score: numeric NEW
```

`percentiles` shape mirrors the existing `player_stats.percentiles` /
`team_stats.percentiles`:

```json
{
  "pts": 88.2,
  "reb": 67.1,
  "ast": 92.8,
  "...": "...",
  "_position_group": "PG",
  "_sample_size": 5234
}
```

`composite_score` is a single numeric in [0, 100], computed as the mean
of the per-stat percentile values for keys where the player had a
non-zero raw value AND the stat is flagged `is_percentile_eligible`.
NULL when the event has no eligible non-zero stats (e.g. a player with
`{}` stats, or a football appearance where every relevant stat is zero).

### Compute hook

Extends `finalize_fixture()`:

```
finalize_fixture(fixture_id)
  → aggregate season blobs (existing)
  → recalculate_percentiles(sport, season) (existing, season-level)
  → recalculate_event_percentiles(sport, season) (NEW, event-level)
  → mark_fixture_seeded (existing)
```

`recalculate_event_percentiles(p_sport, p_season)` is a new SQL function
that mirrors `recalculate_percentiles` but reads from `event_box_scores`
/ `event_team_stats` instead of `player_stats` / `team_stats`. Per-fixture
trigger is intentional: every finalize re-ranks the SEASON's events.
Yes, that's more work than today; see "Compute cost" risk below for the
measurement and optimization path.

Sketch (real SQL lives in `sql/shared.sql` next to `recalculate_percentiles`):

```sql
-- For each event row in (p_sport, p_season):
--   For each numeric stat in the row's stats blob where the stat is
--     percentile-eligible AND value != 0:
--       Compute percent_rank() over PARTITION BY (position, stat_key)
--       Invert if is_inverse=true (high values = bad)
--       Apply per-sport value transform (see below)
--   Write {stat_key: percentile, ...} to event.percentiles
--   Write AVG(percentile values) to event.composite_score
```

### Per-sport value transform inside the percentile function

The thing being percentiled has to be in the same unit across the cohort:

| Sport / entity | Value the cohort ranks | Why |
|---|---|---|
| NBA player | Raw event value | Single-game raw counts compare directly across players in same position. |
| NBA team | Raw event value | Same. |
| NFL player | Raw event value | 1 event = 1 game by definition; raw count is per-game. |
| NFL team | Raw event value | Same. |
| Football player | `raw * 90 / minutes_played` for `cumulative_total` keys; raw for `rate_pct` and `per_game_avg` keys | Variable minutes per match means rate-of-play is the fair comparison. Per-90 normalization done inline in SQL — no schema change needed. |
| Football team | Raw event value | A team plays the full match by definition; no minutes variance. |

The football per-90 transform is the same SQL expression used in the
trends entity-recent CTE (per the earlier event-derivation proposal,
Change A). Carrying the same expression in two SQL consumers is mild
duplication; see "Resurrects the trigger question" below.

### Sample size disclaimer flow

`event_box_scores.minutes_played` is already a top-level column (used by
`aggregate_player_season` for the `minutes_played > 0` filter). The
trends endpoint and any consumer surfacing composite scores should emit
it alongside each score so the frontend can render a disclaimer:

```json
{
  "entity_recent_scores": [
    {"composite_score": 78, "minutes_played": 38},
    {"composite_score": 82, "minutes_played": 90},
    {"composite_score": 71, "minutes_played": 12}   // → frontend renders "small sample" badge
  ]
}
```

Threshold for the badge is a frontend display choice (e.g. < 20 mins for
football, < 5 mins for NBA). The backend just emits the data.

## Phased rollout

### Phase 1 — schema + function + backfill (one migration)

Migration `017_event_percentiles_and_composite.sql`:

- `ALTER TABLE event_box_scores ADD COLUMN percentiles JSONB DEFAULT '{}'::jsonb, ADD COLUMN composite_score NUMERIC;`
- `ALTER TABLE event_team_stats ADD COLUMN percentiles JSONB DEFAULT '{}'::jsonb, ADD COLUMN composite_score NUMERIC;`
- Backfill `stat_definitions.is_percentile_eligible` per sport. Rule-based
  default: `is_percentile_eligible = (comparable = true AND unit IN ('per_game_avg', 'rate_pct'))`,
  with a small special-case list to exclude things like `games_played`,
  `minutes_played`, jersey-ish keys. The data-driven derivation here is
  intentional — anything that's a clean per-game / rate stat is fair game;
  cumulative totals already have per-game siblings that carry the percentile.
- Create `recalculate_event_percentiles(p_sport, p_season)` function.
- Hook into `finalize_fixture()` immediately after `recalculate_percentiles()`.
- One-time chunked backfill: for each (sport, season) that has events,
  call `recalculate_event_percentiles()` once. Estimated runtime needs
  measurement on a copy.
- Index for the trends consumer:
  `CREATE INDEX idx_event_box_scores_composite_recent ON event_box_scores(sport, player_id, fixture_id DESC) INCLUDE (composite_score, minutes_played);`
  matches the "last-N composite scores for player X" access pattern.

### Phase 2 — surface in API (Go-only, no migration)

Add fields across three endpoints:

**Trends endpoint** (most valuable consumer):
```json
{
  "...": "...",
  "entity_recent_scores": [
    {"composite_score": 78, "minutes_played": 38, "fixture_id": 9912},
    {"composite_score": 82, "minutes_played": 90, "fixture_id": 9905},
    {"composite_score": 71, "minutes_played": 12, "fixture_id": 9897}
  ],
  "entity_season_score_avg": 75.3,
  "peer_season_score_avg":   50.0
}
```

The peer-side score is by definition centered around 50 within a position
cohort (it's the average of percentile values, which average to 50). It's
mostly there as a sanity anchor for the frontend; could be omitted in v1.

**Profile endpoint**:
```json
{
  "meta": {
    "...": "...",
    "season_composite_score": 75.3
  }
}
```

Single number for the entity headline. Computed as the season-mean of
event composite scores (stored back into `player_stats` / `team_stats` as
a top-level numeric column via the same aggregate function — cleaner than
recomputing on read).

**Team results endpoint** (we just shipped this):
```json
{
  "results": [
    {"fixture_id": 9912, "team_score": 112, "opponent_score": 108, "composite_score": 78, "...": "..."}
  ]
}
```

One field, big readability win.

### Phase 3 — frontend reaps (no backend work)

- TrendsCard: single-number score sparkline above the per-stat rows.
- Profile header: composite score as the at-a-glance rating.
- Form streak: "5 games above 70" computed client-side from the score
  array.
- Team results page: one more column.

## Risks and tradeoffs

### Compute cost on every finalize

The new function re-ranks all events for the (sport, season) on every
finalize_fixture call. Today's `recalculate_percentiles` runs against
season-rolled tables (hundreds to low-thousands of rows per sport-season);
events are 50–100× larger.

Rough estimate per finalize:
- NBA: ~25 player events/game × 82 games × 30 teams = ~60K player events
  per season. Partition by position (~10) gives 6K rows per partition.
  Per-stat × 50 stats = 300K cells to rank. `percent_rank()` is window
  function — fast in Postgres. Estimated: a few seconds.
- NFL: similar order.
- Football: largest by row count (multiple leagues × longer seasons).

Acceptable for finalize_fixture (which is already a heavy operation). If
measurement shows it's too slow, optimization paths:

- Compute only the new fixture's events' percentiles against a cached
  cohort distribution snapshot. The cohort snapshot updates less often
  (nightly batch).
- Recompute the full season's events on a nightly cron instead of per
  finalize.
- Materialize percentile boundaries (a table of "the 90th-percentile
  value for (sport, season, position, stat)") and lookup-vs-compare
  per row.

Start simple; measure; optimize if needed.

### Storage cost

`percentiles` JSONB ≈ 400 bytes per row × estimated 300K event rows across
all sports/seasons → ~120 MB. `composite_score` NUMERIC ≈ 8 bytes per row
→ ~2.4 MB. Both fine for Postgres.

### Score interpretability

"73 composite" needs to be obvious. Documentation needs to spell out:
"average is 50 by construction (it's a percentile-of-percentiles); 70+ is
strong; 85+ is elite." Frontend tier badges (already part of the design
system) can reinforce. Worth documenting in ENDPOINTS.md prominently.

### Specialists look mediocre

By design: a 3-point shooter who's 99th-percentile in `fg3_pct` but 30th
in everything else scores around 55. Counterintuitive but defensible.
The per-stat percentiles are still surfaced for the breakdown view, and
the composite scoring system is up-front about being "well-roundedness
weighted." Documenting this is more important than fixing it; we
explicitly chose unweighted mean for transparency.

### Small-position-cohort noise

NFL kickers (~32 of them), some football specialist positions. Percent
ranks over N=30 are noisy. Same issue as today's season percentiles,
not a new failure mode. The `_sample_size` metadata key in the
percentiles JSONB makes this explicit to consumers.

### Historical drift

Same-season cohort means an October game's composite score in October ≠
its score in May. We accept this for consistency with the season-percentile
pattern. The trends card's last-3-events view always uses current scores,
so the drift is invisible there. If a consumer wants frozen historical
scores, that's a follow-up (snapshot a per-day cohort distribution).

### Resurrects the trigger question from the earlier event-derivation proposal

The football per-90 normalization inside `recalculate_event_percentiles`
uses the same SQL expression as the trends entity-recent CTE (per Change
A in `2026-05-23_event-derivation-proposal.md`). Two consumers carrying
the same 4-line CASE expression is acceptable duplication; if a third
consumer appears, the math should move into a helper SQL function — or
move further upstream into a BEFORE trigger that materializes `*_per_90`
keys on event rows (the Change-B option I previously argued was overkill
when trends was the only consumer).

Recommendation: ship Phase 1 with the inline CASE expression. Revisit
whether to land Change B if/when a third consumer needs the same
normalization.

## Open considerations not blocking Phase 1

- **Season composite score on `player_stats` / `team_stats`.** Phase 2's
  profile-endpoint addition implies a `season_composite_score` numeric
  column on the season tables, populated by extending the
  `aggregate_*_season` functions to AVG over event composite scores. This
  is a small additional surface area in Phase 1 — easier to ship together
  than to add the column later.
- **The peer cohort for "is this team's recent composite score above
  league?"** is already implicit in the data (every team's events are
  in the same partition). The trends endpoint can surface
  `peer_season_score_avg` = AVG over all peer events' composite scores.
  By construction this should hover around 50; it's mostly an anchor for
  the frontend to render against rather than a meaningful comparison
  point. Could be omitted in v1.
- **The composite score as a discovery signal.** The vibe pipeline ranks
  entities by sentiment trend; composite score trend is another
  discovery axis (e.g. "hottest entities by composite trajectory this
  week"). Out of scope for Phase 1 but worth keeping in mind when
  designing the data shape.

## What this proposal explicitly does NOT do

- **No new tables.** Two new columns on existing event tables; that's it
  for schema. No `event_percentile_snapshots` history table or similar —
  recompute is cheap enough at season scale that snapshotting is
  premature.
- **No weighted composite formula.** Unweighted mean is the decision.
  If experience shows this needs revisiting, the `percentiles` JSONB is
  preserved per event so re-weighting doesn't require a cohort recompute.
- **No minimum-minutes filter for football.** Treat short appearances
  the same as any other event; surface `minutes_played` alongside each
  score so the frontend can disclaim. Short appearances with strong
  per-90 numbers are signal, not noise.
- **No touching the Python seeder.** A/B/C architecture preserved.
- **No premature optimization of the recompute cost.** Ship the
  straightforward implementation; measure; optimize if needed.

## Quick reference

| Item | Path |
|---|---|
| Existing season-percentile function (model) | `sql/shared.sql:803` (`recalculate_percentiles`) |
| Existing finalize_fixture hook | `sql/shared.sql:630` |
| Existing stat metadata table | `sql/shared.sql:198` (`stat_definitions`) |
| Related — event-derivation proposal | `progress_docs/2026-05-23_event-derivation-proposal.md` |
| Related — trends comparability work | `progress_docs/2026-05-23_trends-unit-comparability.md` |
