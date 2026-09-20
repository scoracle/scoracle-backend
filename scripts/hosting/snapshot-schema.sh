#!/usr/bin/env bash
# Capture structure, reference data and the source's actual applied ledger from
# one exported PostgreSQL snapshot. Never derive applied history from filenames.
# Usage: snapshot-schema.sh [SOURCE_URL] [OUTPUT_DIRECTORY]
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." >/dev/null && pwd)"
DB="${1:-${DATABASE_PRIVATE_URL:-${DATABASE_URL:-}}}"
[ -n "$DB" ] || { echo "Set DATABASE_PRIVATE_URL/DATABASE_URL or pass a source URL" >&2; exit 1; }
OUT="${2:-$REPO_ROOT/sql/schema}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" >/dev/null && pwd)"
STAGE="$(mktemp -d "$OUT/.capture.XXXXXX")"
trap 'rm -rf "$STAGE"' EXIT
export PGDATABASE="$DB" SCORACLE_CAPTURE_DIR="$STAGE"
cat > "$STAGE/dump.sh" <<'DUMP'
#!/usr/bin/env bash
set -euo pipefail
pg_dump --dbname="$PGDATABASE" --schema-only --section=pre-data --no-owner --no-privileges --snapshot="$SCORACLE_SNAPSHOT" > "$SCORACLE_CAPTURE_DIR/schema.sql"
# Ordered parent-first. These are product configuration, never users, source
# corpora, model products, credentials or transient processing state.
for table in sports leagues provider_seasons stat_definitions rate_modes rating_thresholds stat_templates entity_fact_policy transfer_identity_thresholds stage_routing_subscriptions boxscore_sources; do
  pg_dump --dbname="$PGDATABASE" --data-only --column-inserts --rows-per-insert=100 --no-owner --no-privileges \
    --snapshot="$SCORACLE_SNAPSHOT" --table="public.$table" >> "$SCORACLE_CAPTURE_DIR/reference-data.sql"
done
# Load reference rows before foreign keys/triggers. Sports and leagues have a
# legitimate circular reference through the reporting-clock league.
printf '\n\\ir reference-data.sql\n' >> "$SCORACLE_CAPTURE_DIR/schema.sql"
pg_dump --dbname="$PGDATABASE" --schema-only --section=post-data --no-owner --no-privileges --snapshot="$SCORACLE_SNAPSHOT" >> "$SCORACLE_CAPTURE_DIR/schema.sql"
DUMP
chmod 700 "$STAGE/dump.sh"
export SCORACLE_CAPTURE_HELPER="$STAGE/dump.sh"
# psql keeps this read-only transaction open while its child pg_dump processes
# import the snapshot. ON_ERROR_STOP also covers a failed child command.
psql "$DB" -X -v ON_ERROR_STOP=1 -v capture_dir="$STAGE" <<'SQL'
BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;
SELECT pg_export_snapshot() AS snapshot \gset
\setenv SCORACLE_SNAPSHOT :snapshot
\pset format unaligned
\pset tuples_only on
\set ledger_file :capture_dir '/applied-migrations.txt'
\o :ledger_file
SELECT version FROM public.schema_migrations ORDER BY version;
\o
\! "$SCORACLE_CAPTURE_HELPER"
\if :SHELL_ERROR
SELECT 1/0;
\endif
COMMIT;
SQL
for file in schema.sql reference-data.sql applied-migrations.txt; do
  test -s "$STAGE/$file"
done
# pg_dump adds trailing blank lines; normalize only that formatting before checksumming.
for file in schema.sql reference-data.sql; do
  awk '{line[NR]=$0} END {n=NR; while(n>0 && line[n]=="") n--; for(i=1;i<=n;i++) print line[i]}' "$STAGE/$file" > "$STAGE/trimmed"
  mv "$STAGE/trimmed" "$STAGE/$file"
done
(
  cd "$STAGE"
  if command -v sha256sum >/dev/null; then
    sha256sum schema.sql reference-data.sql applied-migrations.txt > SHA256SUMS
  else
    shasum -a 256 schema.sql reference-data.sql applied-migrations.txt > SHA256SUMS
  fi
)
for file in schema.sql reference-data.sql applied-migrations.txt SHA256SUMS; do
  mv "$STAGE/$file" "$OUT/$file"
done
printf 'Captured verified baseline in %s\n' "$OUT"
