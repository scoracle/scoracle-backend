# SQL: the durable world

**Postgres preserves trustworthy facts and durable results. DuckDB computes studies. Studio interprets the evidence.**

Keep identity, source observations, provenance, constraints, product history and indexed serving projections here. Postgres also owns exact work leases and atomic publication. Application code selects evidence and schedules work; Studio owns prompt composition and character judgment. Cheap indexed lookups can stay in SQL. Move analytical scans when their measured workload benefits from DuckDB.

One computation has one active producer. The API maintains Rating cohort/season-change context with DuckDB on startup and every five minutes. It exports bounded consistent inputs, skips unchanged hashes, and atomically publishes results with a source-checked receipt. Failure retains the last complete result. Historical corrections and deleted members are included. The existing SQL rating, event-score and Momentum formulas remain active until separately migrated with parity evidence.

## Tree and ownership

- `schema/`: a generated, checksummed baseline: structure, required reference data and the actual applied migration ledger. `schema.sql` loads reference data before foreign keys/triggers, including circular sports/league references. Never hand-edit this baseline.
- `migrations/`: immutable upgrade history and new forward changes. History is not a second current schema.
- `tests/`: behavior, measurement and integrity contracts.
- `build.sh`, `migrate.sh`, `migration_template.sql`: bootstrap and change tooling.

There are no parallel hand-maintained sport schema files. Application queries stay with their consumers; DuckDB SQL stays in `go/internal/analytics/duckdb/`. Historical descriptions and rollout scripts are in the [wiki archive](../../scoracle-wiki/raw/sql-history/2026-09-20/README.md).

## Build and change

Use PostgreSQL 18 tooling and an empty target database with a bootstrap administrator:

```sh
./sql/build.sh "$NEW_ENV_URL"
./sql/migrate.sh "$NEW_ENV_URL"
```

Build verifies checksums, restores the complete baseline in one transaction and installs its applied ledger. It refuses a nonempty target. Reference data contains product configuration, not users, acquired content or generated products. Historical migrations are not an empty-database installer.

For a change, copy `migration_template.sql` to the next unused migration filename. Derive existing definitions from the verified schema/deployed catalog. Keep DDL and its ledger entry in the same transaction. Release compatible consumers before destructive changes; preserve and restore-test historical data before retiring its table. Never use `CASCADE` to avoid understanding dependencies.

Apply and test the migration against a disposable baseline. Then capture that verified release schema—or the deployed database after an approved cutover:

```sh
scripts/hosting/snapshot-schema.sh "$SOURCE_URL"
```

The capture uses one exported read-only snapshot for structure, reference rows and applied history. Commit all baseline files together. CI checks their checksums and lineage, restores them, verifies migration no-op behavior, and runs SQL, Go and Rust database contracts. Source correctness, NULL versus zero, measurement compatibility and durable recovery matter; retired wording does not.
