# FIRST-GPT-AUDIT Session 15 — Harden backups, restores, and migrations

**Date:** 2026-06-24 (archbox, production)
**Commit:** _(see git log; this session)_ — scripts + one new Go cmd + a versioned schema snapshot.
**Migration:** none (no schema change; **next free migration stays 107**).
**API restart:** none required — S15 is backup/restore/migration tooling, not the serving path.

## Goal

Replace "we have backups" with "we can restore a backend that **boots**," and make migration
application + ledger recording one step so a fresh/restored environment is never ambiguous.
(FIRST-GPT-AUDIT Session 15.)

## What shipped

### 1. Restore drill → a "bootable backend" proof (`scripts/hosting/restore-drill.sh`)

Re-confirmed first (F-015 discipline — read reality, not the audit text): the live drill **already**
had the `tweets` check removed and **no `|| true`** around `pg_restore`. The audit's Problems list
described an older file. What S15 actually added:

- **`pg_restore` failure is fatal + explicit** (message, not just `set -e` + `--exit-on-error`).
- **Missing OR empty critical table is fatal.** 13 core tables checked for existence + non-emptiness.
- **Migration-ledger lineage check:** the restore must be a *prefix* of the live source. A version the
  restore has that the source lacks ⇒ **forked lineage ⇒ FAIL**; versions the source has that the
  restore lacks ⇒ "N migrations behind" (informational — an old dump is expected to lag).
- **Stable structural assertions:** `finalize_fixture` + `enqueue_derive_on_vetted` functions, the
  derive trigger on `news_article_entities`, every critical table's PRIMARY KEY, and the
  `vibe_scores_trigger_type_check` CHECK. Catches `pg_restore` silently dropping objects.
- **Prepared-statement boot check (F-025):** runs the exact `db.New → AfterConnect →
  registerPreparedStatements → Ping` path against the restore via the new kept `go/cmd/validate-stmts`.
  If every statement registers, the restored backend **would boot** — not just hold data.
- **Row-count drift vs the live source is informational**, not fatal (the source moves on after the
  dump — observed `transfer_rumors` +17). Only missing/empty is fatal.
- `SKIP_STMT_CHECK=1` escape hatch for drilling a dump older than the current binary's schema.

### 2. Independent off-disk backup mirror (`scripts/hosting/backup-postgres.sh`)

Postgres data (`/mnt/data/postgres/data`) and the primary backups (`/mnt/data/backup/scoracle`) are
both on **nvme0n1** — one drive failure loses both. No cloud/NAS/USB is configured; the only other
physical drive is the root SSD (`sda`). **Scott's call: mirror off-disk to root now.**

- Each dump is copied to `OFFHOST_BACKUP_DIR` (default `/home/sheneveld/scoracle-offdisk-backup` on
  `sda` — a different physical drive than the NVMe).
- **Same-device guard:** refuses to "mirror" onto the same filesystem as the primary (that would be no
  protection) — warns + skips.
- **Free-space guard:** if the copy would leave less than `MIN_FREE_GB` (5) free, it skips the mirror
  **loudly** rather than fill the OS disk; the primary backup is untouched.
- Tighter mirror retention (`OFFHOST_KEEP_DAYS`=7 daily + day-01 monthlies for a year).
- **Off-DISK, not off-SITE.** Point `OFFHOST_BACKUP_DIR` at a NAS/USB/cloud mount to also survive
  losing the host — the nightly mirror picks it up with no code change. (Pre-launch infra call for Scott.)

### 3. Atomic migration recording (`sql/migrate.sh`)

The old runner applied the migration (`psql -f`) and inserted the ledger row in **two separate psql
processes** — a crash between them stranded a migration applied-but-unrecorded. Now:

- Apply + record run in a **single psql process**.
- Plain-DDL files (no transaction control, no non-transactional statement) are wrapped with
  `--single-transaction` so the DDL **and** the ledger INSERT commit atomically — a crash leaves
  **neither** applied nor recorded.
- `CONCURRENTLY`/`VACUUM` files (e.g. 004, 096) can't be wrapped, so they run autocommit in the one
  process (the cross-process gap is collapsed to a single process).
- Files that self-manage `BEGIN;…COMMIT;` get true atomicity by **self-recording** the INSERT before
  their COMMIT — now a REQUIRED convention (`sql/README-migrations.md` step 4 + new
  `sql/migration_template.sql`). The runner's INSERT is an idempotent `ON CONFLICT` backstop.

### 4. Versioned schema snapshot (`scripts/hosting/snapshot-schema.sh` → `sql/schema/`)

`pg_dump --schema-only` + the applied `schema_migrations` list, committed to the repo. Gives a recovery
path independent of a running prod **and** makes `ledger == live == repo` a diffable artifact (serves
the F-015 / S17 launch gate). Refresh after every migration (README step 5).

### 5. F-015 RESOLVED — the ledger already equals the live schema

The S9 drift was **transient**. Read live (not the files): `088` renamed `vibe_scores→sentiment_scores`;
`093` **reverts** it (`ALTER TABLE IF EXISTS sentiment_scores RENAME TO vibe_scores`) and renames
`vibe_synthesis→sigil_synthesis` + adds `vibe_scores.prompt`; `094` renames `divined_sigil→divined_peak`;
`103` already dropped 088's double-fire trigger. The live schema is **exactly** what 088→093→094→095
as-written produce (verified object-by-object). The only `divined_sigil` string in the whole dump is the
breadcrumb in `divined_peak`'s COMMENT. So the audit's "finish the rename / revert 088" decision is moot —
no heavy rename session needed. The snapshot now proves it.

## Verification

- **Off-disk backup:** ran the hardened script with temp dirs on nvme + sda → 451M dump, **byte-identical
  mirror** on `sda`, both retentions ran, free-space reported. Same-device + free-space guards exercised.
- **Restore drill (happy path) against the OFF-DISK copy:** all 13 critical tables present + non-empty,
  lineage clean (107 applied, 0 behind, no forked versions), structure OK (PK 11/11, indexes 137=137,
  functions 42=42), **prepared statements registered → "the backend would boot."** Exit 0. → the S15
  "done when" (a verified off-host restore can boot the backend).
- **Restore drill (negative):** corrupt dump → `FAIL: pg_restore reported errors` → exit 1 (the original
  `|| true` defect is gone). Dump missing `sigil_synthesis` → `FAIL: critical table missing` → exit 1.
- **migrate.sh atomicity (throwaway DB):** `--single-transaction` failure rolls back **both** the table
  and the ledger row; success records atomically; self-txn records in one process; real `migrate.sh` is a
  clean no-op when every version is already recorded.
- **Build:** `go build ./...`, `go vet ./...`, `gofmt -l` all clean (new `cmd/validate-stmts`).

## Quick reference

| Concern | Command |
| --- | --- |
| Nightly backup + off-disk mirror | `scripts/hosting/backup-postgres.sh` (cron 04:00) |
| True off-site target | `OFFHOST_BACKUP_DIR=/mnt/nas/... scripts/hosting/backup-postgres.sh` |
| Drill a backup (must boot) | `scripts/hosting/restore-drill.sh <dump>` |
| Drill an old dump (skip boot) | `SKIP_STMT_CHECK=1 scripts/hosting/restore-drill.sh <dump>` |
| Validate statements vs a DB | `cd go && go run ./cmd/validate-stmts -db "<url>"` |
| Refresh schema snapshot | `scripts/hosting/snapshot-schema.sh` |
| Apply pending migrations | `DATABASE_PRIVATE_URL=… ./sql/migrate.sh` (next free = **107**) |

## Files

- `scripts/hosting/restore-drill.sh` — rewritten (5-stage bootable-backend proof).
- `scripts/hosting/backup-postgres.sh` — off-disk mirror + same-device/free-space guards.
- `scripts/hosting/snapshot-schema.sh` — **new**; versions `sql/schema/`.
- `sql/migrate.sh` — atomic apply+record (one process / one transaction for plain DDL).
- `sql/migration_template.sql` — **new**; self-recording migration template.
- `sql/README-migrations.md` — atomic-recording + self-record-required + snapshot step.
- `sql/schema/{schema.sql,schema_migrations.txt}` — **new**; versioned snapshot (107 versions).
- `go/cmd/validate-stmts/main.go` — **new**; kept F-025 prepared-statement boot check (feeds S16 CI).

## Not in scope / hand-offs

- **Off-SITE backup** (cloud/NAS) — mechanism is ready (`OFFHOST_BACKUP_DIR`); Scott picks the target
  pre-launch. Today's mirror covers drive failure, not host/site loss. (F-040)
- **`cmd/validate-stmts` → CI** — Session 16 wires it as "prepared statements register against a migrated
  test DB" + a pre-`release.sh` gate. (F-025/F-039)
- **099_team_rosters** is applied + recorded on prod but its file is still untracked (parallel Rust
  session's to commit). Schema captured in the snapshot regardless. (F-041 ops note)
- Still outstanding from prior sessions: F-035 (ollama `OLLAMA_NUM_PARALLEL=1` drop-in, needs sudo),
  F-030 (season-stamp current-season NFL/FOOTBALL Sigils — launch gate).
