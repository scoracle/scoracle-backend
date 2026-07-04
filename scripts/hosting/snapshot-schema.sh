#!/usr/bin/env bash
# Version a schema snapshot of live prod into the repo, so there is a recovery path
# (and a reviewable record of the live structure) that does NOT depend on a running
# production database.
#
# Writes:
#   sql/schema/schema.sql            — pg_dump --schema-only (no data, no owners/privs)
#   sql/schema/schema_migrations.txt — the applied migration ledger (one version per line)
#
# Run after every migration (see sql/README-migrations.md) and commit the result with
# the migration, so sql/schema/ always describes the deployed schema. This is the proof
# that "the ledger == the live schema == the repo".
#
# Usage:
#   scripts/hosting/snapshot-schema.sh                 # reads DATABASE_PRIVATE_URL/DATABASE_URL or .env.local
#   scripts/hosting/snapshot-schema.sh "postgres://…"  # explicit source

set -euo pipefail

# `>/dev/null` guards against shells where `cd` is wrapped to echo its target:
# without it the echoed path is captured alongside pwd.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." >/dev/null && pwd)"
OUT_DIR="$REPO_ROOT/sql/schema"

DB="${1:-${DATABASE_PRIVATE_URL:-${DATABASE_URL:-}}}"
if [ -z "$DB" ] && [ -f "$REPO_ROOT/.env.local" ]; then
    DB=$(grep -E '^DATABASE_PRIVATE_URL=' "$REPO_ROOT/.env.local" | head -1 | cut -d= -f2-)
    [ -z "$DB" ] && DB=$(grep -E '^DATABASE_URL=' "$REPO_ROOT/.env.local" | head -1 | cut -d= -f2-)
    DB=${DB%\"}; DB=${DB#\"}   # strip surrounding double quotes if present
fi
[ -n "$DB" ] || { echo "error: set DATABASE_PRIVATE_URL/DATABASE_URL or pass a connection string" >&2; exit 1; }

mkdir -p "$OUT_DIR"

echo "-> dumping schema-only snapshot"
# --schema-only: structure, no rows. --no-owner/--no-privileges: portable to any role.
# Deterministic enough to diff across runs (pg_dump emits objects in a stable order).
pg_dump --schema-only --no-owner --no-privileges "$DB" > "$OUT_DIR/schema.sql"

echo "-> dumping applied migration ledger"
psql "$DB" -v ON_ERROR_STOP=1 -tAc \
    "SELECT version FROM public.schema_migrations ORDER BY version" \
    > "$OUT_DIR/schema_migrations.txt"

N=$(grep -c . "$OUT_DIR/schema_migrations.txt" || true)
LINES=$(wc -l < "$OUT_DIR/schema.sql")
echo "-> wrote sql/schema/schema.sql ($LINES lines) + schema_migrations.txt ($N versions)"
echo "   commit sql/schema/ with the migration that changed the schema."
