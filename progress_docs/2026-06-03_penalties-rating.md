# 2026-06-03 — Penalties enter the rating (migrations 040, 041)

Bottom-up from the box score, per Scott.

## 040 — penalties conceded (football) + penalty yards both ways (NFL)

- **FOOTBALL**: `Penalties Conceded` (`penalties_committed`, aggregated to team season)
  → negative z, **defense**. Gate-checked: distinct (corr ≤0.22 vs defense terms),
  spread avg 5.9/sd 2.5. (Omitted-when-zero at event grain, but for a discrete event
  absence = 0 is truthful.)
- **NFL**: `Penalty Yards For` (opponent's `penalty_yards`, DERIVED via the opp
  self-join → `penalty_yards_drawn`) → +z, and `Penalty Yards Against` (own
  `penalty_yards`) → −z, in a `discipline` facet. "The penalty battle, both ways."
- aggregate_team_season (football penalties_committed; nfl penalty_yards_drawn) +
  rating_datapoints_team gain the terms; additive backfill; football+NFL teams recomputed.

## 041 — penalties won (football)

- **PLAYER**: `Penalties Won` → **Specialist-only** (in_spec, NOT in_comp). It's sparse
  (~9% of player-seasons nonzero) → by gate-2 a sparse spike belongs in the peak, not
  the breadth sum. Player composite stays byte-identical (proved); it becomes a leadable
  specialty (Ouattara, Vini, Mbappé). (Already aggregated by aggregate_player_season.)
- **TEAM**: `Penalties Won` → **offense** composite (+z). Team-grain denser (avg 4.3/
  sd 2.2), distinct (corr ≤0.36 vs goals/SoT/key passes) — gate-checked. aggregate_team_season
  gains penalties_won; additive backfill; football recomputed.

## Notes

- No API rebuild/restart: both are SQL-only; the new datapoints flow through the
  existing `rating_breakdown` / composite columns the API already serves.
- Frontend: NFL `discipline` penalty datapoints render as chips (CompositeCard chips
  filter relaxed to all non-pizza facets); football `Penalties Conceded` joins the
  defense pizza automatically.
- Watch item: 315 player-seasons now specialise in "Penalties Won" (sparse-spike
  effect). Reversible to display-only if it reads noisy — flagged to Scott.
