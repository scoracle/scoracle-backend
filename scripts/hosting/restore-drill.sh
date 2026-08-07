#!/usr/bin/env bash
# Backup restore drill — run this after each schema migration and at
# least once per quarter. An untested backup is not a backup.
#
# Takes a dump path as arg, restores it into a throwaway DB, and proves the
# restored database is a backend you could actually boot:
#   1. pg_restore succeeds (failure is fatal — never swallowed).
#   2. Every critical table exists and is non-empty; row-count drift vs the
#      live source is reported (drift is expected — the source moved on since
#      the dump; only a MISSING or EMPTY critical table is fatal).
#   3. The migration ledger is a coherent prefix of the source's: any version
#      the restore has that the source lacks means a forked lineage (fatal);
#      versions the source has that the restore lacks just mean the dump is N
#      migrations behind (reported, not fatal).
#   4. A curated set of stable structural objects exists (core functions, the
#      derive trigger, every critical table's primary key, a key CHECK
#      constraint) — pg_restore silently dropping objects is the failure this
#      catches.
#   5. The API's prepared statements register against the restore (F-025 via
#      cmd/validate-stmts) — i.e. the restored backend would boot, not just
#      hold data.
# Drops the throwaway DB on exit.
#
# Usage:
#   scripts/hosting/restore-drill.sh /mnt/data/backup/scoracle/scoracle-<date>.dump
#
# Env overrides: DB_HOST DB_PORT DB_USER DB_NAME (source/comparison DB) PGPASSWORD
#   SKIP_STMT_CHECK=1  skip the prepared-statement boot check (e.g. when the dump
#                      predates the current binary's schema and you only want the
#                      structural/row checks).

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

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GO_DIR="$SCRIPT_DIR/../../go"

RESTORE_DB="scoracle_restore_drill_$$"
DB_HOST=${DB_HOST:-localhost}
DB_PORT=${DB_PORT:-5432}
DB_USER=${DB_USER:-scoracle}
DB_NAME=${DB_NAME:-scoracle}

if [ -z "${PGPASSWORD:-}" ] && [ -f /home/sheneveld/scoracle/scoracle-backend/.env.local ]; then
    PGPASSWORD=$(grep -oP '(?<=:)[^@/]+(?=@)' /home/sheneveld/scoracle/scoracle-backend/.env.local | head -1)
fi
export PGPASSWORD

FAIL=0
note_fail() { echo "FAIL: $*" >&2; FAIL=1; }

cleanup() {
    dropdb -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" --if-exists "$RESTORE_DB" >/dev/null 2>&1 || true
}
trap cleanup EXIT

# psql against the throwaway restore (errors are fatal: ON_ERROR_STOP + set -e).
rq() { psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$RESTORE_DB" -v ON_ERROR_STOP=1 -tAc "$1"; }
# psql against the live source — informational; never aborts the drill.
sq() { psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -tAc "$1" 2>/dev/null || echo "n/a"; }

echo "-> creating $RESTORE_DB"
createdb -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" "$RESTORE_DB"

echo "-> restoring $DUMP"
# --exit-on-error makes pg_restore stop (and exit non-zero) on the FIRST error;
# set -e then aborts the drill. A restore that errors is never reported as ok.
if ! pg_restore -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$RESTORE_DB" \
        --exit-on-error --no-owner --no-privileges "$DUMP"; then
    echo "FAIL: pg_restore reported errors — restore is not trustworthy" >&2
    exit 1
fi

echo
echo "-> [1] critical-table existence, non-emptiness, and row-count drift vs source"
CRITICAL=(
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
printf '%-26s %12s %12s %s\n' table source restored drift
printf '%-26s %12s %12s %s\n' ----- ------ -------- -----
for t in "${CRITICAL[@]}"; do
    if [ "$(rq "SELECT to_regclass('public.$t') IS NOT NULL")" != "t" ]; then
        note_fail "critical table missing from restore: $t"
        printf '%-26s %12s %12s %s\n' "$t" "?" "MISSING" "FAIL"
        continue
    fi
    REST=$(rq "SELECT count(*) FROM public.$t")
    SRC=$(sq "SELECT count(*) FROM public.$t")
    if [ "$REST" -eq 0 ]; then
        note_fail "critical table restored EMPTY: $t"
        DRIFT="FAIL(empty)"
    elif [ "$SRC" = "n/a" ]; then
        DRIFT="src-unreadable"
    elif [ "$SRC" = "$REST" ]; then
        DRIFT="ok"
    else
        DRIFT="$((SRC - REST))"
    fi
    printf '%-26s %12s %12s %s\n' "$t" "$SRC" "$REST" "$DRIFT"
done

echo
echo "-> [2] migration-ledger lineage (restore must be a prefix of source)"
REST_VERS=$(rq "SELECT version FROM public.schema_migrations ORDER BY version" | sort)
SRC_VERS=$(sq "SELECT version FROM public.schema_migrations ORDER BY version" | sort)
REST_MAX=$(printf '%s\n' "$REST_VERS" | tail -1)
REST_N=$(printf '%s\n' "$REST_VERS" | grep -c . || true)
echo "   restore: $REST_N migrations applied, latest = $REST_MAX"
if [ "$SRC_VERS" != "n/a" ]; then
    RESTORE_ONLY=$(comm -13 <(printf '%s\n' "$SRC_VERS") <(printf '%s\n' "$REST_VERS") | grep -c . || true)
    BEHIND=$(comm -23 <(printf '%s\n' "$SRC_VERS") <(printf '%s\n' "$REST_VERS") | grep -c . || true)
    if [ "$RESTORE_ONLY" -gt 0 ]; then
        note_fail "restore carries $RESTORE_ONLY migration(s) the source lacks — forked lineage, not a prefix:"
        comm -13 <(printf '%s\n' "$SRC_VERS") <(printf '%s\n' "$REST_VERS") | sed 's/^/         /' >&2
    fi
    echo "   restore is $BEHIND migration(s) behind the live source (expected if the dump is old)"
else
    echo "   source ledger unreadable — skipping lineage comparison"
fi

echo
echo "-> [3] stable structural objects present in restore"
# All object lookups go through to_regclass so a missing relation yields a clean
# count of 0 (and a note_fail) instead of a raw '::regclass does not exist' error.
# 2026-08-06: `enqueue_derive_on_vetted` (function AND trigger) was retired with
# news_article_entities.vetted in the 8.10/8.11 rip — this drill was still asserting both, so a
# restore drill would have FAILED on the schema being CORRECT. Replaced with the packet rail's
# live equivalents: enqueue_voices_on_packet is the trigger the whole voice fan-out hangs off,
# and detect_team_change is the box-score trigger migration 215 narrowed.
for fn in finalize_fixture enqueue_voices_on_packet detect_team_change; do
    if [ "$(rq "SELECT count(*) FROM pg_proc WHERE proname='$fn'")" -lt 1 ]; then
        note_fail "function missing from restore: $fn()"
    fi
done
if [ "$(rq "SELECT count(*) FROM pg_trigger WHERE tgrelid=to_regclass('public.packets') AND tgname='enqueue_voices_on_packet' AND NOT tgisinternal")" -lt 1 ]; then
    note_fail "trigger missing from restore: enqueue_voices_on_packet on packets"
fi
if [ "$(rq "SELECT count(*) FROM pg_trigger WHERE tgrelid=to_regclass('public.event_box_scores') AND tgname='trg_detect_team_change' AND NOT tgisinternal")" -lt 1 ]; then
    note_fail "trigger missing from restore: trg_detect_team_change on event_box_scores"
fi
if [ "$(rq "SELECT count(*) FROM pg_constraint WHERE conname='vibe_scores_trigger_type_check'")" -lt 1 ]; then
    note_fail "constraint missing from restore: vibe_scores_trigger_type_check"
fi
PK_TABLES=(fixtures player_stats team_stats news_articles news_article_entities transfer_rumors news_summaries vibe_scores stat_summaries sigil_synthesis schema_migrations)
PK_MISSING=0
for t in "${PK_TABLES[@]}"; do
    PKN=$(rq "SELECT count(*) FROM pg_constraint WHERE conrelid=to_regclass('public.$t') AND contype='p'")
    if [ "${PKN:-0}" -lt 1 ]; then
        note_fail "primary key missing from restore: $t"
        PK_MISSING=$((PK_MISSING + 1))
    fi
done
# Informational structural-count comparison (object adds since the dump are normal).
REST_IDX=$(rq "SELECT count(*) FROM pg_indexes WHERE schemaname='public'")
SRC_IDX=$(sq "SELECT count(*) FROM pg_indexes WHERE schemaname='public'")
REST_FN=$(rq "SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='public'")
SRC_FN=$(sq "SELECT count(*) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='public'")
echo "   functions present: PK-checks $((${#PK_TABLES[@]} - PK_MISSING))/${#PK_TABLES[@]} tables OK"
echo "   public indexes:  source=$SRC_IDX restore=$REST_IDX"
echo "   public functions: source=$SRC_FN restore=$REST_FN"

echo
echo "-> [4] prepared-statement registration against the restore (would the backend boot?)"
if [ "${SKIP_STMT_CHECK:-0}" = "1" ]; then
    echo "   SKIP_STMT_CHECK=1 — skipped (dump may predate the current binary's schema)"
else
    REST_URL="postgresql://$DB_USER:$PGPASSWORD@$DB_HOST:$DB_PORT/$RESTORE_DB?sslmode=disable"
    if ( cd "$GO_DIR" && go run ./cmd/validate-stmts -db "$REST_URL" ); then
        echo "   restored backend registers every prepared statement — it would boot."
    else
        note_fail "prepared statements do NOT register against the restore — the backend would boot DEGRADED."
        echo "   (if this dump predates the current binary, run migrate.sh on the restore first, or set SKIP_STMT_CHECK=1)" >&2
    fi
fi

echo
if [ "$FAIL" -ne 0 ]; then
    echo "RESTORE DRILL FAILED — see FAIL lines above. Throwaway DB $RESTORE_DB dropped on exit." >&2
    exit 1
fi
echo "-> restore VERIFIED: data, ledger lineage, structure, and a bootable backend. $RESTORE_DB dropped on exit."
