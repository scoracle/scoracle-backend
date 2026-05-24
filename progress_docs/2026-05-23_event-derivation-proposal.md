# Proposal — close out the trends-card known limitations

Date: 2026-05-23 (second pass)
Status: Proposal — not yet implemented.

## What this proposal covers

Two known limitations from the trends comparability work
(`2026-05-23_trends-unit-comparability.md`):

1. **Entity-side rate_pct is unreliable for NBA & NFL.** The trends endpoint
   filters them out. NBA team `event_team_stats` rate keys (`fg_pct`, `ft_pct`,
   `fg3_pct`) get written by the BDL seeder as the SUM of player-row
   fractions, producing values like 4.0 per team-game instead of ~47.
   Football SportMonks per-fixture rate keys (`tackles_won_percentage`,
   `duels_won_percentage`, `aerials_won_percentage`) are non-normalized
   provider aggregates that can read as 700 for a single match.

2. **Player trends on NFL & football show few or zero comparable keys.** The
   key namespace differs across tables: `event_box_scores.stats` carries raw
   per-fixture counts (`tackles`, `passing_yards`) while `player_stats.stats`
   carries the derived per-game / per-90 siblings (`tackles_per_90`,
   `passing_yards_per_game`). The trends SQL intersects the two — they
   never line up, so the intersection is empty.

## First-pass proposal and why it shrank

The first version of this doc proposed a uniform fix: a new
`enrich_event_*_stats()` BEFORE trigger family on `event_box_scores` and
`event_team_stats`, parallel to the existing `compute_derived_*_stats()`
triggers on `player_stats` / `team_stats`. Per-sport functions, full
backfill, `stat_definitions` introspection inside the trigger, the works.

On second pass with live data audits in hand, the two problems split into
two different right answers, and a lot of the first-pass complexity
collapsed:

- **Problem 2 (key namespace mismatch) is fully solvable as a read-side
  change in `trendsStatement`.** No migration, no backfill, no trigger.
  Today the only consumer of event rows is the trends endpoint; pushing
  the derivation into the source table only matters when there are
  additional consumers. There aren't.
- **Problem 1 (rate corruption) genuinely benefits from a trigger** —
  the stored event-row value is wrong, and any current or future consumer
  reading event rows sees garbage. Writing the correct value at the
  source fixes everyone downstream in one place.
- **`stat_definitions` introspection inside the trigger is overkill.** The
  existing season-level `compute_derived_*_stats` functions use hardcoded
  per-sport key lists; matching that pattern is more readable and faster.
  Adding a new stat key already needs a migration; introspection doesn't
  save any work.
- **Migration `018` (flipping `comparable = true` on player cumulatives)
  isn't needed.** Both changes work via the existing per-game / per-90
  `stat_definitions` rows. Cumulative rows can stay non-comparable.

The result is **two independent changes that ship sequentially**, each
materially smaller than the first-pass plan.

## Audit findings that shaped the simplification

Verified directly against the live DB during this session.

- **NBA player_stats.fg_pct is on the 0..100 scale, matching team_stats.**
  Both season blobs agree. The 0..1 scale only appears in
  `event_box_scores` for NBA players (verified value `0.375` for a single
  game). So fixing team event_team_stats rate keys to 0..100 brings
  team-event into agreement with team-season; player events remain on
  the 0..1 scale but they're filtered out of trends anyway (per the
  Phase A sport-specific rate_pct guard).
- **Three of the four SportMonks broken rate keys have raw inputs in the
  same event row, one does not.** Live check on Spurs:

  | Key | Raw inputs present | Recomputable |
  |---|---|---|
  | `tackles_won_percentage` | `tackles_won=9`, `tackles=17` | ✓ |
  | `duels_won_percentage` | `duels_won=42`, `total_duels=75` | ✓ |
  | `aerials_won_percentage` | `aerials_won=NULL`, `aerials_total=NULL` | ✗ |

  The trigger can't fix what isn't there. `aerials_won_percentage` keeps
  the `[0, 100]` SQL guard (or stays masked behind the NBA/NFL-style
  per-sport drop, depending on how the trends statement is restructured
  after Change B).
- **`aggregate_player_season` / `aggregate_team_season` are hardcoded
  per-key, not "all numeric."** Verified `sql/nba.sql:475`. The function
  reads specific stat names (`pts`, `fgm`, etc.) and writes specific
  derived keys — it does NOT pull every numeric jsonb_each. So new
  derived event-row keys would not get accidentally summed into season
  blobs. Same architectural property for NFL and football aggregates
  (same pattern).
- **`aggregate_*_season` already computes correct team season rates from
  raw counts**, e.g. NBA's `fg_pct = 100 * fgm_sum / fga_sum`. This means
  the rate corruption today only affects consumers that read event rows
  directly — the season blob has always been right. Narrows the trigger's
  impact to "trends entity-recent side + any future event-row consumers."

## Change A — read-side `trendsStatement` extension (player trends fix)

**Scope:** purely a `trendsStatement` change in `go/internal/db/db.go`.
No migration, no trigger, no backfill.

**Idea:** the `entity_recent_avgs` CTE currently averages raw values and
filters by `stat_definitions.comparable = true`. Extend it so that, for
`cumulative_total` keys on NFL & football PLAYERS specifically, the CTE
emits the value under the canonical derived key name (`<base>_per_game`
for NFL, `<base>_per_90` for football, with the `* 90 / minutes_played`
factor on the football side).

After the change, the emitted entity-recent key intersects with the peer
`*_per_game` / `*_per_90` keys that already live in `player_stats.stats`
and that the season-level `compute_derived_*_stats()` trigger has been
maintaining all along.

Sketch (real code lives in `trendsStatement`'s `entity_recent_avgs` CTE):

```sql
SELECT
    CASE
        WHEN sd.unit = 'cumulative_total'
             AND req.entity_type = 'player'
             AND '<SPORT>' = 'NFL'
            THEN sd.key_name || '_per_game'
        WHEN sd.unit = 'cumulative_total'
             AND req.entity_type = 'player'
             AND '<SPORT>' = 'FOOTBALL'
            THEN sd.key_name || '_per_90'
        ELSE sd.key_name
    END AS emit_key,
    CASE
        WHEN sd.unit = 'cumulative_total'
             AND req.entity_type = 'player'
             AND '<SPORT>' = 'FOOTBALL'
            THEN AVG((kv.value)::numeric * 90.0
                     / NULLIF((e.stats->>'minutes_played')::numeric, 0))
        ELSE AVG((kv.value)::numeric)
    END AS avg_val
FROM entity_events e
...
```

**Cumulative player keys also need `comparable = true` for the JOIN to admit
them.** A tiny migration delta (one UPDATE on `stat_definitions` for NFL +
football player rows) flips the flag. Or — alternative — the JOIN's
`comparable` predicate is widened in SQL to also admit
`(unit = 'cumulative_total' AND entity_type = 'player' AND sport IN
('NFL','FOOTBALL'))`. Either works; flipping the flag is more discoverable.

**Per-90 minutes guard:** intentionally NOT added. The original trends
endpoint design rule is "raw values only — frontend decides what direction
means visually" (see `2026-05-22_trends-endpoint.md`). A 5-minute sub
appearance with one goal showing `goals_per_90 = 18` in the 3-event window
is honest reporting of what happened, and the frontend's top-5-by-|delta|
display will rank it appropriately. If outliers become a real UX problem
later, the right place to fix it is in the frontend's display logic, not
silently in the SQL.

**Risks:** very small. The change is additive at the dictionary level
(emits keys under different names than today) and the peer side already
exposes those key names. Worst case: a player with `minutes_played = 0`
in an event row emits NULL per-90 values, AVG ignores them — same
behavior as today.

**Verification path:** rebuild Go binary, curl
`/api/v1/football/player/<id>/trends` for an entity with ≥ 3 fixtures,
confirm intersect of `entity_recent_avgs` / `peer_season_avgs` contains
`*_per_90` keys with comparable values. Same for NFL with `*_per_game`.

**Effort:** ~30 lines of SQL inside the helper, no other moving parts.

## Change B — `017_event_rate_recompute.sql` (rate corruption fix)

**Scope:** one migration. One trigger function family. Only `event_team_stats`
(the worst offender; `event_box_scores` has narrower rate-key surface
area and is left for follow-up if needed).

**Idea:** add `nba.enrich_event_team_stats()` and
`football.enrich_event_team_stats()` BEFORE INSERT OR UPDATE OF stats
triggers on `event_team_stats`. Each loops over a hardcoded list of
(target rate key, numerator key, denominator key) tuples, recomputes the
target key from the raw inputs in the same row, and merges the result
back into `NEW.stats`. Mirrors the file ownership and shape of the
existing `compute_derived_*_stats()` triggers.

```sql
-- nba.enrich_event_team_stats() body sketch
NEW.stats := NEW.stats
    || jsonb_build_object(
        'fg_pct',  CASE WHEN (NEW.stats->>'fga')::numeric  > 0
                        THEN 100 * (NEW.stats->>'fgm')::numeric  / (NEW.stats->>'fga')::numeric  END,
        'fg3_pct', CASE WHEN (NEW.stats->>'fg3a')::numeric > 0
                        THEN 100 * (NEW.stats->>'fg3m')::numeric / (NEW.stats->>'fg3a')::numeric END,
        'ft_pct',  CASE WHEN (NEW.stats->>'fta')::numeric  > 0
                        THEN 100 * (NEW.stats->>'ftm')::numeric  / (NEW.stats->>'fta')::numeric  END
    );
RETURN NEW;
```

**Per-sport coverage:**

| Sport / fixable key | Numerator | Denominator | Status |
|---|---|---|---|
| NBA `fg_pct` | `fgm` | `fga` | recompute |
| NBA `fg3_pct` | `fg3m` | `fg3a` | recompute |
| NBA `ft_pct` | `ftm` | `fta` | recompute |
| Football `tackles_won_percentage` | `tackles_won` | `tackles` | recompute |
| Football `duels_won_percentage` | `duels_won` | `total_duels` | recompute |
| Football `aerials_won_percentage` | — | — | **inputs missing per fixture**; stays masked by the trends SQL `[0, 100]` guard |

**Trigger declaration detail:**

```sql
CREATE TRIGGER trg_nba_enrich_event_team_stats
    BEFORE INSERT OR UPDATE OF stats ON event_team_stats
    FOR EACH ROW
    WHEN (NEW.sport = 'NBA')
    EXECUTE FUNCTION nba.enrich_event_team_stats();
```

`OF stats` is load-bearing: without it, ANY update to `event_team_stats`
(adding a column later, setting a status flag, anything) re-fires the
trigger and rewrites the JSONB column. With it, the trigger only fires
when the stats column is part of the UPDATE.

**Backfill in the same migration:**

```sql
-- Chunked to keep transactions short and WAL volume manageable.
DO $$
DECLARE
    chunk_size INT := 50000;
    max_id     INT;
    lo         INT := 0;
BEGIN
    SELECT MAX(id) INTO max_id FROM event_team_stats WHERE sport IN ('NBA','FOOTBALL');
    WHILE lo <= max_id LOOP
        UPDATE event_team_stats
           SET stats = stats
         WHERE id BETWEEN lo AND lo + chunk_size - 1
           AND sport IN ('NBA','FOOTBALL');
        RAISE NOTICE 'backfilled chunk %–%', lo, lo + chunk_size - 1;
        lo := lo + chunk_size;
    END LOOP;
END$$;
```

The seeder should be paused for the duration (~minutes given current row
counts) to avoid lock contention. Estimated impact: `event_team_stats`
has on the order of low tens of thousands of rows total, not the
millions of `event_box_scores`. The backfill is bounded.

**Once Change B is verified live**, the per-sport `recentRatePctGuard` in
`trendsStatement` collapses to a single shared `[0, 100]` sanity clause.
That's a separate, trivial PR — pure deletion in `db.go` — that
demonstrates the trigger did its job.

**Risks:**

- Backfill row-locks `event_team_stats` for the chunk's duration. Pause
  the seeder; run during a quiet window if any other process reads the
  table.
- The trigger function is `VOLATILE` (default — it modifies `NEW`). Do
  not mark `STABLE` or `IMMUTABLE`.
- A future provider data correction that re-upserts the same fixture
  triggers re-derivation. That's the desired behavior.

## Sequencing

| Step | Ships | Effort | Risk |
|---|---|---|---|
| Change A (read-side player trends) | self-contained | ~30 lines SQL inside `db.go` + 1 small `stat_definitions` flag update | very low |
| Change B (rate-recompute trigger) | independent of A | one migration + one Go cleanup PR | low |
| Trends-SQL cleanup of per-sport guards | after Change B is verified | pure deletion in `db.go` | very low |

Both changes can ship same-week if desired; neither blocks the other.

## What it unblocks

- **Player trends on NFL & football** become populated. The trends card on
  player pages for those sports shows real rows instead of being mostly
  empty.
- **Entity-side rate_pct for NBA team trends** becomes trustworthy. `fg_pct`
  / `fg3_pct` / `ft_pct` show up correctly on the team trends card.
- **Football team rates** remain trustworthy (already are after Phase A's
  Spurs fix), with two additional rate keys (`tackles_won_percentage`,
  `duels_won_percentage`) now derived correctly at the event level
  rather than masked by the SQL guard.
- **Future event-row consumers** (data.scoracle PostgREST, ad-hoc
  analysis, debug tooling) inherit correct rate values automatically —
  the source of truth is finally consistent with the season blob.
- **`trendsStatement` gets shorter** once the per-sport `recentRatePctGuard`
  can be removed, making the data.scoracle SQL-function lift cleaner.

## What this proposal explicitly does NOT do

- **No trigger on `event_box_scores`.** The only `event_box_scores` rate
  issue is NBA player `fg_pct` being stored on a 0..1 scale (vs season's
  0..100). Since player rate_pct is filtered out of trends anyway, fixing
  this is purely about cleaning up the source data — separate session
  if/when warranted.
- **No NFL player or football player triggers.** Their per-game / per-90
  emission happens read-side in Change A. The triggers would be
  duplicative.
- **No minutes-played threshold for football per-90.** Honest reporting of
  the 3-event window matches the trends endpoint's "raw values only"
  design rule. Outlier handling, if needed later, belongs in the
  frontend's display logic.
- **No touching the Python seeder.** A/B/C architecture preserved.
- **No removing the trends SQL guards before Change B is verified.** The
  `recentRatePctGuard` stays in place until the trigger is live and the
  data is confirmed clean.
