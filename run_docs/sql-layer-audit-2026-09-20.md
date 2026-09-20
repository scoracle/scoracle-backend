# SQL layer audit — September 20, 2026

**Postgres keeps the world trustworthy and recoverable. DuckDB computes bounded studies. Studio interprets prepared evidence.** Application code selects inputs, schedules work and publishes results. A small indexed SQL lookup does not need a DuckDB round trip; arithmetic does not become model judgment simply because it moves out of Postgres.

This is a repository and caller audit, not a live usage or performance measurement. It covers SQL definitions, Go/Rust consumers, hosting scripts and CI, with additional searches of frontend/iOS/client code. No SQL changes or database operations were performed for this audit. Absence of a source caller is a retirement candidate, not proof that an external client or manual operation never uses an object.

## What needs to stay

| Responsibility | Necessary substrate |
|---|---|
| Facts and identity | Sport-qualified entities, metadata, affiliations, fixtures, event observations, source documents and attributed claims. Preserve constraints, provenance and missingness. |
| Durable work and publication | Exact queue leases/revisions, atomic product publication, the application outbox, source invalidation, reconciliation and notifications. `pipeline_work` remains necessary despite its old name. |
| Product history and serving | The six character products, their source/model versions, reporting weeks, active API views/projections, authentication and user data. Keep indexed reads close to stored results. |
| Analytical contracts | Measurement identity, units, eligibility, rates, cohort membership, formula versions and publication freshness. Postgres can store source/configuration/result rows while DuckDB performs the studies. |
| Reproducibility | A verified schema baseline, its actual applied migration ledger, required reference data, forward migrations and behavioral tests. |

The checked-in snapshot contains 79 tables, 101 function definitions, 21 ordinary views, four materialized views and 27 triggers. Counts alone do not identify waste. Several compact transactional triggers protect the current harness.

## Findings and recommended treatment

1. **Fix bootstrap before trusting it as a recovery/build contract.** [build.sh](../sql/build.sh#L20) performs a schema-only dump but says migration history is cloned. It copies the ledger's structure, not its rows; the migration runner then sees an empty applied set. It also omits required reference rows such as stat definitions, rate modes, rating thresholds, templates and routing subscriptions. Build from a verified baseline plus explicit reference data and its applied ledger. Test that a fresh build is usable and a subsequent migration run is a no-op.

2. **Make snapshot lineage truthful.** [snapshot-schema.sh](../scripts/hosting/snapshot-schema.sh#L39) dumps a database, then builds `schema_migrations.txt` from local filenames. That proves which files exist, not which changes the dumped database contains. CI repeats the filename comparison. Capture the source database's applied ledger consistently with the snapshot; compare that ledger with repository migrations separately. Do not silently certify an unapplied migration by adding its filename.

3. **Retire competing schema descriptions.** `shared.sql`, `nba.sql`, `nfl.sql`, `football.sql`, `metadata_system.sql` and the `platform.sql` stub total 4,450 lines. Neither build nor migration tooling applies them. The lifecycle guide still calls them canonical and requires mirroring edits; `metadata_system.sql` admits it is only a readable duplicate. Preserve their historical content in the wiki, extract/verify required reference data, and keep one generated schema snapshot. Archive `prepared/` too: its 179 lines describe the old packet cutover, including a draft of the already-existing migration 213. Correct the guide's stale “next free = 222.”

4. **Remove SQL prompt composition after confirming dependencies.** `narrative_context_for_entity`, `narrative_context_for_pair` and `stat_context_for_entity` still produce prose such as “Prior story” and “Our prior read.” No active application caller was found in the inspected code. The harness now selects and renders sourced memory through `rust/src/evidence/memories/`. These are strong candidates for a narrow forward DROP migration after checking the deployed catalog and external callers. Preserve their underlying historical records. Do not replace them with another generic SQL context builder.

5. **Archive operational history outside the active schema.** The snapshot explicitly labels `oracle_readings` frozen, with no reader or writer; current Oracle publication uses `sigil_synthesis`. `vibe_scores_echo_scrub_20260905` is a dated repair table with no application reference found. Preserve and verify their data before retiring them from the active database/baseline. The unbound `refresh_latest_momentum_scores_per_entity()` trigger helper is another candidate; Go now explicitly refreshes that projection. The projection itself remains actively consumed.

6. **Migrate analytical ownership deliberately.** `recompute_season` still invokes percentiles, ratings, event scores and a serving refresh. Go importers actively call it; current NFL/FPL imports already use `finalize_fixture(..., false)` and recompute once per touched season, so do not misdescribe them as recomputing the whole season per game. Go maintenance still calls `refresh_momentum_scores` and `refresh_peer_cohort_aggregates`; Rust still calls `compute_transfer_heat`. These are live behavior, not dead pipeline code. The generic Go Postgres/DuckDB interface currently has no non-test caller of `Open`, `RatingBundle` or `RatingTrajectory` outside its own implementations; its config switch does not establish a production cutover. The separate snapshot CLI is the concrete bounded DuckDB publication path.

   Reuse that export → private computation → checked publication pattern for the next analytical calculation. Preserve formulas and prove parity first; switch one producer/consumer group, then remove its old computation. `peer_cohort_aggregate` supplies per-stat, position-aware, leave-one-out API comparisons, while `analytics_entity_context` supplies rating-change cohort context. They are not interchangeable just because both say “cohort.” Keep cheap transactional lookups in Postgres unless measurement justifies moving them.

7. **Test the database contract in CI.** The inspected workflow restores the schema and validates Go statements, but does not run `sql/tests/253_rating_evidence_contract.sql` or the Rust database tests. The local harness cleanup passed all 57 Rust database tests. Add those meaningful checks to the disposable database job, plus baseline/reference-data verification. Test provenance, units, NULL versus zero, measurement compatibility, leases and atomic publication—not old prose or incidental function layout.

## Proposed tree

```text
sql/
  README.md                   # concise ownership and lifecycle contract
  build.sh                    # verified offline baseline + reference data
  migrate.sh                  # forward changes only
  migration_template.sql
  schema/
    schema.sql                # generated current structure, single snapshot
    applied-migrations.txt    # ledger belonging to that snapshot
    reference-data.sql        # explicit required configuration, no user/source corpus
  migrations/                 # preserve existing immutable upgrade history
  tests/                      # behavioral SQL contracts
```

Merge the corrected lifecycle guidance into the README; archive the duplicate root DDL, stub and prepared cutover files. Keep DuckDB queries in the existing `go/internal/analytics/duckdb/` implementation and application queries with their consumers. Do not add a generic query framework or split the generated dump into a second hand-maintained schema tree.

The 264 migration files are upgrade history, not 264 active layers. Keep them intact in the first cleanup; replacing the history with a baseline is a separate compatibility decision. The useful reduction is one source of schema truth and one owner per active computation.

## Recommended order

1. Repair baseline/ledger/reference-data handling and its disposable-database checks.
2. Archive duplicate SQL descriptions and completed rollout scripts; write the concise SQL contract.
3. Verify and retire unused prompt builders and historical-only objects with small forward migrations.
4. Move the next valuable analytical workload to the existing DuckDB boundary, retaining transactional publication and active API contracts.

The first two steps simplify the repository without redesigning product calculations. The latter two require explicit database changes and measured analytical acceptance.
