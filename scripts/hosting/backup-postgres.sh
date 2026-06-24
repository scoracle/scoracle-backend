#!/usr/bin/env bash
# Nightly Postgres backup with tiered retention + an independent off-disk mirror.
#
# Primary destination: $BACKUP_DIR (default /mnt/data/backup/scoracle, the KingSpec
# NVMe). NOTE: Postgres's data directory ALSO lives on that NVMe, so the primary
# backup shares a physical drive with the database — it protects against accidental
# drops and bad migrations, but NOT against that drive failing.
#
# Off-disk mirror: $OFFHOST_BACKUP_DIR (default /home/sheneveld/scoracle-offdisk-backup
# on the root SSD — a DIFFERENT physical drive than the NVMe). This survives an NVMe
# failure. It is off-DISK, not off-SITE: point $OFFHOST_BACKUP_DIR at a NAS / USB /
# cloud mount to also survive losing the whole host. Set OFFHOST_BACKUP_DIR="" to
# disable the mirror. The mirror is bounded (short retention + a free-space guard) so
# it can never fill the OS disk.
#
# Override in the crontab if you want a real off-host target:
#   OFFHOST_BACKUP_DIR=/mnt/nas/scoracle /path/to/backup-postgres.sh
#
# Retention (both destinations):
#   - Primary: all daily dumps for 14 days; day-01 monthly snapshots for 1 year.
#   - Mirror:  daily dumps for $OFFHOST_KEEP_DAYS (default 7); day-01 for 1 year.
#
# Restore (verify the mirror, not just the primary):
#   scripts/hosting/restore-drill.sh "$OFFHOST_BACKUP_DIR/scoracle-<date>.dump"

set -euo pipefail

BACKUP_DIR=${BACKUP_DIR:-/mnt/data/backup/scoracle}
OFFHOST_BACKUP_DIR=${OFFHOST_BACKUP_DIR-/home/sheneveld/scoracle-offdisk-backup}
OFFHOST_KEEP_DAYS=${OFFHOST_KEEP_DAYS:-7}
MIN_FREE_GB=${MIN_FREE_GB:-5}
DB_HOST=${DB_HOST:-localhost}
DB_USER=${DB_USER:-scoracle}
DB_NAME=${DB_NAME:-scoracle}

# Load password from .env.local if not already in env.
if [ -z "${PGPASSWORD:-}" ] && [ -f /home/sheneveld/scoracle/scoracle-backend/.env.local ]; then
    PGPASSWORD=$(grep -oP '(?<=:)[^@/]+(?=@)' /home/sheneveld/scoracle/scoracle-backend/.env.local | head -1)
fi
export PGPASSWORD

DATE=$(date -u +%Y%m%dT%H%M%SZ)
FILE="$BACKUP_DIR/scoracle-$DATE.dump"
mkdir -p "$BACKUP_DIR"

echo "[$(date -Iseconds)] dump start -> $FILE"
pg_dump -h "$DB_HOST" -U "$DB_USER" -d "$DB_NAME" -Fc -Z 6 -f "$FILE"
SIZE=$(du -h "$FILE" | cut -f1)
echo "[$(date -Iseconds)] dump done size=$SIZE"

# Prune primary: drop daily dumps older than 14 days EXCEPT day-01 (monthly snaps).
find "$BACKUP_DIR" -name 'scoracle-*T*.dump' -mtime +14 \
    ! -name 'scoracle-??????01T*.dump' -delete
find "$BACKUP_DIR" -name 'scoracle-??????01T*.dump' -mtime +365 -delete
REMAINING=$(find "$BACKUP_DIR" -name 'scoracle-*.dump' | wc -l)
echo "[$(date -Iseconds)] primary retention done files_kept=$REMAINING"

# ---------------------------------------------------------------------------
# Off-disk mirror — an independent copy on a different physical drive.
# ---------------------------------------------------------------------------
if [ -z "$OFFHOST_BACKUP_DIR" ]; then
    echo "[$(date -Iseconds)] WARNING: OFFHOST_BACKUP_DIR is empty — mirror DISABLED. Backups live on the same drive as Postgres; a drive failure loses everything." >&2
    exit 0
fi

mkdir -p "$OFFHOST_BACKUP_DIR"

# Refuse to "mirror" onto the same physical filesystem as the primary — that would
# defeat the entire point (no protection against the drive failing). Warn loudly
# and skip rather than create a false sense of safety.
PRIMARY_DEV=$(stat -c %d "$BACKUP_DIR")
MIRROR_DEV=$(stat -c %d "$OFFHOST_BACKUP_DIR")
if [ "$PRIMARY_DEV" = "$MIRROR_DEV" ]; then
    echo "[$(date -Iseconds)] WARNING: OFFHOST_BACKUP_DIR ($OFFHOST_BACKUP_DIR) is on the SAME filesystem as BACKUP_DIR — not an independent copy. Skipping mirror." >&2
    exit 0
fi

# Free-space guard: never let the mirror fill the OS disk. Skip (don't fail the
# whole backup — the primary already succeeded) if the copy would leave less than
# MIN_FREE_GB free on the mirror's filesystem.
DUMP_KB=$(du -k "$FILE" | cut -f1)
AVAIL_KB=$(df -kP "$OFFHOST_BACKUP_DIR" | awk 'NR==2 {print $4}')
MIN_FREE_KB=$((MIN_FREE_GB * 1024 * 1024))
if [ $((AVAIL_KB - DUMP_KB)) -lt "$MIN_FREE_KB" ]; then
    echo "[$(date -Iseconds)] WARNING: mirror skipped — copying ${DUMP_KB}KB would leave under ${MIN_FREE_GB}GB free on $OFFHOST_BACKUP_DIR (avail=${AVAIL_KB}KB). Primary backup is intact; free space or repoint OFFHOST_BACKUP_DIR." >&2
    exit 0
fi

MIRROR_FILE="$OFFHOST_BACKUP_DIR/$(basename "$FILE")"
echo "[$(date -Iseconds)] mirror copy -> $MIRROR_FILE"
cp "$FILE" "$MIRROR_FILE"

# Prune mirror: tighter daily window than primary (it lives on the smaller OS disk),
# same 1-year day-01 monthly snapshots.
find "$OFFHOST_BACKUP_DIR" -name 'scoracle-*T*.dump' -mtime "+$OFFHOST_KEEP_DAYS" \
    ! -name 'scoracle-??????01T*.dump' -delete
find "$OFFHOST_BACKUP_DIR" -name 'scoracle-??????01T*.dump' -mtime +365 -delete
MIRROR_KEPT=$(find "$OFFHOST_BACKUP_DIR" -name 'scoracle-*.dump' | wc -l)
MIRROR_FREE=$(df -hP "$OFFHOST_BACKUP_DIR" | awk 'NR==2 {print $4}')
echo "[$(date -Iseconds)] mirror retention done files_kept=$MIRROR_KEPT free=$MIRROR_FREE on $OFFHOST_BACKUP_DIR"
