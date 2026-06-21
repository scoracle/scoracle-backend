#!/usr/bin/env bash
# Backup restore drill — run this after each schema migration and at
# least once per quarter. An untested backup is not a backup.
#
# Takes a dump path as arg, restores into a throwaway DB, verifies the
# critical backend tables, and reports row-count drift against production.
# Drops the throwaway DB on exit.
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
DB_NAME=${DB_NAME:-scoracle}

if [ -z "${PGPASSWORD:-}" ] && [ -f /home/sheneveld/scoracle/scoracle-backend/.env.local ]; then
    PGPASSWORD=$(grep -oP '(?<=:)[^@/]+(?=@)' /home/sheneveld/scoracle/scoracle-backend/.env.local | head -1)
fi
export PGPASSWORD

cleanup() {
    dropdb -h "$DB_HOST" -U "$DB_USER" --if-exists "$RESTORE_DB" >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "-> creating $RESTORE_DB"
createdb -h "$DB_HOST" -U "$DB_USER" "$RESTORE_DB"

echo "-> restoring $DUMP"
pg_restore -h "$DB_HOST" -U "$DB_USER" -d "$RESTORE_DB" \
    --exit-on-error --no-owner --no-privileges "$DUMP"

echo "-> critical-table and row-count verification"
TABLES=(
    fixtures
    event_box_scores
    event_team_stats
    player_stats
    team_stats
    news_articles
    news_article_entities
    transfer_rumors
    news_summaries
    vibe_scores
    stat_summaries
    sigil_synthesis
    schema_migrations
)
printf '%-28s %12s %12s %s\n' table prod restored delta
printf '%-28s %12s %12s %s\n' ----- ---- -------- -----
for t in "${TABLES[@]}"; do
    PROD=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -v ON_ERROR_STOP=1 -tAc "SELECT count(*) FROM $t")
    REST=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$RESTORE_DB" -v ON_ERROR_STOP=1 -tAc "SELECT count(*) FROM $t")
    if [ "$REST" -eq 0 ]; then
        echo "critical table restored empty: $t" >&2
        exit 1
    fi
    if [ "$PROD" = "$REST" ]; then
        STATUS="ok"
    else
        STATUS="$((PROD - REST))"
    fi
    printf '%-28s %12s %12s %s\n' "$t" "$PROD" "$REST" "$STATUS"
done

RESTORED_MIGRATIONS=$(psql -h "$DB_HOST" -U "$DB_USER" -d "$RESTORE_DB" \
    -v ON_ERROR_STOP=1 -tAc "SELECT count(*) FROM public.schema_migrations")
if [ "$RESTORED_MIGRATIONS" -lt 1 ]; then
    echo "restore has no applied migration history" >&2
    exit 1
fi

echo
echo "-> restore verified. Throwaway DB $RESTORE_DB will be dropped on exit."
