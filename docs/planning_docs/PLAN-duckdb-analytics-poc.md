# DuckDB Analytics POC — Current Path Trace (Phase 1, Task 1)

Historical September 15 trace. The retired root SQL files are preserved in the wiki archive; use the [SQL contract and generated baseline](../../sql/README.md) for current structure and producer ownership.

> **Postgres runs Scoracle. DuckDB studies Scoracle.**
> This document records the current Scout/rating analytical path and selects the
> DuckDB proof-of-concept benchmark target. No architecture changes are made here.

## 1. Box-score storage (PostgreSQL — system of record)

All core tables live in `sql/shared.sql` (per-sport `nba.sql` / `nfl.sql` / `football.sql`
define aggregation functions and views on top):

| Table | Location | Grain |
|---|---|---|
| `event_box_scores` | `sql/shared.sql:429` | per player × fixture; `stats JSONB`, `composite_score` (mig 017), `rating` / `rating_pct` / scoped projections (migs 028/221) |
| `event_team_stats` | `sql/shared.sql:458` | per team × fixture, same derived rating columns |
| `player_stats` / `team_stats` | `sql/shared.sql:106` / `:171` | season-rolled aggregate: `stats JSONB`, `rating`, `rating_breakdown` JSONB, `rating_scoped_ranks`, `rating_modes` |
| `fixtures` | `sql/shared.sql:354` | time axis (`start_time`, `season`, `league_id`) for every window query |
| `stat_definitions` / `rating_thresholds` / `rate_modes` | `sql/shared.sql:198/251/228` | metric metadata: units, comparability, eligibility gates |
| `rating_history` | `sql/migrations/092`, line 254 | append-only per-entity season rating snapshots |
| `momentum_scores` + `latest_momentum_scores_per_entity` | `sql/migrations/128`, `140`, `227` | durable trajectory snapshots (vibe_slope / rating_slope) |

Flow: ingest writes `event_box_scores.stats` → `finalize_fixture()` re-aggregates into
`player_stats.stats` (`{sport}.aggregate_player_season`) → rating bundle + percentiles
precompute the derived columns.

## 2. Where z-scores / metrics are computed

Key finding: **almost all z-score math lives in Postgres SQL functions**, not Go or Rust.

- Season z-engine — `sql/migrations/027_rating_engine_z.sql` (`rating_datapoints`:110,
  `compute_rating`:192: `STDDEV_POP` population → z → sign-weighted composite →
  `percent_rank()`). Team variants at :298/:340.
- Per-event z (starline) — `sql/migrations/028_rating_engine_starline.sql`
  (`compute_event_starline`) z-scores each game's datapoints against the per-event
  population; writes `event_box_scores.rating*`.
- **Current production bundle** — `sql/migrations/253_rating_evidence_contract.sql`
  (`_compute_rating_bundle(sport, season, rate_mode)`:12) using
  `rating_measurements()` from `sql/migrations/252_rating_measurement_identity.sql:8`.
  Self-scaling eligibility gate via `rating_thresholds`, per-measure-population z,
  builds `rating_breakdown` (`[{measure,label,value,z,pct,in_comp,in_spec,sign,facet,scoped_pct}]`),
  `rating_scoped_ranks`, `rating_modes` — exactly what Rust reads.
- Refinement lineage of `_compute_rating_bundle`: 042 (modes) → 043/045/054 (scopes,
  metadata) → 058 (multiscope percentiles) → 064/066 (football PAdj/GK) → 077/079/080
  (position scope, gates) → 247 (FPL era z model) → 252/253 (current identity + evidence
  contract). 221 (`peak_retirement.sql`) performs the global rename
  `rating_composite → rating`.
- Rolling / recent-form slope — **Rust**, not SQL:
  - `linear_slope` (mean-centered OLS) `rust/src/junctions/scout/mod.rs:908`
  - `trajectory_key` (rising/falling/steady, ±0.25) `scout/mod.rs:930`
  - window = 10% of scored events, clamped 3–16 (`scout/mod.rs:955-957`)
  - `relative_direction(delta)` (Rose/Fell/Held, ±1.0) `scout/mod.rs:710`
  - SQL's own slope variant feeds `momentum_scores`
    (`sql/migrations/130_momentum_game_lookback.sql:93-106`)

## 3. Historical / recent-performance retrieval

**Rust Scout** (`rust/src/junctions/scout/`):

- `load_rating_profile()` — `scout/mod.rs:203-320`. Selects the `player_stats`/`team_stats`
  row (`rating_score::float8`, `rating_breakdown`, `rating_scoped_ranks`, `rating_modes`)
  plus a `stat_definitions` join re-deriving the eligibility gate from `rating_thresholds`;
  prefers unscoped then richest league row.
- `load_rating_trajectory()` — `scout/mod.rs:959-1060`. The core recent-form query:

  ```sql
  SELECT e.rating::float8
  FROM public.{event_box_scores|event_team_stats} e
  JOIN public.fixtures f ON f.id = e.fixture_id
  WHERE e.{player_id|team_id} = $1 AND e.sport = $2 AND e.season = $3
    AND e.rating IS NOT NULL
  ORDER BY f.start_time DESC
  LIMIT $4   -- window = clamp(round(events*0.10), 3, 16)
  ```

  then `linear_slope` over the chronological series.
- Cross-season history: `load_rating_profile(..., season-1)` (`scout/mod.rs:1689`),
  joined per-label by `build_skill_changes()` (:811) → `comparison_directions()` (:720)
  = Rose/Fell/Held via `relative_direction(current_pct − prior_pct)`.
- Filters: `drop_off_facet_datapoints` (:572), `drop_degenerate_zero_datapoints` (:548),
  `drop_display_tier_datapoints` (:593).

**Go side** (`go/internal/db/db.go`, plain `pgx/v5`, prepared statements in
`registerPreparedStatements`:239, module `github.com/albapepper/scoracle-data`, layouts in
`go/cmd/{api,pipeline,vibesynth,...}` + `go/internal/{api,db,corpus,...}`):

- `trendsStatement()` (declared :2406, body :2406-2836) — the `/momentum` trends query:
  last-3 fixtures (bridging prior season), `entity_recent_avgs`, season aggregate vs the
  precomputed peer-cohort leave-one-out reconstruction (`peer_aggregate`, :2664-2700),
  full-season `composite_score` sparkline, assembled with `json_build_object` (:2761-2836).
- Movers boards: `trending_vibe_leaderboard` (:627) / `trending_rating_leaderboard` (:715)
  — `DISTINCT ON` over `latest_momentum_scores_per_entity`, ranked by `ABS(slope)`.

## 4. Scout input → LLM

`rust/src/junctions/scout/mod.rs` (Scout "rating" stage, `Role::StatsLogic`, writes
`stat_summaries`). `build_rating_request()` (:1565):

1. `load_rating_profile` → datapoint filters (facet/zero/display-tier drops)
2. `input_components()` (:853) → SHA-256 `input_hash` debounce
3. memories + `compute_notability()` (:355)
4. `load_rating_trajectory()` (recent form) + personnel/availability/scout reports
   (`rust/src/evidence/personnel.rs`, load_scout_reports at :309)
5. prompt: `build_stat_prompt()` `rust/src/junctions/scout/inputs.rs:169`; system prompt
   `rust/src/composition/characters/scout.rs` (`RATING_SYSTEM_PROMPT`)
6. guarded parsing: `RatingParser` / `RatingRequestParser::parse` (mod.rs:1144/1181);
   `first_direction_contradiction` (:1222) vetoes the LLM contradicting the computed
   Rose/Fell/Held direction; `pct_band` (:392) fixes elite/strong/average bands

Consumers: The Analyst (`rust/src/junctions/analyst/`) via `rust/src/runtime/work.rs:208-224`;
`main.rs:154` wires `Stage::Rating → scout::RatingHandler`; batch entry binary
`rust/src/bin/statcommentary.rs`. Reference fixture: `rust/src/examples/scout_request_inspect.rs`.

## 5. DuckDB POC benchmark target

**Primary: `load_rating_trajectory()` (scout/mod.rs:959-1060) + `linear_slope` +
`trajectory_key`/`relative_direction`.**
Rationale: it is Scout's actual "which direction is the metric moving" computation —
an entity-window query over `event_box_scores ⋈ fixtures` with an adaptive window
(10% of scored events, clamped 3–16) and OLS slope. Everything it does in Rust after a
 LIMIT query (OLS slope, rise/fall/hold classification, cross-season pct comparison)
is expressible in a single DuckDB SQL expression
(`regr_slope(rating, row_index)` over a windowed CTE, one general-purpose SQL expression
for direction). Same entities, same Postgres data (via the
Postgres–DuckDB integration), trivially comparable against the existing results.

**Secondary: `_compute_rating_bundle(sport, season, rate_mode)` (mig 253).**
Heavy population-scan z-engine: JSONB datapoint expansion (lateral join), cohort
mean/`STDDEV_POP` z, sign-weighted facet-balanced composite, `percent_rank` scoped ranks,
multiple rate modes. Ideal for measuring full-population OLAP throughput and SQL-code
simplification — but as a per-(sport, season, mode) rebuild it is closer to an offline
workload than a per-request path.

**Tertiary (stretch): Go `trendsStatement` (db.go:2406).** Heaviest single read CTE
workload (last-3 + season series + leave-one-out peer aggregate), but it is a serving
query with heavyweight JSON plumbing — less clean as a first benchmark.

Recommended POC scope: benchmark **primary + secondary** against the Postgres path on
the same basketball/football entities and season, checking result equivalence
(z values, trajectory slopes, Rose/Fell/Held labels) before timing.

## 6. Architecture notes for the POC boundary

- Go module: `github.com/albapepper/scoracle-data`; DB access is raw `pgx/v5`. There is
  **no existing `analytics/` package** — the DuckDB work would introduce one
  (e.g. `go/internal/analytics` with a `Postgres` implementation and a `DuckDB`
  implementation behind one interface, Scout/Rust asking for
  `GetEntityPerformanceContext(entityID)`-style context rather than DuckDB SQL).
- Rust crate: `scoracle-cognition`. Scout should stay ignorant of storage; the context
  handoff point is `load_rating_profile` / `load_rating_trajectory` inputs.
- Postgres stays canonical; DuckDB owns nothing and is a read/compute layer.
- Data access for the POC: DuckDB reads the existing Postgres directly
  (postgres extension). No sync/ETL infrastructure unless benchmarks prove the
  direct path is the bottleneck.
- No prior DuckDB references exist anywhere in the repo (planning/run/progress docs
  all clean as of this writing).
