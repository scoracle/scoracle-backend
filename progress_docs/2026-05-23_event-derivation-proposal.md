# Proposal — event-level derivation triggers (parallel to season-level)

Date: 2026-05-23
Status: Proposal — not yet implemented.

## Context

Two known limitations from the trends comparability work
(`2026-05-23_trends-unit-comparability.md`) point at the same gap in the
derivation pipeline:

1. **Entity-side rate_pct values are unreliable for NBA & NFL.** The trends
   endpoint currently drops them entirely. NBA team `event_team_stats` rate
   keys (`fg_pct`, `ft_pct`, `fg3_pct`) are written by the seeder as the SUM
   of player-row fractions, so a team's per-game `fg_pct` shows up as ~4.0
   instead of ~47. SportMonks per-fixture rate keys for football
   (`tackles_won_percentage`, `aerials_won_percentage`, etc.) are non-normalized
   provider aggregates that can read as 700 for a single match. Whenever a
   raw numerator and denominator are also in the row, the correct rate can be
   derived from them.

2. **Player trends on NFL & football show few or zero comparable keys.** The
   key namespace differs across tables: `event_box_scores.stats` carries
   raw per-fixture counts (`tackles`, `passing_yards`) while
   `player_stats.stats` carries the derived per-game / per-90 siblings
   (`tackles_per_90`, `passing_yards_per_game`). The trends SQL intersects
   the two — they never line up, so the intersection is empty.

Both can be fixed by the same architectural move: **extend the existing
"BEFORE trigger that enriches the JSONB blob on upsert" pattern down one
level**, from season-rolled tables (`player_stats` / `team_stats`) to event
tables (`event_box_scores` / `event_team_stats`).

## Where the existing pattern lives

Today, derivation flows like this on every fixture finalize
(`finalize_fixture()` in `sql/shared.sql:630`):

1. Python seeder upserts raw provider data into `event_box_scores` /
   `event_team_stats`. **Event rows stay raw.**
2. Seeder calls `finalize_fixture(p_fixture_id)`.
3. `finalize_fixture` calls the sport-specific
   `aggregate_player_season(player_id, season, league_id)` /
   `aggregate_team_season(team_id, season, league_id)` functions
   (`sql/nba.sql:475`, `sql/nfl.sql:550`, `sql/football.sql:591`, plus team
   variants). These read all matching events and return a season JSONB.
4. The aggregate output is UPSERTed into `player_stats` / `team_stats`.
5. **BEFORE trigger** `compute_derived_*_stats()` fires on the upsert
   (`sql/nba.sql:95/161`, `sql/nfl.sql:225/283`, `sql/football.sql:242`).
   It loops over raw stat keys and injects `_per_36` / `_per_game` /
   `_per_90` derived siblings + composite ratios into the same JSONB.
6. `recalculate_percentiles()` writes the `percentiles` /
   `scoped_percentiles` JSONB columns.

The thing to notice: **the pattern only runs at the season-row level**. Event
rows are written by the seeder and then read by aggregate functions, but
nothing derives back into them. That's the gap this proposal closes.

## Proposed architecture

Add a new BEFORE INSERT/UPDATE trigger family —
`enrich_event_*_stats()` — on `event_box_scores` and `event_team_stats`,
one function per (sport, entity_kind). The trigger:

- Reads the raw values that are present on the same `NEW.stats` blob.
- Recomputes rate keys from the underlying numerators / denominators
  (fixes follow-up #1).
- Writes per-game / per-90 siblings for raw counts that have a derived
  cousin in `stat_definitions` (fixes follow-up #2).
- Merges the new keys back into `NEW.stats` and returns NEW.

This sits exactly parallel to the existing `compute_derived_*_stats()`
trigger on `player_stats` / `team_stats`. Same file ownership
(per-sport SQL files), same naming convention, same enrich-the-JSONB
pattern. Python stays thin. Postgres remains the derivation engine. The Go
trends statement gets simpler — it can stop carrying sport-specific
entity-side rate_pct guards.

```
Python seeder  →  raw upsert  →  event_box_scores / event_team_stats
                                       │
                                       ↓
                          enrich_event_*_stats() trigger  (NEW)
                          – recompute broken rate keys
                          – emit per-game / per-90 siblings
                                       │
                                       ↓
                          finalize_fixture()  (unchanged)
                                       │
                                       ↓
                          aggregate_*_season()  (unchanged)
                                       │
                                       ↓
                          player_stats / team_stats upsert
                                       │
                                       ↓
                          compute_derived_*_stats() trigger  (unchanged)
                                       │
                                       ↓
                          recalculate_percentiles()  (unchanged)
```

The Go trends endpoint then reads event rows that already carry the keys it
wants to compare, in the same units as the season-rolled side.

## Concrete derivations per (sport, entity)

### NBA team event rates — fixes #1

Today `event_team_stats.stats` for NBA holds `{fg_pct: 4.0, fg_pct: ..., fgm:
45, fga: 95}` because the seeder sums player fractions. The trigger
recomputes:

```sql
fg_pct  = 100 * (NEW.stats->>'fgm')::numeric  / NULLIF((NEW.stats->>'fga')::numeric,  0)
fg3_pct = 100 * (NEW.stats->>'fg3m')::numeric / NULLIF((NEW.stats->>'fg3a')::numeric, 0)
ft_pct  = 100 * (NEW.stats->>'ftm')::numeric  / NULLIF((NEW.stats->>'fta')::numeric,  0)
-- plus existing derived composites (efg_pct, true_shooting_pct) computed
-- from the same raw counts so all four sports agree on the 0..100 scale
```

(Today's NBA `*_pct` lives on a 0..100 scale in `team_stats`. The trigger
emits the event row in the same scale, removing the
"event=fraction-in-0..1, season=percentage-in-0..100" split that surfaced
during this session.)

### Football team event rates — fixes #1

For SportMonks per-fixture stats, recompute the broken `*_percentage` keys
from raw inputs in the same row:

```sql
tackles_won_percentage = 100 * tackles_won / NULLIF(tackles_won + tackles_lost, 0)
duels_won_percentage   = 100 * duels_won   / NULLIF(total_duels, 0)
aerials_won_percentage = 100 * aerials_won / NULLIF(aerials_total, 0)
```

(Names line up with what the season-rolled `team_stats` already produces,
so the trends SQL intersection matches.)

The keys SportMonks gets right (`pass_accuracy`, `possession_pct`,
`shot_accuracy`, `cross_accuracy`, `long_ball_accuracy`,
`dribble_success_rate`) — confirmed in this session's spot-check that all
sit in `[0, 100]` per fixture — pass through unchanged.

### NFL player per-game siblings — fixes #2

Every NFL player event row is one game by definition, so the per-game
sibling is the value itself:

```sql
-- For every stat_definitions row where (sport='NFL', entity_type='player',
-- key_name LIKE base, derived sibling exists as <base>_per_game):
emit  '<base>_per_game' := value
```

Implementation note: rather than enumerate, the trigger can introspect
`stat_definitions` once per sport at function-define time (Postgres allows
a function body to read a catalog) — or simpler, iterate the
`stat_definitions` rows at trigger time, which costs ~50 rows per insert
(negligible).

### Football player per-90 siblings — fixes #2

Football events carry variable `minutes_played` per row, so the trigger
normalizes:

```sql
emit  '<base>_per_90' := (NEW.stats->>base)::numeric * 90.0
                          / NULLIF((NEW.stats->>'minutes_played')::numeric, 0)
```

`minutes_played` is verified present in football event_box_scores (it was
in the recon sample for this session). Players with `minutes_played = 0`
(unused subs) emit NULL for derived keys — the AVG in the trends CTE
ignores them, which is the right behavior.

### NBA — nothing needed

NBA's `event_box_scores.stats` already matches the unit convention used in
`player_stats.stats` for non-rate keys (raw single-game values average
cleanly against per-game season averages). NBA event-row fixes are limited
to the team rate-key recomputation in follow-up #1.

## Migration plan

The work splits into three concrete migrations and a follow-up trends
simplification. They can ship sequentially — each independently improves
the surface area without breaking the next.

### Migration 017 — `event_enrichment_triggers.sql`

- Create per-sport functions: `nba.enrich_event_team_stats()`,
  `football.enrich_event_team_stats()`, `nfl.enrich_event_player_stats()`,
  `football.enrich_event_player_stats()`.
- Attach BEFORE INSERT OR UPDATE triggers on `event_box_scores` and
  `event_team_stats`, filtered by `WHEN (NEW.sport = '<SPORT>')` so each
  function only fires for its sport.
- Backfill existing rows in the same migration:
  `UPDATE event_box_scores SET stats = stats WHERE sport IN ('NFL','FOOTBALL');`
  (the no-op update re-fires the trigger and enriches in place.) Same for
  `event_team_stats` for NBA + football.
- Log per-sport row counts via `RAISE NOTICE` (matches migration 016 style).

### Migration 018 — `stat_definitions_event_comparable.sql`

Now that event rows carry the derived siblings, mark them as comparable
end-to-end:

- For NFL & football PLAYERS: flip `comparable` to true on
  cumulative_total rows whose `_per_game` / `_per_90` sibling is now
  written by the event trigger. (Or — cleaner — leave the cumulative as
  non-comparable and just rely on the existing per-game / per-90 stat_definitions
  entries, which the event trigger now fills both sides of.)
- For NBA & NFL TEAM rate_pct: nothing to flip (already comparable=true);
  the win is on the trends-SQL side which can drop the entity-side guards.

### `trendsStatement` simplification

Once events carry corrected rate_pct and derived per-X siblings, the
sport-specific guards in `trendsStatement` (`recentRatePctGuard`) become
unnecessary. The CTE can drop the per-sport conditional and use a single
`AND (sd.unit <> 'rate_pct' OR (kv.value)::numeric BETWEEN 0 AND 100)`
sanity clause for everyone. Pure deletion in `go/internal/db/db.go`.

### Sequencing & risk

| Step | Risk | Rollback |
|---|---|---|
| 017 | Low. Trigger functions are idempotent and `STABLE` w.r.t. the same NEW row. Backfill is a no-op UPDATE that's been the standard pattern (migration 015 for team metadata, etc.). | DROP TRIGGER + DROP FUNCTION; data is unchanged because the trigger only overwrites derived keys. |
| 018 | Trivial — UPDATE on stat_definitions. | UPDATE the flag back. |
| Go simplification | Pure deletion; existing tests verify the trends payload shape. | git revert. |

## What it unblocks

- **Player trends on NFL & football** become populated. The frontend's
  trends card shows real rows on the existing surface for every player,
  not just teams.
- **Entity-side rate_pct** becomes trustworthy across all sports — the
  trends payload can stop hiding NBA team `fg_pct` and friends. NBA team
  trends card gains the percentage rows that are arguably the most
  interesting ones for a basketball UI.
- **Percentile pipeline becomes consistent** with derived event data.
  `recalculate_percentiles()` already runs over every numeric key in
  `player_stats.stats` / `team_stats.stats`; once events carry their own
  derived per-game / per-90 keys, the aggregate functions can pull them
  through to the season blob without bespoke per-sport logic. (Several
  existing `aggregate_*_season()` functions already recompute these
  themselves; the trigger lets them simplify to a straight average.)
- **Frontend "+15% vs peers"** with directional arrows becomes safe to
  ship. Today's `tier-color.ts` already supports it; what's missing is
  unit-consistent data on both sides of the comparison.
- **data.scoracle / PostgREST** inherits the cleanup automatically. The
  trends CTE chain — which is intended to lift into a SQL function and be
  exposed as an RPC — gets shorter, no longer carries sport-specific Go
  branching, and is easier to maintain in two places at once.

## Open questions for review

1. **Backfill cost.** Re-firing the trigger on every existing event row
   touches `event_box_scores` (3M+ rows across sports if I'm reading the
   seeder churn right). A single UPDATE rewrites the whole row. Might want
   to chunk it (`WHERE id BETWEEN x AND y`) or run it once during a
   maintenance window. Worth measuring on a copy first.
2. **Per-sport vs. one-function-many-CASE.** Following the existing pattern
   (one function per sport in the per-sport SQL file) keeps ownership
   clear, but it means three near-identical functions for the event-team
   case (NBA, NFL, football). Could fold into one
   `public.enrich_event_team_stats()` with a `CASE NEW.sport WHEN ...` —
   easier to maintain, harder to see per-sport at a glance. Mild
   preference for the existing per-sport pattern for consistency with
   `compute_derived_*_stats`.
3. **Trigger introspection of `stat_definitions`.** Looping over
   `stat_definitions` inside the trigger function makes the trigger
   declarative — every new stat key added to `stat_definitions` with the
   right `is_derived` / `unit` metadata is automatically picked up. The
   alternative (hardcoded key list in the function body, as
   `compute_derived_*_stats` does today) is faster but requires a
   migration for every new stat. The trigger fires on every event upsert,
   so the introspection cost matters — but `stat_definitions` is ~470
   rows with an index on `(sport, entity_type)`, so a per-(sport,
   entity_type) lookup is cheap. Recommend introspection here even if
   `compute_derived_*_stats` keeps its hardcoded list (different access
   pattern — season triggers fire rarely, event triggers fire often).

## Out of scope

- **Renaming / unifying the event-row vs season-row key namespace.** The
  proposal leaves the existing keys intact and adds new ones; no consumers
  need to rename anything. A future cleanup could deprecate one side, but
  that's a separate project.
- **Removing the trends SQL guards now.** They stay until migration 017
  ships and is verified end-to-end; otherwise we re-introduce the Spurs
  bug during the transition.
- **Touching the Python seeder.** Per the A/B/C design, the seeder stays
  the thin layer it is today. All derivation moves to (or stays in) SQL.
