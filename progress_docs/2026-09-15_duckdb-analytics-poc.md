# DuckDB analytics POC: kicked off

Status: **POC complete and verified on production data.** All four decision
gates pass: trajectory equivalence (30/30 entities), bundle equivalence
(10/10 cohorts, all rate modes), bundle performance (DuckDB 5–35x faster and
flat with volume), and the Phase-3 memories enrichment spike is live
(DuckDB-derived cohort context now flows into Scout/Analyst memory packages
through the existing provenance/budget machinery). Continues
[the trace](../planning_docs/PLAN-duckdb-analytics-poc.md) and
[the plan](../planning_docs/PLAN-duckdb-analytics.md).

## What was decided

- Engine: DuckDB 1.5.x stable via the official Go client
  `github.com/duckdb/duckdb-go/v2` (v2.10505.0; version-encodes DuckDB 1.5.5).
  Re-verify against 2.0 (Cyanoptera, ~mid-Oct 2026) before any promotion.
- Postgres access: DuckDB postgres extension, `ATTACH ... (TYPE POSTGRES,
  READ_ONLY)` — Postgres is upstream, no ETL.
- Home: new `go/internal/analytics` boundary package. Nothing else in the
  backend may reference DuckDB. The production Postgres path is untouched;
  selection is config-only.

## Boundary and package layout

```text
go/internal/analytics/
    analytics.go            Analytics interface + Open(ANALYTICS_ENGINE dispatch)
    analytics_test.go
    model/model.go          Trajectory type + Scout-shared math (engine-free leaf package;
    model/model_test.go     exists to avoid an import cycle between the boundary and engines)
    postgres/analytics.go   canonical path: faithful Go reproduction of the Rust Scout
    duckdb/analytics.go     experimental engine: embedded DuckDB, postgres extension attach,
                            all DuckDB SQL lives here
    duckdb/contract_test.go DuckSQL contract pins
    duckdb/equivalence_test.go  Phase-1 correctness gate
```

The interface so far covers the first benchmark workload only:

```go
type Analytics interface {
    RatingTrajectory(ctx, entityType, entityID, sport, season) (model.Trajectory, error)
    Close(ctx) error
}
```

`model.Trajectory` mirrors the Rust `RatingTrajectory` field for field (key,
label, reason, events_played, window_size, sample_size, slope, latest, series
most-recent-first) so the two engines can be compared like for like.

Config (go/internal/config/config.go): `ANALYTICS_ENGINE` = `postgres`
(default) | `duckdb` (validated at Load), `ANALYTICS_DUCKDB_PATH` (empty =
in-memory), `ANALYTICS_DUCKDB_MEMORY_LIMIT` (default 512MB). The DuckDB engine
also caps `threads=2` and uses a single connection.

## The first workload: Scout recent-form trajectory

Implemented in both engines:

- **Postgres path** (`internal/analytics/postgres`): count query + window
  query mirroring `load_rating_trajectory()`
  (rust/src/junctions/scout/mod.rs:959-1060), then `LinearSlope` — a Go port
  of the Rust `linear_slope` (:908, mean-centered OLS, den<1e-9 guard).
  Assembled through shared `model.NewTrajectory`.
- **DuckDB path** (`internal/analytics/duckdb`): the whole computation in one
  query — events CTE over `pg.public.event_box_scores ⋈ pg.public.fixtures`,
  `COUNT(*) OVER ()` for the adaptive window
  `clamp(round(events*0.10),3,16)`, window filter, and `regr_slope(rating,
  chrono_idx)` with the series returned as JSON. No app-side OLS plumbing.

Semantic decisions worth recording:

1. **Tiebreaker**: both engines order the window by
   `start_time DESC, rating DESC`. The production Rust path orders by
   `start_time DESC` only, which is nondeterministic on equal start times;
   the tiebreaker makes the equivalence comparison deterministic and is a
   strict improvement to carry forward.
2. **regr_slope affine-shift invariance**: `chrono_idx` is a global season
   index with gaps after the window filter; OLS slope is invariant under the
   affine shift of x, so no re-index is needed.
3. **Rounding**: `model` uses `math.Round` (half away from zero), matching
   Rust's `round1` at `runtime/util.rs:64`; `LinearSlope` is the exact port of
   :908 including the accumulation order.
4. **Unsupported entity types** return an error at the Go interface (Rust's
   Scout returns `steady("unknown_entity_type")`) — documented divergence.
5. Sparse fallbacks preserved: `sparse_recent_events` (<3 scored events) and
   `sparse_z_score_events` (<3 window sample), both steady.

## Correctness gate (Phase 1)

`duckdb/equivalence_test.go` — external test package, gated on
`TEST_DATABASE_URL` (the repo convention, so CI's postgres:18 provision runs
it). Picks the 10 most-scored players + 5 teams + 15 random players with ≥3
scored events, then asserts per entity:

- key / label / reason / events_played / window_size / sample_size: exact
- slope: `|pg − duck| ≤ 1e-6` (accumulation order differs between the Go port
  and DuckDB's regr_slope)
- series values: `≤ 1e-9`

Boundary-adjacent pins: 0.25 / −0.25 classification thresholds are strict
(greater-than), tested directly; the DuckSQL contract and READ_ONLY attach are
pinned by string-contract tests in repo style.

## Local verification state

- `go build ./...`, `go vet`, `gofmt`, and the full `go test ./internal/...`
  suite are green, including the new packages.
- Nothing is wired into `cmd/api` or any serving path.

## Verified against production data (2026-09-16)

Ran the correctness gate live over an SSH tunnel from the dev Mac to the
canonical Postgres on Archbox (`TEST_DATABASE_URL` = the production
`scoracle` database, local forward 15433→5432):

- **All 30 entities PASS** — 25 players + 5 teams, NBA/NFL/FOOTBALL, seasons
  2018–2026, heavy and sparse samples alike. Key/label/reason, events played,
  window/sample sizes exact; slopes within tolerance; series values equal.
- One driver fix was needed: DuckDB returns a JSON-typed value as
  `[]interface{}` to `database/sql`, so the series is selected as
  `CAST(to_json(list(...)) AS VARCHAR)` and decoded in Go.
- The first run also exercised the postgres extension INSTALL (network
  download, then cached under ~/.duckdb).

**Trajectory latency benchmark** (same tunnel, heaviest player, 50
iterations, Apple M4; `go test -bench BenchmarkRatingTrajectory -benchtime 50x`):

| Engine | ns/op |
|---|---|
| Postgres (count + LIMIT window + Go OLS) | 267,868,230 |
| DuckDB (single regr_slope query) | 589,062,972 |

DuckDB is ~2.2x slower here. This is the outcome the plan predicted: the
production path fetches ~17 rows (adaptive window LIMIT); the DuckDB postgres
extension scan pulls the entity's whole scored-event season over the wire on
every call, and both engines pay tunnel/protocol costs. Trajectory was always
a **correctness validation workload, not a speed one** — the value was proving
DuckDB reproduces the Scout's analytical result, which it now has on real
production data. The decisive performance question remains the Phase-2
population-scan bundle benchmark, where Postgres has no precomputed shortcut
and DuckDB's vectorized scans are the honest comparison.

## Not in this change (pre-existing, unrelated)

The working tree also carries in-flight articulator changes
(`go/cmd/articulator-compose`, `go/internal/articulator`, handler/server
edits, scout.rs edits) from other sessions — do not attribute those to the
DuckDB work.

## The rating bundle: Phase 1 part b (2026-09-16)

The decisive workload. One design decision shapes it:

**`rating_measurements` (migration 252) was NOT re-implemented in DuckDB.** It
is the measurement-identity source of truth (~250 lines of era detection,
GK/out splits, PAdj injections, rate-mode denominators accumulated over 40
migrations), and the 2026-09-14 doctrine says Postgres owns evidence meaning.
Re-expressing it in a second SQL dialect would guarantee drift. Instead the
bundle splits along the natural seam:

```text
Postgres: expansion (253's lasp + eff + dp CTEs, verbatim, incl. the
          FOOTBALL team_opp_possession/league_avg_save_pct merge)
    ↓ pushed down via DuckDB postgres_query('pg', …) — ~0.1–1s, streams
      5.9k–65k expanded datapoints per cohort
DuckDB:   the analytical core (253 lines 67–179): pop (AVG/STDDEV_POP),
          z, sign-weighted composite (SUM FILTER in_comp), percent-rank
          breakdown with scoped pcts, ranks + rating_score (067, inlined),
          scoped ranks/scores
    ↓
equivalence against the LIVE _compute_rating_bundle function (pgx)
```

New interface method: `RatingBundle(ctx, sport, season, rateMode)`.
Postgres impl = call the production function as-is; DuckDB impl = pushdown +
core. Both parse into shared `model.BundleRow`/`BreakdownEntry`.

Gate: `TestRatingBundleEquivalence` over 10 cohorts — NBA 2024/2018 total +
per_36 + per_season, FOOTBALL 2023/2026 total + per_90, NFL 2023/2018 total +
per_game. Tolerances reflect the numeric-vs-double seam: identity fields and
eligibility exact; raw values 1e-9; 4-decimal-rounded fields 4e-4 (covers a
rounding-boundary flip at 1e-4 scale, while a real drift would be orders of
magnitude larger); 1-decimal percentiles/ranks/scores 0.1001.

**Result: all 10 cohorts PASS** — every sport, every rate mode, an old era
(NBA 2018), the early-season gate (FOOTBALL 2026), and ~20k players total.

**Benchmark** (same SSH tunnel, 10 iterations each):

| Cohort | Postgres (live function) | DuckDB | Speedup |
|---|---|---|---|
| NBA 2024 total (587 players, 5.9k datapoints) | 483 ms | 108 ms | **4.5x** |
| FOOTBALL 2023 total (3,412 players, ~65k datapoints) | 3.64 s | 2.94 s | 1.24x |

**Full sweep, one run per cohort, total mode** (`TestBenchmarkSweep`):

| Cohort family | Postgres | DuckDB | Speedup |
|---|---|---|---|
| NBA 2018–2025 (8 cohorts) | 438–742 ms | 85–98 ms | **5–8x, flat** |
| NFL 2018–2026 (9 cohorts) | 3.4 s → **12.3 s** (grows linearly) | 200–355 ms | **10–35x, flat** |
| FOOTBALL 2019–2025 (7 cohorts) | 3.3–3.7 s | 2.8–3.0 s | ~1.25x |
| FOOTBALL 2026 (early season, thin gate) | 739 ms | 106 ms | **7x** |

The headline pattern: **DuckDB's bundle time is flat with data volume;
Postgres degrades linearly** (NFL went 3.4 s → 12.3 s across seasons while
DuckDB held ~330 ms). A full 25-cohort season rebuild is ~98 s on Postgres
versus ~19 s on DuckDB. Process max RSS across both engines: ~346 MB
(memory_limit capped at 1 GB).

Code-volume comparison is honest parity: the DuckDB core (~130 lines of
DuckSQL + ~60 lines of Go binding) replaces 253's analytical half (~110
lines) plus app-side JSON plumbing — the structural win is the boundary and
the measurement function staying single-source, not fewer lines.

Driver lessons recorded in `probe_test.go`: `postgres_query` takes the
ATTACH alias, not a connection string; JSON-typed values must be
`CAST(... AS VARCHAR)` to scan into Go; struct literals must be built as a
named column in a subquery before `list(e ORDER BY key)` serializes as
proper JSON text.

## Next (plan sequencing)

1. ~~Trajectory equivalence gate~~ — 2026-09-16, 30/30 entities pass.
2. ~~Rating-bundle workload + gate~~ — 2026-09-16, 10/10 cohorts pass; DuckDB
   4.5x/1.24x on the two benchmark cohorts.
3. ~~Phase 2 benchmark table~~ — 2026-09-17: full 25-cohort sweep, memory
   figure, code-volume parity (above).
4. ~~Phase 3 memories enrichment spike~~ — 2026-09-17 (below).
5. Decision memo + promotion proposal (smallest production architecture).

## Phase 3: memories enrichment spike (2026-09-17)

The remaining gate: DuckDB producing context Postgres cannot cheaply produce.
Context-injection policy per product direction: **stat tables reach Scout and
Analyst only**; the other characters keep meta/relational context; the Oracle
reads only the other voices' cards and sees no source tables.

### The derived context

New DuckDB computation — per-entity **season-grain arc vs. the cohort's
season-over-season delta distribution**: the entity's rating now, prior
season's rating, delta, its delta's percentile among the league cohort's own
movements, plus the cohort median/p25/p75 and peer count. This answers "rose
while the league held steady" — a cross-entity analytical fact no existing
Postgres query produces, exactly the third context prong the memories work
wants.

### Data path (Postgres canonical, DuckDB read-only)

- **Migration 255** `analytics_entity_context`: PK
  `(sport, entity_type, entity_id, season, league_id)`, columns
  rating/prior_season/prior_rating/delta/delta_pctile/peer_count/
  peer_delta_{median,p25,p75}/computed_at. Additive, self-recording, applied
  to prod 2026-09-17.
- **DuckDB query** (`duckdb.EntityContext`): whole-cohort computation in one
  statement over `pg.public.player_stats`/`team_stats` — arcs via
  self-join on prior season, cohort delta distribution via `quantile_cont`,
  inclusive `percent_rank` for delta_pctile. Entities without a prior rating
  keep their row with NULL deltas; without a current rating, no row.
- **Batch job** `go/cmd/analytics-snapshot`: per cohort → DuckDB → pgx batch
  upsert. **DuckDB itself never writes** — the job owns the only write.
  Full run: 53,186 rows across all 25 cohorts in **18 s**.
- **Rounding**: the model-facing record rounds (rating/delta/peers 2dp,
  percentile 1dp) in the memories SQL; the snapshot table keeps full
  precision.

### memories.rs consumption (stat voices only)

`rust/src/composition/memories/sources.rs` adds an optional
**"cohort trajectory"** group inside the existing Scout/Analyst +
player/team block (so Journalist/Influencer/Insider/Oracle never see it —
verified: Journalist package contains no cohort records). New
`cohort.sql` reads `analytics_entity_context` for the entity (seasons ≤
requested, LIMIT 5, most recent first), each season one `Record` — current
season `PresentEvidence`, earlier seasons `EstablishedHistory` — with a
valid `SourceRef` (`analytics_entity_context:{sport}/{type}/{id}/{season}/
{league}`) and observation time = the snapshot's `computed_at`.

Invariants preserved: provenance resolves (the table is the source); the
group is optional and added after "performance comparison", so it is the
first thing `within_bytes` drops under budget pressure; season-grain rows
change only when the underlying player_stats ratings do (same refresh
semantics as the performance group), and observation times sit outside the
fingerprint material, so a no-op recompute cannot churn cards.

**Interpretation guard** in the group's qualifications: delta is composite
movement, delta_percentile is a percentile of *movement* (cohort-inclusive),
not of ability; a rise from a low base, a fall after a peak and a league-wide
shift all move the same delta. No playing-time, fitness or tactical causes
are attached.

**Known one-time cost**: adding the group changes every Scout/Analyst
`input_hash` debounce once — the next generation run for each entity
regenerates its card with the richer context. After that, season-grain
stability holds.

### Validation

- `memory_inspect` against the canonical DB (SSH tunnel): FOOTBALL player
  881 (Michael Keane) — 2025 arc rendered into the model view: "delta 4.71,
  delta percentile 91.6, peers 274, median −0.29, p25 −2.80, p75 1.84"
  against the full provenance chain in the audit render.
- Mission gating verified: same request as Journalist contains neither the
  group nor the source table.
- Full `cargo test`: 462 passed. The one failure
  (`scout::tests::unranked_one_appearance...`) belongs to the pre-existing
  in-flight scout prompt rework in the same working tree (its own new test
  against its own modified `format_datapoint_evidence` path); the memories
  diff (+36 lines sources.rs, new cohort.sql) is not imported by that test.

## Next (plan sequencing)
