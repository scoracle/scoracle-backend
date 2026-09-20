# SQL cleanup and DuckDB production cutover — September 20, 2026

Runtime `2969dcdbee4a` is deployed to the Archbox API, all six scheduled/service binaries, and the Mac cognition worker. Migration `263_sql_layer_cleanup` is applied; the actual ledger contains 265 entries. The [SQL contract](../sql/README.md) governs the new tree. The [Studio contract](../rust/README.md) remains the harness north star.

## Active ownership

The API runs one bounded DuckDB cohort maintainer at startup and every five minutes. It exports consistent PostgreSQL inputs, computes privately and publishes a complete scope plus its receipt in one source-checked transaction. An advisory lock excludes duplicate maintainers. Identical input/formula hashes skip computation and publication; failures retain prior complete results and retry. Corrections, NULL changes, deleted members and season rollover participate in invalidation. There is no new service, queue or prompt layer.

All 50 stored NBA/NFL/FOOTBALL player/team season scopes are maintained. The initial pass published 48 and retained two unchanged scopes; the immediate second pass checked 50 and published zero. The resulting projection contains 26,596 rows. API startup independently checked 50 unchanged scopes. The existing Rating evidence adapter reads this projection; ordinary pending work can use it without a model-route change. Publication does not regenerate existing products or enqueue a fleet-wide replay.

The first scheduled pass at 15:31:46 UTC checked all 50 scopes, published zero and reported no competing producer. Both Archbox services remained active with zero restarts, and both workers continued draining normal publication obligations.

SQL rating, event-score and Momentum formulas, per-stat peer serving projections and transfer heat remain active. Their replacement requires separate producer/consumer parity. The older `ANALYTICS_ENGINE` factory setting does not select the cohort maintainer: this production computation uses DuckDB directly.

## Cleanup and proof

- One generated baseline now includes required reference configuration, its actual applied ledger and checksums. Capture uses one exported read-only snapshot. Fresh bootstrap restores atomically; subsequent migrations are a verified no-op.
- Duplicate sport/root DDL, the platform stub, prepared rollout scripts and the former lifecycle guide are archived in the [wiki](../../scoracle-wiki/raw/sql-history/2026-09-20/README.md), commit `3f38d31`. Historical migrations remain immutable.
- Migration 263 removes three unused prose builders, one unbound refresh helper and two historical-only tables, and removes the retired Oracle table from week restamping. Live products, identity, provenance, queue leases and publication machinery remain.
- An isolated PostgreSQL 18 baseline passed the SQL measurement contract, Go prepared-statement registration, the full Go race suite, 497 Rust library tests, 14 evaluation tests, other binary tests and all 57 Rust database tests. Go vet, Rust formatting and shell syntax passed. The isolated cluster on port 55449 is stopped.
- Read-only PostgreSQL/DuckDB parity passed for all 50 production scopes on retained inputs. Exact fields match; only interpolated/rank floats permit the predeclared `1e-9` tolerance. Recovery tests cover failed publication, competing producers, corrections, NULLs and deletions.
- Local and public API smoke tests each passed 18/18 checks. Both workers report the deployed revision. All 15 Rating reservations retain their exact status and availability timestamp; the original cron schedule and both binary watchers were restored. No queue reset or reservation release occurred.

## Deployment evidence and limits

Archbox artifacts: `/mnt/data/backup/scoracle/releases/duckdb-20260920/`. This includes replayable parity reports, validation logs, publication summaries, installed/previous binary hashes, exact cron backups, reservation comparisons and smoke results. Mac binary and launcher backup: `/Users/scotty/scoracle-worker/releases/duckdb-20260920/`.

The retired tables were exported and restored independently before removal: 3,940 `oracle_readings` rows and 831 `vibe_scores_echo_scrub_20260905` rows matched source row digests. Dump SHA-256: `bd59e05f4fa93d6cc1dcbd53466f459053c05bf8a926fa30e3b553fc5ad1816e`. A matching copy resides at `/home/sheneveld/scoracle-offdisk-backup/duckdb-20260920/sql-historical-products.dump`.

The first deployment attempt applied the migration, then stopped before publication because `/usr/bin/time` was absent. The exit trap restored compatible prior binaries and resumed production. Measurement was changed to Python's standard library; the second attempt completed. Recorded Archbox pause/resume intervals were 15:23:00–15:24:18 UTC and 15:25:28–15:26:47 UTC (78 and 79 seconds). These are service-operation intervals, not independently sampled public outage durations. Existing worker shutdown/lease recovery handled interrupted work.

Initial publication took 1.524 seconds, 0.767 user CPU seconds, 0.104 system CPU seconds and 163,308 KiB peak process RSS. The API used 59,260 KiB RSS in an early post-start sample. These observations do not establish sustained resource overhead, acquisition-cycle performance or improved model prose. The existing [quality findings](quality-2026-09-20/findings.md) still apply; semantic quality work and the 15 held cases remain separate.
