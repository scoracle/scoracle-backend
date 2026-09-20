#!/usr/bin/env bash
# Restore the checked-in baseline into an empty database. No live source needed.
# Usage: sql/build.sh TARGET_URL [BASELINE_DIRECTORY]
set -euo pipefail
TGT="${1:?target empty database URL required}"
BASE="${2:-$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null && pwd)/schema}"
BASE="$(cd "$BASE" >/dev/null && pwd)"
(
  cd "$BASE"
  if command -v sha256sum >/dev/null; then sha256sum -c SHA256SUMS; else shasum -a 256 -c SHA256SUMS; fi
)
empty="$(psql "$TGT" -X -v ON_ERROR_STOP=1 -Atc "SELECT NOT EXISTS(SELECT 1 FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname NOT IN ('pg_catalog','information_schema') AND n.nspname NOT LIKE 'pg_toast%' AND c.relkind IN ('r','p','v','m'))")"
[ "$empty" = t ] || { echo "Target contains relations; refusing to overwrite it" >&2; exit 1; }
# The generated RLS policies need this non-login role. Use a database bootstrap
# administrator; this role and the restore commit together.
psql "$TGT" -X -v ON_ERROR_STOP=1 --single-transaction \
  -c 'DO $$ BEGIN IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = '\''web_user'\'') THEN CREATE ROLE web_user NOLOGIN; END IF; END $$;' \
  -f "$BASE/schema.sql" \
  -c "\copy public.schema_migrations(version) FROM '$BASE/applied-migrations.txt'"
printf 'Baseline restored. Apply later changes with sql/migrate.sh.\n'
