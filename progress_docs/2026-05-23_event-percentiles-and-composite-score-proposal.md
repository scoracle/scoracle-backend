# Proposal — per-event percentiles + composite score per game

Date: 2026-05-23 (initial) / 2026-05-24 (normalization)
Status: **Phase 1 + Phase 2 + normalization shipped.** Schema, function,
finalize_fixture hook, full backfill, API surface across trends +
profile + team results, AND a two-pass normalization (migration 018)
so composite_score now has mean=50 per partition with uniform
distribution in `[0, 100]`. Phase 3 (frontend) is purely client-side
from here; all needed data flows through the three existing endpoints.
Most outlier concerns are now bounded by normalization; the
remaining sparse-stat case is documented at the bottom of this doc
for a future session.

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
| Phase 1 migration | `sql/migrations/017_event_percentiles_and_composite.sql` |
| Normalization migration | `sql/migrations/018_event_composite_normalization.sql` |
| Related — event-derivation proposal | `progress_docs/2026-05-23_event-derivation-proposal.md` |
| Related — trends comparability work | `progress_docs/2026-05-23_trends-unit-comparability.md` |

## Phase 1 shipped — what's live now

Migration 017 applied to production on 2026-05-23. Backfill ran in
**91 seconds** across all (sport, season) pairs — well under the
"need to chunk this" threshold flagged in the proposal.

### Coverage after backfill

| Table | Sport | Rows | Scored | Avg composite |
|---|---|---|---|---|
| event_box_scores | FOOTBALL | 134,482 | 98,193 | 49.0 |
| event_box_scores | NBA | 213,895 | 138,322 | 42.1 |
| event_box_scores | NFL | 89,816 | 89,157 | 35.9 |
| event_team_stats | FOOTBALL | 6,352 | 6,352 | 47.2 |
| event_team_stats | NBA | 13,070 | 13,070 | 48.3 |
| event_team_stats | NFL | 2,848 | 2,848 | 44.6 |

The gap between event_box_scores totals and "scored" counts is rows
with no eligible non-zero stats (e.g. a player listed in the lineup
but with an empty stats blob, or a DNP-CD). Team events score at
100% because every team event has a full stat slate.

The sub-50 averages for player events reflect the long tail of
low-minute / sparse-stat events; the same composite scoring system
that ranks Anthony Edwards at 64.8 for a 22-point game ranks bench
appearances in the 20s. Aggregate distribution by playing-time band
confirms this is healthy:

| NBA minutes band | Events | Avg composite |
|---|---|---|
| 0-10 min | 20,345 | 28.2 |
| 10-25 min | 53,156 | 36.0 |
| 25-35 min | 45,393 | 48.5 |
| 35+ min | 21,550 | 56.6 |

Clean monotonic relationship: more minutes → more stats → composite
trends toward 50+.

### Top-of-leaderboard sanity check (NBA 2025 season)

```
Nikola Jokic              65.6   (27.6 pts avg)
Jayson Tatum              64.2   (21.8 pts avg)
Luka Doncic               64.2   (33.5 pts avg)
Kawhi Leonard             64.0   (27.8 pts avg)
Shai Gilgeous-Alexander   62.9   (31.1 pts avg)
Lauri Markkanen           61.6   (26.7 pts avg)
Jalen Duren               61.1   (19.5 pts avg)
Giannis Antetokounmpo     61.1   (27.6 pts avg)
```

Exactly who you'd expect. Jokic edges Luka despite scoring 6 fewer
PPG because his percentile profile is more balanced (rebounds,
assists, efficiency).

### Per-90 normalization confirmed working

Spot-checked Arthur Vermeeren's 5-minute appearance (13 passes, 5
backward passes, 13 touches):
- 13 passes × 90/5 = 234 passes/90 → 99.2 percentile among midfielder events
- Composite for the event = 99.2

This is the "illuminates underused players" framing the user
explicitly asked for. The frontend will need to surface
`minutes_played` alongside the score so users see "(5 min)" next to
the 99.2 rather than reading it as a 90-minute dominant performance.

### Edge case noted: few-ranked-stats events produce extreme composites

A goalkeeper event where the only ranked stat is `error_lead_to_goal`
(an inverse stat, so "low value = high percentile") scores 100 if
the GK had the fewest errors in the cohort. Same family of issue as
the low-minute appearance — sparse data → extreme composites, by
design. Worth surfacing a per-event "stats_contributed" count to
the metadata in a future refinement so the frontend can disclaim
single-stat composites the same way it'll disclaim short appearances.
Not blocking Phase 2.

### Deviation from the proposal — `aggregate_*_season` not modified

The proposal suggested extending `aggregate_*_season` functions to
include `season_composite_score`. In implementation it was cleaner
to write the rollup as a final UPDATE step inside
`recalculate_event_percentiles` itself, because the season composite
depends on event composite scores — which don't exist until after
the percentile recompute runs. The aggregate functions stay
unmodified; one fewer file touched.

### `is_percentile_eligible` activation

The flag had existed since migration 008 with no reader. Migration
017 backfills it via this rule:

```sql
is_percentile_eligible = (
    unit IN ('per_game_avg', 'rate_pct', 'cumulative_total')
    AND key_name NOT IN (games_played, matches_played, lineups,
                         minutes_played, minutes, gp,
                         cumulative_minutes_played,
                         captain, substitutions, rating)
)
```

`rating` is excluded because SportMonks already provides it as a
composite score per fixture — including it would double-count
itself in the new composite.

### Phase 2 and 3 — next steps

Both are now unblocked:

- **Phase 2 (API surface)**: extend the trends endpoint with
  `entity_recent_scores` (last-3 composite scores + minutes_played
  per entry), `entity_season_score_avg`, `peer_season_score_avg`.
  Extend the profile payload with `season_composite_score`. Extend
  the team-results endpoint with `composite_score` per result row.
- **Phase 3 (frontend)**: composite sparkline above TrendsCard's
  per-stat rows; profile headline rating; form-streak rendering
  computed client-side from the score array.

No further migrations needed; everything Phase 2 needs is already
populated in the DB.

## Phase 2 shipped — 2026-05-23

API surface now exposes the Phase 1 data across three endpoints. No
migration; pure-additive Go changes in `go/internal/db/db.go`:

- **Trends** (`/api/v1/{sport}/{entityType}/{id}/trends`) gains:
  - `entity_recent_scores: [{fixture_id, composite_score, minutes_played}, ...]`
    — last 3 events, with `minutes_played` for sample-size disclaimers.
  - `entity_season_score_avg` — entity's own season composite.
  - `peer_season_score_avg` — AVG of peer cohort season composites.
- **Profile** (`/api/v1/{sport}/{entityType}/{id}`) gains
  `meta.season_composite_score` — the headline rating.
- **Team results** (`/api/v1/{sport}/team/{id}/results`) gains
  `composite_score` per row.

Live verified end-to-end:

```
Spurs trends: season=51.1, peer=46.6, recent=[40.0, 41.7, 53.9]
Jokic trends: season=65.6, peer=39.2 (Center cohort), recent=[54.4 (40m), 56.7 (18m), null (DNP)]
Jokic profile: meta.season_composite_score=65.6 ✓ matches trends
Spurs results: composite scores 40-56 per fixture, surfaced inline
```

Implementation deviation: the proposal suggested separate
`entity_recent_scores` / `entity_season_score_avg` /
`peer_season_score_avg` fields rather than nesting under a `scores`
block. Kept flat for consistency with the existing
`entity_recent_avgs` / `entity_season_avgs` / `peer_season_avgs`
triplet. Trivially regroupable later if a `scores` block is preferred.

## Deferred to a follow-up session — outlier handling

Phase 1 exposed two edge cases worth dedicated work but explicitly
out-of-scope for the initial ship:

1. **Few-ranked-stats events produce extreme composites.** A
   goalkeeper event where the ONLY ranked stat is `error_lead_to_goal`
   (an inverse stat) scores 100 because that one stat ranked high.
   Same family as the low-minute appearance issue but rooted in
   sparse stat presence rather than sparse minutes. Today the
   `_sample_size` metadata key tells consumers the cohort size for
   the most-ranked stat in that event — it does NOT tell them how
   many stats contributed to the composite. A future refinement
   could:
   - Add `stats_contributed` to each event's percentiles metadata.
   - Optionally clamp composite to NULL when `stats_contributed < N`
     (e.g. < 3 stats), letting the frontend distinguish "this
     player legitimately scored 80" from "this player only had
     one ranked stat so the math degenerates."
   - Or weight the composite toward stats that have multi-row cohort
     support, reducing the impact of degenerate single-stat games.

2. **Low-minute football appearances producing per-90 outliers.**
   A 5-minute sub with 13 passes → 234 passes/90 → 99th percentile.
   This is by design (per-90 illuminates underused players, per
   the user's explicit framing) but consumers will want the
   `minutes_played` already surfaced to render a disclaimer rather
   than show the raw 99 as a normal score. Frontend-side display
   call; backend currently surfaces the data the disclaimer needs.

Both should be revisited together once the frontend has been built
out to consume composite scores and we see how the edge cases read
in practice. The fixes are non-breaking refinements; nothing in
Phase 1 or 2 needs to change to ship them later.

## Normalization shipped — 2026-05-24 (migration 018)

The Phase 1 + 2 ship exposed a real distributional issue: per-event
composite means were ~42 (NBA), ~36 (NFL), ~49 (football) instead of
the expected 50. Mechanism: composite is AVG of per-stat percentiles
across stats with non-zero values; stat presence correlates with stat
quality (bench players have few stats AND those stats are low-
percentile; stars have many AND they're high), so the population mean
drifts below 50.

Fix: migration 018 replaces `recalculate_event_percentiles` with a
two-pass version. Pass 1 is the original unweighted-mean (now called
`raw_composite` inside the function). Pass 2 percent-ranks the raw
composite within the same (sport, season, position) partition for
players, (sport, season) for teams. The stored `composite_score`
becomes the result of pass 2.

This delivers:

- **Mean per partition = 50 by construction.** Verified post-migration:
  NBA player events 49.91, NFL 47.56, FOOTBALL 49.88; all team event
  averages 49.8-49.9. Standard deviations ~28.9 — the textbook value
  for a uniform distribution on `[0, 100]` is `100/√12 ≈ 28.87`.
- **Self-bounded outliers in `[0, 100]`.** The previous degenerate
  case (goalkeeper event with one inverse stat scoring 100) now
  ranks against other sparse-stat events — usually clustering at
  similar values rather than uniquely pinning to 100.
- **No schema change, same function signature, same call site.**
  `finalize_fixture` continues to work unchanged. API surface
  unchanged — clients still see a number in `[0, 100]`, and the
  "higher = better" property is preserved.
- **Interpretation shift, documented:** the stored composite is no
  longer "average of stat-percentiles" — it's now "percentile rank
  of the event's raw composite within its position cohort." Arguably
  more intuitive for users ("ranks in the 70th percentile of NBA
  Center events" vs. "scored a 65 on a homemade index"). Tier
  thresholds (50/60/80) actually mean what they say now.

Post-normalization sanity check:

```
NBA top season composites:
  Nikola Jokic              90.5  (C)
  Jayson Tatum              90.2  (F)
  Kawhi Leonard             88.0  (F)
  Shai Gilgeous-Alexander   87.9  (G)
  Lauri Markkanen           86.7  (F)
  Luka Doncic               86.2  (F-G)
  Kevin Durant              85.5  (F)
  Giannis Antetokounmpo     84.6  (F)
  James Harden              84.6  (G)
  Tyrese Maxey              84.5  (G)
```

The right names cluster in the 85-90 band — exactly what a uniform
distribution under "season composite = AVG of percentile-ranked event
composites" would predict for genuinely elite players.

Backfill ran in ~50 seconds (slightly faster than 017's 91 seconds
because the per-stat percentile pass is the dominant cost; pass 2 is
a single percent_rank over the small per-event table). One transient
deadlock with the live football seeder during the inline backfill
loop — resolved by running the per-season backfill via a small bash
retry wrapper. Worth noting for any future bulk-recompute that
contends with active writes.

### Deferred (still open): football player per-90 inflation breaks the leaderboard

Confirmed 2026-05-24 with two top attackers (Harry Kane #997 at 53.3,
João Pedro #28931574 at 33.3). Both are 76-min-average starters who
should rank in the 80-90 band like NBA stars. They don't because the
event cohort is dominated by low-minute appearances whose per-90
extrapolations look elite. Minute-band distribution makes the
mechanism unmistakable:

```
< 15 min : 2,780 events, avg composite 75.2
15-30 min: 3,057 events, avg composite 65.0
30-60 min: 2,265 events, avg composite 51.3
60-80 min: 3,733 events, avg composite 40.0
80+ min  : 4,722 events, avg composite 32.8
```

Almost a perfect inverse correlation between minutes and composite — a
5-min sub with one stat extrapolates to elite per-90 numbers, then
percent-ranks at the top of the Attacker cohort. Real starters get
pushed into the bottom third.

This is the same family as the goalkeeper-100 case (sparse data →
extreme score), but for football PLAYERS it has structural rather
than edge-case impact because the per-90 transform actively amplifies
low-minute events. Teams aren't affected (every team event is 90
mins, no minutes variance). NBA/NFL players aren't affected (1 event
= 1 game, no per-minute extrapolation).

Two fix paths to weigh next session:

1. **Simpler:** drop per-90 normalization at event ranking for football
   players. Rank raw cumulative values (same as the other sports).
   Loses "per-90 illuminates underused" framing at the event level
   but preserves it at season aggregation (where `*_per_90` keys
   already live in `player_stats`). Plus minutes-weighted season
   composite rollup: `SUM(composite * minutes) / SUM(minutes)` so a
   30-event starter outweighs a 1-event sub at the season-summary
   level.
2. **More principled:** filter the per-event cohort to events with
   `minutes_played >= 30` (or some threshold). Bench appearances
   either get NULL composite or score against a separate sub
   cohort. Plus the same minutes-weighted season rollup.

Plus a separate bug surfaced: Papa Dame Ba shows event composite 99.9
with stats `{}` and percentiles `{}` — either NULL raw composites are
getting ranked at the top of percent_rank's NULL-handling, or stale
composite from an earlier state never got cleared when his stats were
emptied. Worth tracking down independent of the per-90 fix.

### Deferred (still open): few-ranked-stats outlier

The earlier "goalkeeper event scoring 100 because one inverse stat
ranked high" case is improved by normalization (such events now
rank against other sparse-stat events rather than uniquely
saturating the high end), but not fully solved. A goalkeeper with
chronically sparse stats can still rank in the high percentiles
within the goalkeeper cohort. The proper fix — Bayesian shrinkage
by participating-stat count, or a `stats_contributed` metadata
field for the frontend to disclaim — remains in the "deferred"
section above. Normalization made it less acute; we can decide
whether to layer the more-principled fix on top once the frontend
is consuming composites and we see how it reads in practice.

## Full-season sparkline data — 2026-05-24

Frontend feedback after the initial composite ship: a 3-dot sparkline
from `entity_recent_scores` reads as "last 3 dots" rather than
"season form shape." Extended the field to cover every played event
in the current season:

- **Renamed `entity_recent_scores` → `entity_event_scores`** (no
  external consumer was attached yet — frontend integration landed
  same-day; better to fix the name before it ossifies).
- **No more `LIMIT 3`** — full-season coverage: NBA ~82, NFL ~17,
  football ~38 per response.
- **Added `start_time` per row** so the frontend can label
  hover-tooltips and bucket by week/month without a second fetch.
  Fixtures table already joined; cheap addition.
- **Scope: current season only.** The prior-season bridge logic
  (used by `entity_recent_avgs` when current season is sparse)
  doesn't apply here — once a team has any current-season
  data, the sparkline shows that season's shape; off-season requests
  return `[]`.

Implementation: two new CTEs in `trendsStatement`
(`player_season_events`, `team_season_events` unioned into
`entity_season_events`) scoped to current season with no row limit.
Existing limit-3-with-bridge CTEs (`player_events` / `team_events`
/ `entity_events`) stay intact for `entity_recent_avgs` and the
`window` metadata — the two consumers are now cleanly separated.

Pure SQL composition; no schema change. Cache TTL on `/trends`
unchanged. Live verification: Jokic returns 83 events for the 2025
NBA season (regular season + early-spring playoffs); Spurs returns
37 events for the PL season.
