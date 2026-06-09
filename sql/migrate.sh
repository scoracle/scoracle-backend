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

applied="$(psql "$DB" -tAc "SELECT version FROM public.schema_migrations")"
count=0
for f in $(ls "$DIR"/*.sql | sort); do
  v="$(basename "$f" .sql)"
  grep -qxF "$v" <<<"$applied" && continue
  echo ">> applying $v"
  psql "$DB" -v ON_ERROR_STOP=1 -f "$f"
  psql "$DB" -v ON_ERROR_STOP=1 -qc "INSERT INTO public.schema_migrations(version) VALUES ('$v') ON CONFLICT DO NOTHING;"
  count=$((count + 1))
done
echo "done — $count migration(s) applied; schema up to date."
