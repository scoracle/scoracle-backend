# Migrations & schema lifecycle

Source of truth = the ordered files in `sql/migrations/`, tracked in the
`public.schema_migrations` table. The canonical `sql/shared.sql` + `sql/{nba,nfl,football}.sql`
are the **base** (tables, views, per-sport triggers, RPCs); the rating / fantasy / percentile
**engine evolves through migrations**. Don't assume `shared.sql` alone builds a working DB.

## Applying migrations (prod / incremental)
```bash
DATABASE_PRIVATE_URL=… ./sql/migrate.sh
```
Applies every migration not yet in `schema_migrations`, in lexical filename order, recording
each. Idempotent — re-running is a no-op. (Bootstrapped once by migration `051`, which created
`schema_migrations` and backfilled `001`–`051`.)

**Atomic recording.** The migration SQL and its `schema_migrations` INSERT are issued in a
**single `psql` process** (never two), so a crash can no longer strand a migration
applied-but-unrecorded across a process boundary. For a plain-DDL file with no transaction
control (and no non-transactional statement like `CONCURRENTLY`), the runner wraps the DDL
**and** the ledger INSERT in one transaction (`--single-transaction`) → genuinely atomic
(crash ⇒ neither applied nor recorded). For a file that manages its own transaction, true
atomicity comes from **self-recording** (see step 4 below); the runner's INSERT is then an
idempotent `ON CONFLICT DO NOTHING` backstop.

Before applying a migration that a running Go API depends on, remember the API **fail-fasts**:
`db.New` prepares every statement at startup (validating columns + functions against the live
schema), so a restart against a drifted schema refuses to boot. Apply the migration first, then
restart/deploy. (This is the guard that turns silent drift into a loud boot failure.)

## Standing up a fresh environment (sandbox.scoracle, fantasy.scoracle, dev)
```bash
./sql/build.sh "$PROD_URL" "$NEW_ENV_URL"
```
Clones the prod **schema only** (no data), including `schema_migrations`, so `migrate.sh` is
incremental from there. Do **not** replay migrations from scratch on an empty DB — several carry
data-dependent gates (e.g. `045`/`046`/`048` smoke checks) that assume real rows.

## Writing a new migration
Start from `sql/migration_template.sql` (copy it to `sql/migrations/NNN_….sql`). Then:
1. Name it `NNN_short_description.sql` (next number; keep it unique). **Next free = 222.**
2. Wrap in `BEGIN; … COMMIT;`. Prefer idempotent DDL (`CREATE … IF NOT EXISTS`,
   `CREATE OR REPLACE`). For data backfills, add a parity/smoke gate (see `045`).
3. **Rebuilding an existing function** (e.g. `finalize_fixture`): derive it from the CURRENT
   prod definition — `psql "$DB" -c "\sf finalize_fixture"` or `pg_get_functiondef(...)` — NOT
   from a possibly-stale canonical file. (Rebuilding from a stale `shared.sql` is what caused the
   049→050 regression.) Then mirror the change into the canonical file.
4. **Self-record (REQUIRED for true atomicity).** Make the LAST statement before `COMMIT;`:
   `INSERT INTO public.schema_migrations(version) VALUES ('NNN_…') ON CONFLICT DO NOTHING;`
   so the version is recorded inside the SAME transaction as the schema change — a crash leaves
   neither applied nor recorded. (A `CONCURRENTLY`/`VACUUM` migration can't run in a transaction,
   so it can't self-record atomically; the runner records it in the same process instead.)
5. After applying, refresh the versioned schema snapshot (`scripts/hosting/snapshot-schema.sh`)
   and commit it alongside the migration, so `sql/schema/` keeps describing live prod.
   The lineage file (`schema_migrations.txt`) is auto-derived from `sql/migrations/` —
   no prod query needed for that part. If you only added a migration (no schema.sql
   refresh needed), regenerate it with:
   `ls sql/migrations/*.sql | xargs -I{} basename {} .sql | sort > sql/schema/schema_migrations.txt`

## Notes
- Duplicate-number history: `042_rating_modes` + `042a_auth_refresh_tokens` (the auth one was
  renamed from a second `042`). Both applied; the lexical runner + table key handle them.
- The runner never touches `sql/migration_template.sql` (it globs `sql/migrations/*.sql` only).
