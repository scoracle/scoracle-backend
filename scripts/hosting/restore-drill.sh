#!/usr/bin/env bash
# Backup restore drill — run this after each schema migration and at
# least once per quarter. An untested backup is not a backup.
#
# Takes a dump path as arg; restores into a throwaway DB and
# compares row counts against production. Drops the throwaway DB on exit.
#
# Usage:
#   scripts/hosting/restore-drill.sh /mnt/backup/scoracle/scoracle-<date>.dump

set -euo pipefail

if [ $# -ne 1 ]; then
    echo "usage: $0 <path-to-dump>" >&2
    exit 2
fi
DUMP=$1
if [ ! -f "$DUMP" ]; then
    echo "dump file not found: $DUMP" >&2
    exit 1
fi

RESTORE_DB="scoracle_restore_drill_$$"
DB_HOST=${DB_HOST:-localhost}
DB_USER=${DB_USER:-scoracle}

if [ -z "${PGPASSWORD:-}" ] && [ -f /home/sheneveld/scoracle-data/.env.local ]; then
    PGPASSWORD=$(grep -oP '(?<=:)[^@/]+(?=@)' /home/sheneveld/scoracle-data/.env.local | head -1)
fi
export PGPASSWORD

cleanup() {
    dropdb -h "$DB_HOST" -U "$DB_USER" --if-exists "$RESTORE_DB" >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "-> creating $RESTORE_DB"
createdb -h "$DB_HOST" -U "$DB_USER" "$RESTORE_DB"

echo "-> restoring $DUMP"
pg_restore -h "$DB_HOST" -U "$DB_USER" -d "$RESTORE_DB" --no-owner --no-privileges "$DUMP" 2>/dev/null || true

echo "-> row-count comparison (prod vs restored)"
TABLES=(news_articles news_article_entities vibe_scores tweets fixtures players teams event_box_scores)
printf '%-28s %12s %12s %s\n' table prod restored delta
printf '%-28s %12s %12s %s\n' ----- ---- -------- -----
for t in "${TABLES[@]}"; do
    PROD=$(psql -h "$DB_HOST" -U "$DB_USER" -d scoracle -tAc "SELECT count(*) FROM $t" 2>/dev/null || echo "n/a")
    REST=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$RESTORE_DB" -tAc "SELECT count(*) FROM $t" 2>/dev/null || echo "n/a")
    if [ "$PROD" = "$REST" ]; then
        STATUS="ok"
    else
        STATUS="DIFF"
    fi
    printf '%-28s %12s %12s %s\n' "$t" "$PROD" "$REST" "$STATUS"
done

echo
echo "-> done. Throwaway DB $RESTORE_DB will be dropped on exit."
