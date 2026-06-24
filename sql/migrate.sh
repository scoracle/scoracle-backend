#!/usr/bin/env bash
# Forward migration runner. Applies every migration not yet in public.schema_migrations,
# in lexical filename order, recording each on success. Safe to re-run (applied ones are
# skipped). This is the PROD / incremental path.
#
# Fresh environments (sandbox.scoracle, fantasy.scoracle, a dev clone): do NOT replay
# migrations here — some carry data-dependent gates (e.g. 045/046/048 smoke checks) that
# fail on an empty DB. Use sql/build.sh, which clones the prod schema (and schema_migrations
# with it) so this runner stays incremental from that point.
#
# Atomic recording: the migration SQL and its schema_migrations INSERT are issued in a
# SINGLE psql process (never two), so a crash can no longer leave a migration
# applied-but-unrecorded across a process boundary. For files that don't manage their own
# transaction (and don't use a non-transactional statement like CONCURRENTLY), the INSERT
# is wrapped INTO the same transaction as the DDL (--single-transaction) → genuinely atomic:
# a crash leaves neither applied nor recorded. Files that self-manage a transaction get true
# atomicity by self-recording the INSERT before their own COMMIT (see sql/README-migrations.md
# and sql/migrations/_TEMPLATE.sql); the runner's INSERT is then an idempotent backstop.
#
# Usage:
#   ./sql/migrate.sh                 # reads $DATABASE_PRIVATE_URL or $DATABASE_URL
#   ./sql/migrate.sh "postgres://…"  # explicit connection string
set -euo pipefail

DB="${1:-${DATABASE_PRIVATE_URL:-${DATABASE_URL:-}}}"
[ -n "$DB" ] || { echo "error: set DATABASE_PRIVATE_URL/DATABASE_URL or pass a connection string"; exit 1; }
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/migrations"

# Guard: the tracking table must already exist. Bootstrap it ONCE (migration 051) so we
# never start from an empty table on a prod DB and re-replay 001+ over live data.
exists="$(psql "$DB" -tAc "SELECT to_regclass('public.schema_migrations') IS NOT NULL")"
if [ "$exists" != "t" ]; then
  echo "error: public.schema_migrations is missing — bootstrap once, then re-run:"
  echo "  psql \"\$DATABASE_PRIVATE_URL\" -f $DIR/051_schema_migrations.sql"
  exit 1
fi

# Statements that cannot run inside a transaction block (so the DDL + ledger INSERT
# cannot be wrapped together — they run in autocommit, one psql process).
NONTXN_RE='CONCURRENTLY|VACUUM|REINDEX|ALTER[[:space:]]+SYSTEM|CREATE[[:space:]]+DATABASE|ADD[[:space:]]+VALUE'

applied="$(psql "$DB" -tAc "SELECT version FROM public.schema_migrations")"
count=0
for f in $(ls "$DIR"/*.sql | sort); do
  v="$(basename "$f" .sql)"
  grep -qxF "$v" <<<"$applied" && continue
  ledger="INSERT INTO public.schema_migrations(version) VALUES ('$v') ON CONFLICT DO NOTHING;"
  if grep -qiE '^[[:space:]]*BEGIN[[:space:]]*;' "$f"; then
    # File manages its own transaction(s). Apply + record in ONE psql session so the
    # ledger INSERT runs immediately after the file's COMMIT in the same connection —
    # no second process. True atomicity requires the migration to self-record before
    # its own COMMIT (convention); this INSERT is the idempotent backstop.
    mode="self-txn"
    psql "$DB" -v ON_ERROR_STOP=1 -f "$f" -c "$ledger"
  elif grep -qiE "$NONTXN_RE" "$f"; then
    # Contains a non-transactional statement (CONCURRENTLY, etc.) — cannot wrap. One
    # psql session collapses the apply→record gap to a single process anyway.
    mode="autocommit(non-txn)"
    psql "$DB" -v ON_ERROR_STOP=1 -f "$f" -c "$ledger"
  else
    # Plain DDL with no transaction control: wrap the file AND the ledger INSERT in a
    # single transaction so they commit atomically — a crash leaves NEITHER applied nor
    # recorded (never the ambiguous applied-but-unrecorded state).
    mode="single-transaction"
    psql "$DB" -v ON_ERROR_STOP=1 --single-transaction -f "$f" -c "$ledger"
  fi
  echo ">> applied $v [$mode]"
  count=$((count + 1))
done
echo "done — $count migration(s) applied; schema up to date."
