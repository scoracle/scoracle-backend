# DuckDB Analytics — Build-Out Plan

Companion to `PLAN-duckdb-analytics-poc.md` (current-path trace). This document turns
the POC into an executable plan with integration decisions, phases, gates, and the
smallest production architecture if it succeeds.

> **Postgres runs Scoracle. DuckDB studies Scoracle.**
> Postgres remains canonical. DuckDB initially owns nothing.

## 1. Version and client decisions

| Decision | Choice | Rationale |
|---|---|---|
| Engine version | Start on **1.5.x stable**; re-verify against **2.0 (Cyanoptera)** when it ships (~mid-Oct 2026) | 2.0 is feature-frozen alpha (only Python/CLI clients today); Go/Rust clients lag. The POC is version-agnostic; re-bench on 2.0 before any promotion. |
| Go client | `github.com/duckdb/duckdb-go/v2` (official, primary client; static linking; `database/sql` interface) | Matches `internal/db` conventions (plain driver, raw SQL, prepared statements). Adds CGO/static libs to the binary — acceptable for the POC binary, noted as a risk. |
| Rust client | Not in the POC. If Scout-side embedding is ever needed: `duckdb` crate (`duckdb-rs`, 1.5.x) | Keeps `scoracle-cognition` ignorant of storage, per handoff constraint #3. |
| Postgres access | DuckDB **postgres extension**: `ATTACH 'postgres://…' AS pg (TYPE POSTGRES, READ_ONLY)` | No ETL/sync infrastructure (constraint #2). Direct reads treat Postgres as upstream. |

**2.0 notes for the later re-verify:** VARIANT with shredding maps directly onto the
`stats JSONB` workload (field filters/aggregates ~6x faster than JSON text, ~parity with
typed columns, no schema declaration); server mode (quack 1.0 + CONNECT) would replace
per-process attachment if multiple consumers appear; async I/O and the recursive-CTE
rewrite help future `rating_history`/graph traversals.

## 2. Integration shape

```text
go/internal/analytics/          ← new boundary package
    analytics.go                ← interface: Analytics { … } (+ postgres impl + duckdb impl)
    postgres/                   ← current path (thin wrappers over existing internal/db queries)
    duckdb/                     ← experimental path: duckdb-go driver, embedded engine,
                                  postgres-extension ATTACH, all DuckDB SQL lives here
    duckdb/queries/             ← .sql files (mirrors internal/db + memories/*.sql style)
```

Rules:

- Application code, handlers, and AI code call `Analytics`, never DuckDB.
- The DuckDB connection is process-lifetime (single instance, `READ_ONLY` attach),
  configured with conservative `memory_limit` and `threads` so an analytics query can
  never starve the API.
- The DuckDB implementation is feature-flagged and never defaults into a serving path
  during the POC.
- Scout/AI continue to receive *prepared context* (constraint #4). In this plan DuckDB
  results reach Rust either (a) through a derived-snapshot table Postgres holds for
  memories.rs, or (b) via the existing work/pipeline plumbing — never raw DuckDB SQL.

## 3. Phases

### Phase 1 — Correctness harness (the gate; do not skip)

Goal: prove equivalence before timing anything.

1. Pick a fixed benchmark set: ~20 entities per sport (10 star, 5 median, 5
   low-sample/edge cases — one-appearance snapshots, position-scoped football players,
   FPL era-dead labels), current season + prior season.
2. Implement in `internal/analytics/duckdb`:
   a. **Trajectory workload** — replicate `load_rating_trajectory()` +
      `linear_slope` + `trajectory_key`/`relative_direction`
      (rust/src/junctions/scout/mod.rs:959-1060, :908, :930, :710) as one DuckDB
      query: window CTE over `pg.event_box_scores ⋈ pg.fixtures`
      (`clamp(round(events*0.10),3,16)`), `regr_slope(rating, row_index)`,
      classification.
   b. **Rating-bundle workload** — replicate `_compute_rating_bundle(sport, season,
      rate_mode)` (sql/migrations/253_rating_evidence_contract.sql) including
      `rating_measurements()` expansion, eligibility gates (`rating_thresholds`),
      per-measure-population z, sign-weighted facet composites, `percent_rank`
      scoped ranks, for all rate modes.
3. Equivalence test (Go test, runs both paths against the same Postgres):
   - z values: `abs(pg − duck) ≤ 1e-9` (or explicitly documented float tolerance)
   - ranks/pcts: exact match after rounding to stored precision
   - slopes: tolerance in stored units; classification labels **exact** (Rose/Fell/Held
     at ±0.25 threshold — no tolerance near the boundary; investigate any flips)
   - eligibility decisions: exact
4. Any mismatch → fix before Phase 2. Expect the bundle to be the hard part (it encodes
   ~40 migrations of refinements: 042→247→253 lineage).

### Phase 2 — Benchmark

Same entities, both paths, measured:

| Metric | Method |
|---|---|
| Latency | Go benchmark (go test -bench), p50/p95 over the entity set; trajectory per-entity, bundle per-(sport,season,mode) |
| Memory | DuckDB profiling + process RSS; compare with Postgres `EXPLAIN ANALYZE` |
| Code volume | LOC count: mig-253 SQL + app glue vs DuckDB query files |
| Rolling/window complexity | Qualitative: Rust `linear_slope`/window plumbing vs single SQL expression |

Known going in (from the trace): the trajectory workload will likely be *at parity* —
the Rust path is a LIMIT-16 query plus OLS over ≤16 values; that benchmark mostly
validates correctness. The decisive measurement is the **bundle**: population scans,
JSONB lateral expansion, cohort z, `percent_rank` — where 2.0's VARIANT shredding and
DuckDB's vectorization should show, and where SQL-simplification is a win even at
parity speed.

### Phase 3 — Memories enrichment spike (the AI-product payoff)

Goal: prove the "third prong" gets materially richer from DuckDB-produced context.

1. New DuckDB query: per-entity **multi-season trajectory + cohort comparison**
   (entity z history across `player_stats` seasons, peer-cohort arc comparison,
   per-90 deltas) — material DuckDB produces easily and `_compute_rating_bundle`
   doesn't today.
2. Write results to a Postgres derived-snapshot table (season-grain, stable) — DuckDB
   writes *derived knowledge*, Postgres holds it (canonical-owner rule preserved).
3. Extend `memories/sources.rs` performance block to emit these as new
   `EstablishedHistory`/`PresentEvidence` records with valid `SourceRef`s.
   Constraints from the memories design:
   - season-grain stability: refresh only on snapshot rebuild, so the
     `with_input_components` fingerprint doesn't churn cards on every scan
     (mirrors `player_stats` refresh semantics)
   - new groups enter `within_bytes` as optional groups in relevance order
   - no computed field ever becomes "new evidence" — it is derived knowledge from
     named sources
4. Validate with the offline `memory_packages` example + `scout_request_inspect.rs`
   fixture: richer package renders, budget holds, fingerprint stable.

### Decision gate

Promote DuckDB only if **all** hold:

1. Phase 1 equivalence passes (correctness first, per the handoff).
2. Bundle workload: ≥ parity latency *and* demonstrably less SQL/app code, or a
   substantial speed win.
3. Phase 3 spike shows a richer memory package the Postgres path cannot cheaply
   produce (cross-entity / multi-season context).
4. Production path untouched: default behavior identical with the flag off.

If only (1) passes → conclusion is "Postgres already does this"; keep the harness,
revisit when history volume grows.

## 4. Smallest production architecture (if gate passes — not part of this work)

```text
Postgres (canonical) ──read──▶ DuckDB engine (embedded in a Go job/batch context)
                                    │
                                    ├── trajectory/bundle recomputes where cheaper
                                    ├── cross-entity/multi-season derived context (new)
                                    └── writes derived-snapshot tables → Postgres
                                            │
                                    memories.rs reads them as EvidenceGroups
                                            │
                                    characters (voice + form.rs) — unchanged
```

- DuckDB runs in **one** place: a maintenance/analytics binary (existing
  `go/cmd/` pattern, e.g. alongside `pipeline`), not in the API server hot path.
- Postgres stores derived snapshots (DuckDB owns nothing); freshness = rebuild job,
  like existing `recalculate_*` / `momentum_scores` jobs.
- No server mode, no Parquet, no ducklake yet — revisit on 2.0 GA when serving and
  historical-corpus needs justify it.

## 5. Deferred (explicitly)

- Parquet cold corpus; DuckLake/Iceberg — until volume justifies.
- DuckDB server mode / multi-client — until >1 process needs the engine.
- Triggers — Postgres remains the transactional authority.
- Rust-side embedding (duckdb-rs in cognition) — derived context flows through
  Postgres snapshots instead.

## 6. Risks

| Risk | Mitigation |
|---|---|
| Bundle re-implementation drift (40-migration lineage) | Phase 1 gate; port migration *tests* (`sql/tests`) semantics into Go equivalence test |
| postgres extension row-extraction cost | Benchmark early (Phase 1 step 2); if attach is the bottleneck, fall back to periodic Parquet export (first, minimal ETL — only with evidence) |
| Binary bloat / CGO in Go (static DuckDB libs) | POC builds are fine; production placement is a batch job binary, not the API server |
| DuckDB query starving shared host | `memory_limit`, `threads` capped; single-process batch context |
| Floating-point divergence near ±0.25 / ±1.0 classification thresholds | Exact-match gate on labels; investigate boundary flips individually |
| 2.0 breaking changes (storage format, parser) | POC on 1.5; promotion requires re-bench + re-verify on 2.0 GA |

## 7. Sequencing

1. `go/internal/analytics` skeleton + DuckDB instance/attach + config flag  (small)
2. Trajectory DuckDB query + equivalence test  (small)
3. Bundle DuckDB query + equivalence tests  (the bulk of the work)
4. Benchmarks + write-up  (small)
5. Memories enrichment spike  (separate PR; only after 2–3)
6. Decision memo → smallest production architecture above
