# 2026-06-03 — Team categories + opposition-allowed stats (migrations 036, 037)

Streaming session building out the team side of the rating engine. Players were
already dialed in; teams needed work. See `planning_docs/RATING_DATAPOINT_AUDIT.md`.

## Decisions

- **Teams get OFFENSE / DEFENSE categories** (display layer). Composite stays FLAT Σz
  — `facet` is a display grouping + a new per-category sub-score, NOT category-balancing.
- **DROP the margins** (Point Differential / Goal Difference): they're *outcomes*, and
  the engine rates *the HOW*. Teams now rated purely on production/process.
- **Opposition-allowed stats are DERIVED from the box score** (the source of truth):
  a team's "allowed" in a fixture = the opponent's production in that fixture
  (`event_team_stats` self-join on `fixture_id`). Lives in `aggregate_team_season`
  (Postgres owns derived stats), so `finalize_fixture` emits them natively going forward.
- **Composite additions are gate-checked & minimal**: NBA `Foul Drawing` (`fta`, team
  corr 0.37 vs pts), NFL `Yards Allowed` (−z, corr ≤0.34 vs splash plays), FOOTBALL
  `SoT Allowed` (−z, corr ≤0.59 vs defensive actions). Everything else fan-legible is
  **display-only** (`in_comp/in_spec=false`): TDs/FGs/first downs, the opponent rates
  (RZ-def%, opp FG%), the **oreb/dreb split**, football **cards/injuries**.
- **Player engine**: NBA `Foul Drawing` (`fta`) added **Specialist-only** (in_spec, not
  in_comp) — fta corr 0.87 vs pts at player grain, so it surfaces as a peak skill +
  percentile without polluting the breadth sum. Chose `fta` over `ftm` (attempts isolate
  drawing contact from FT conversion → credits poor-FT% rim attackers).

## Migrations

- **036_team_opposition_stats.sql** — extends `nba/nfl/football.aggregate_team_season`
  to read more opponent columns and emit `*_allowed`/`def_*` keys. STRICTLY ADDITIVE
  backfill (splices only new keys; existing keys + trigger-derived ast_to_tov etc.
  preserved — proved `changed_existing=0`). Base files updated to match.
  New keys: NFL `yards_allowed, first_downs_allowed, red_zone_def_pct, third_down_def_pct,
  yards_per_play_allowed`; NBA `def_fg_pct, def_fg3_pct`; FOOTBALL `shots_on_target_allowed,
  shots_allowed, big_chances_allowed, opp_possession_pct`.
- **037_team_categories.sql** — `rating_datapoints` (+player Foul Drawing spec-only),
  `rating_datapoints_team` (signature gains `facet`; offense/defense + display-only
  discipline/squad; new composite terms; margins dropped), `compute_team_rating`
  (facet-aware breakdown + new `rating_categories` JSONB = `{facet→{z,pct}}`).
  `compute_team_event_starline` (028) untouched — it selects explicit columns and the
  new display terms are in_comp/in_spec=false.
- **038_rating_breakdown_value.sql** — adds raw `value` (the VOLUME) to every
  rating_breakdown datapoint (player + team), so the data package carries the
  underlying counting stat next to z + pct. STRICTLY ADDITIVE (composite/specialist
  math untouched).
- **`go/internal/db/db.go`** — the `starline` prepared statement's `season_rating`
  CTE now also selects `rating_categories` (teams) / `NULL::jsonb` (players); flows
  through `row_to_json` automatically. ENDPOINTS.md updated. Verified end-to-end over
  HTTP: `rating.rating_categories` + per-datapoint `value` serve correctly.

## Verification (read-only, live 2025)

- `fta` gate-1: player corr(fta,pts)=0.870 (→ spec-only), team=0.370 (→ composite).
- `yards_allowed` corr ≤0.34 vs splash plays; `shots_on_target_allowed` ≤0.59 — distinct.
- 036 backfill: existing keys byte-identical (`changed_existing=0`); new keys land
  240/256/582 (NBA/NFL/FOOTBALL).
- 037: **player NBA composite byte-identical** (joined on season); 157 player-seasons
  now specialise **Foul Drawing** (Luka/Giannis/SGA/Embiid/Booker/Butler) — only movement.
- Team categories populate sensibly: Nuggets O pct100 / D pct6.9; Thunder 86/93; Bayern
  offense z 3.03. Vikings defense breakdown: Yards Allowed pct100, RZ-Def% pct100.

## New columns

`team_stats.rating_categories JSONB` = `{"offense":{"z","pct"},"defense":{"z","pct"}}`.

## Open validation flags (for Scott)

1. **Football board shifted toward process-over-outcomes** (intended): dropping
   `goal_difference` floats up high-volume sides that don't convert results — e.g.
   Man Utd / Lens climbed. Bayern/Barça still top. This is "rate the HOW" working; confirm
   it's the desired behavior or we re-weight.
2. **Bend-don't-break defenses** (e.g. Vikings) show a middling *defense category z*
   even with elite Yards/RZ suppression, because the rated (in_comp) defense terms are
   half splash plays (sacks/INT/PD) which they lack. Suppression shows in the display
   stats. If suppression should drive the *rating* more, promote more `*_allowed` terms
   into the composite (gate-checked).

## Remaining (next)

- Frontend (scoracle-frontend): year selector + offense/defense category pizzas (off
  `rating_categories` + faceted breakdown) + per-slice volume + scope toggles. [task 5]
- Adjust football process-vs-outcome + bend-don't-break defense behavior. [task 7]
