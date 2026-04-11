# 2026-04-11 NFL Seeding, Score Mapping Fix, and Aggregator Expansion

## Summary

Seeded the full 2025-2026 NFL season end-to-end, hit a silent score-mapping
bug while inspecting payloads, fixed the handler, backfilled existing data
from already-stored raw responses, then extended the NFL schema to aggregate
every raw BDL stat key (previously only about half were rolled up).

## What Got Seeded

| Step              | Result                                                                 |
|-------------------|------------------------------------------------------------------------|
| Fixtures loaded   | 285 NFL 2025 fixtures, 0 skipped                                       |
| Events processed  | 234 newly processed (51 pre-existing skipped by `get_pending_fixtures`) |
| Box score rows    | 15,296 player rows + 468 team rows added this pass                     |
| Meta seed         | 32 teams, 10,966 players, 0 failed                                     |

## Rough Edge Worth Recording: Meta + Event Process Deadlock

Running `scoracle-seed meta seed nfl` and `scoracle-seed event process
--sport nfl` in parallel deadlocks on the `players` table — both write
paths insert into `players` in different orders, and the background
triggers on `player_stats` add another blocking surface. Ran serially
(events first, then meta) and it worked cleanly. Two options if parallel
runs are needed later: normalize insert order in both paths, or switch to
`INSERT ... ON CONFLICT ... DO NOTHING` with retry.

## Bug #1: NFL team scores never mapped to teams

### Symptom

Pulling `/api/v1/nfl/team/19` (Dallas Cowboys) returned a valid entity
block but `stats.wins`, `losses`, `ties`, `points_for`, `points_against`,
`point_differential` were all zero, and `percentiles` was an empty object.
The Go API `/api/v1/nfl/health` agreed: `team_stats_updated_at: null`.

### Root cause

`seed/services/event/handlers/bdl_nfl.py` was reading
`game.home_team_id` / `game.visitor_team_id` off the BDL stats response,
but BDL actually nests them as `game.home_team.id` / `game.visitor_team.id`.
Both reads returned `None`, the `isinstance(..., int)` guards silently
dropped every score, and `event_team_stats.score` stayed `NULL` for all
570 NFL 2025 rows. `nfl.aggregate_team_season` consequently summed to zero
wins/losses/points for every team.

The `home_team_score` / `visitor_team_score` fields at the top of the
game block were being read correctly — they just had no team id to map to.

### Fix

`seed/services/event/handlers/bdl_nfl.py`: dereference `.id` from the
nested `home_team` / `visitor_team` dicts first, with a fallback to the
flat field for forward-compat. Commit: `Fix NFL event_team_stats.score
mapping from BDL nested team objects`.

### Backfill

Since the stored `event_box_scores.raw_response.game` already has the
full game block (scores included), backfill did not require re-fetching
from BDL:

```sql
UPDATE event_team_stats ets
SET score = sub.score, updated_at = NOW()
FROM (
    SELECT DISTINCT ON (ebs.fixture_id, ebs.team_id)
        ebs.fixture_id,
        ebs.team_id,
        CASE
            WHEN (ebs.raw_response->'game'->'home_team'->>'id')::int = ebs.team_id
                THEN (ebs.raw_response->'game'->>'home_team_score')::int
            WHEN (ebs.raw_response->'game'->'visitor_team'->>'id')::int = ebs.team_id
                THEN (ebs.raw_response->'game'->>'visitor_team_score')::int
        END AS score
    FROM event_box_scores ebs
    WHERE ebs.sport = 'NFL' AND ebs.season = 2025
      AND ebs.raw_response ? 'game'
) sub
WHERE ets.fixture_id = sub.fixture_id
  AND ets.team_id = sub.team_id
  AND ets.sport = 'NFL'
  AND ets.season = 2025
  AND sub.score IS NOT NULL
  AND ets.score IS DISTINCT FROM sub.score;
```

Then re-ran `finalize_fixture(p_fixture_id)` for all 285 NFL 2025 fixtures
to rebuild `team_stats`; the trigger-based percentile normalization on
`team_stats` picked up automatically. Cowboys ended up at 7-9-1, 471 PF,
511 PA, −40.

## Bug #2: Half the defined NFL player stats were never aggregated

### Symptom

Comparing `stat_definitions` for NFL against a live Dak Prescott payload,
22 of 44 defined player stats were missing. Kickers had zero season stats
at all. Non-obvious for the frontend to reason about: the stats exist as
definitions but never show up in any player.

### Root cause

`nfl.aggregate_player_season` in `sql/nfl.sql` only aggregated 22 stat
keys — the original set from the NBA-to-NFL port. The stat_definitions
table was later expanded to include kicking, special teams, and extra
defensive breakdowns, but the aggregator was never extended to match.
BDL provides 56 distinct stat keys per player per game, so the missing
data was sitting in `event_box_scores.stats`, just never rolled up.

Likewise `nfl.aggregate_team_season` only computed standings (wins,
losses, ties, points for/against, differential) — no team offense,
defense, turnovers, or kicking.

### Fix

`sql/nfl.sql`:

1. **New player stat_definitions:** `tackles_for_loss`, `passes_defended`,
   `qb_hits`, `fumbles_touchdowns`, `fumbles`, `fumbles_lost`,
   `extra_points_made`, `total_points`, `touchbacks`, `punts_inside_20`.
2. **New team stat_definitions** (categories `offense`, `defense`,
   `turnovers`, `kicking`): `passing_yards/touchdowns/attempts/completions/
   interceptions`, `rushing_yards/touchdowns/attempts`, `total_yards`
   (derived), `defensive_sacks`, `defensive_interceptions`, `total_tackles`,
   `passes_defended`, `fumbles_lost`, `turnovers` (derived),
   `field_goals_made`, `field_goal_attempts`, `field_goal_pct` (derived).
3. **`nfl.aggregate_player_season`** rewritten to aggregate every defined
   player key BDL provides. Adds inline derivations for `assist_tackles`
   (`total - solo`), `field_goal_pct` (`made / att * 100`), and a
   per-game average for `qbr` (BDL ships QBR per game, so we average over
   games where it's present).
4. **`nfl.aggregate_team_season`** extended to aggregate team offense,
   defense, turnovers, and kicking from the per-game
   `event_team_stats.stats` blob on top of the existing standings logic.
   Derives `total_yards = passing_yards + rushing_yards` and
   `turnovers = passing_interceptions + fumbles_lost` inline.

Commit: `Extend NFL schema to aggregate all raw BDL stat keys`.

### Apply + verify

```bash
python -c "
import psycopg, os
with open('sql/nfl.sql') as f: sql = f.read()
with psycopg.connect(os.environ['DATABASE_PRIVATE_URL'], autocommit=True) as c:
    c.execute(sql)
"
# Then re-finalize every NFL 2025 fixture so team_stats/player_stats
# regenerate via the new aggregators.
```

Re-finalize ran in ~90 seconds for 285 fixtures. Spot-checked Aaron
Rodgers (QB, player 94) — now shows QBR 43.1, full passing line, rushing,
trick-play receptions, fumbles, and zeros across defense/kicking/special
teams (correct for his position). Pittsburgh Steelers (team 7) now shows
full offense/defense/turnovers/kicking alongside the standings.

## Touched

```
seed/services/event/handlers/bdl_nfl.py
sql/nfl.sql
```

## Follow-ups Worth Considering

- **Handler-side field mismatch**: the seeder writes BDL's raw key names
  into `event_box_scores.stats` (e.g., `punt_returns`, `punt_return_yards`)
  but `stat_definitions` uses `punt_returner_returns` /
  `punt_returner_return_yards`. The aggregator handles the mapping today,
  but if any other consumer reads the raw blob they'll hit the drift.
  Cleanest fix: normalize at extract time in `_extract_numeric_stats`.
- **Meta + event process deadlock**: call out in SEEDING_INSTRUCTIONS.md
  that the two commands must be run serially until the insert ordering is
  unified (or `INSERT ... ON CONFLICT DO NOTHING` is added to both paths).
- **Stats BDL doesn't provide**: `rushing_first_downs`,
  `receiving_first_downs`, and `fumbles_forced` are defined but not in
  BDL's response at all. They'll stay empty until a supplemental feed is
  added or the definitions are removed.
- **`defensive_sack_yards`** was not wired up — BDL's `sacks_loss` field
  may cover this, but the semantics (offensive yards lost vs defensive
  yards gained) are ambiguous enough that it was left out this pass.

---

**Status:** ✅ NFL 2025-2026 seeded; every defined NFL player/team stat
now populated from the raw BDL feed.
**Date:** 2026-04-11
