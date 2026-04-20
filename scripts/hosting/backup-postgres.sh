#!/usr/bin/env bash
# Nightly Postgres backup with tiered retention.
#
# Destination: $BACKUP_DIR (default /mnt/backup/scoracle).
# Override with an env var in the crontab if you want cloud:
#   BACKUP_DIR=/some/mount /path/to/backup-postgres.sh
#
# Retention:
#   - All daily dumps for 14 days
#   - Dumps taken on day 1 of the month kept for 1 year (rough monthly snapshots)
#
# Restore with:
#   createdb scoracle_restore
#   pg_restore -h localhost -U scoracle -d scoracle_restore <dump>
#   dropdb scoracle_restore

set -euo pipefail

# Default target is the KingSpec NVMe at /mnt/data/backup/scoracle.
# Same-disk-as-Postgres caveat: useful for snapshots + accidental drops
# but does not protect against drive failure. Plan off-disk (USB / cloud
# / NAS) before the corpus has irreplaceable value.
BACKUP_DIR=${BACKUP_DIR:-/mnt/data/backup/scoracle}
DB_HOST=${DB_HOST:-localhost}
DB_USER=${DB_USER:-scoracle}
DB_NAME=${DB_NAME:-scoracle}

# Load password from .env.local if not already in env.
if [ -z "${PGPASSWORD:-}" ] && [ -f /home/sheneveld/scoracle-data/.env.local ]; then
    # Extract DATABASE_URL or fall back to the static dev password.
    # We use PGPASSWORD rather than parsing the URL so pg_dump stays simple.
    PGPASSWORD=$(grep -oP '(?<=:)[^@/]+(?=@)' /home/sheneveld/scoracle-data/.env.local | head -1)
fi
export PGPASSWORD

DATE=$(date -u +%Y%m%dT%H%M%SZ)
FILE="$BACKUP_DIR/scoracle-$DATE.dump"
mkdir -p "$BACKUP_DIR"

echo "[$(date -Iseconds)] dump start -> $FILE"
pg_dump -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -Fc -Z 6 -f "$FILE"
SIZE=$(du -h "$FILE" | cut -f1)
echo "[$(date -Iseconds)] dump done size=$SIZE"

# Prune: drop daily dumps older than 14 days EXCEPT those taken on day 01
# of the month (monthly snapshots).
find "$BACKUP_DIR" -name 'scoracle-*T*.dump' -mtime +14 \
    ! -name 'scoracle-??????01T*.dump' -delete

# Prune: drop monthly snapshots older than 365 days.
find "$BACKUP_DIR" -name 'scoracle-??????01T*.dump' -mtime +365 -delete

REMAINING=$(find "$BACKUP_DIR" -name 'scoracle-*.dump' | wc -l)
echo "[$(date -Iseconds)] retention done files_kept=$REMAINING"
