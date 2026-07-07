# NFL Air/Ground Z-Score Revision

Date: 2026-07-06

## Summary

Updated the NFL player z-score equation to use a flat, positionless composite with seven box-score responsibility buckets:

- Air Yards Responsible
- Ground Yards Responsible
- Points Responsible For
- Giveaways
- Tackling
- Tackles For Loss
- Interceptions

This replaced the prior NFL offense/defense facet-balanced composite and removed standalone datapoints that were either unavailable in BDL event payloads, duplicated other buckets, or rewarded opportunity more than impact.

## Production Changes

- Applied `131_nfl_player_drop_forced_fumbles`.
  - Removed Forced Fumbles from NFL player z-scores because BDL does not include forced fumbles in event box-score payloads.
- Applied `132_nfl_player_positionless_air_ground`.
  - Replaced Total Yards with Air Yards Responsible and Ground Yards Responsible.
  - Replaced Touchdowns with Points Responsible For.
  - Folded fumbles_lost back into Giveaways.
  - Folded sacks into Tackles For Loss using `GREATEST(tackles_for_loss, defensive_sacks)`.
  - Removed Receiving, Fumbles Lost, Sacks, Pass Defense, Field Goals, and Punting as standalone composite datapoints.
  - Switched NFL player season composites from facet-balanced to flat z-score sum.
  - Switched NFL event starline from facet-balanced to flat z-score sum.
  - Recomputed NFL player ratings and event starline.

## Rationale

The previous defensive board was inflated by fumble recoveries, forced fumbles, and then passes defended. Passes defended is a real box-score stat, but it rewards target opportunity; strong cornerbacks can be punished because they are avoided, while weaker or second corners get more chances to record pass breakups.

The new model keeps only defensive events with clearer box-score impact: tackles, tackles for loss, and interceptions.

For offense, splitting yards into Air and Ground gives production two honest axes without adding usage-only stats like targets, receptions, attempts, or completions. Ground production gets its own bucket because sustained rushing value is materially different from passing/receiving/return yardage.

## Post-Migration Leaderboard Shape

Top 25 after migration:

1. Matthew Stafford, QB
2. Ernest Jones IV, LB
3. Kevin Byard, S
4. Jonathan Taylor, RB
5. Christian McCaffrey, RB
6. Maxx Crosby, DE
7. Marcus Jones, CB
8. Josh Allen, QB
9. Myles Garrett, DE
10. Drake Maye, QB

Top 25 composition:

- RB: 7
- LB: 6
- QB: 5
- CB: 4
- DE: 2
- S: 1

Top 50 composition:

- LB: 13
- QB: 10
- RB: 10
- S: 9
- CB: 5
- DE: 3

## Verification

- `rating_datapoints('NFL', ...)` emits exactly the seven expected labels.
- Removed labels no longer emit: Receiving, Fumbles Lost, Sacks, Pass Defense, Field Goals, Punting, Forced Fumbles.
- NFL `rating_breakdown` has no rows for removed labels.
- Both migrations self-recorded in `public.schema_migrations`.
