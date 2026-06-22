#!/usr/bin/env bash
# Current-season-aware polling wrapper for live fixture ingestion.
#
# Jobs:
#   nba-refresh       bounded NBA schedule refresh
#   nba-process       completed NBA fixture drain
#   nfl-refresh       bounded NFL schedule refresh
#   nfl-process       completed NFL fixture drain
#   football-refresh  weekly football schedule refresh
#   football-process  daily football completed-fixture drain
#   football-meta     weekly football roster/meta refresh
#
# The wrapper resolves sports.current_season from Postgres at runtime so cron
# never hardcodes rollover-sensitive season values. NBA/NFL schedule refreshes
# stay window-bounded to avoid reloading an entire season on every tick.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." >/dev/null && pwd)"
cd "$REPO_ROOT"

# shellcheck source=/dev/null
source .venv/bin/activate

set -a
# shellcheck source=/dev/null
[ -f .env ] && source .env
# shellcheck source=/dev/null
[ -f .env.local ] && source .env.local
set +a

JOB="${1:-}"
case "$JOB" in
    nba-refresh|nba-process|nfl-refresh|nfl-process|football-refresh|football-process|football-meta)
        ;;
    "")
        echo "usage: $0 <nba-refresh|nba-process|nfl-refresh|nfl-process|football-refresh|football-process|football-meta>" >&2
        exit 2
        ;;
    *)
        echo "cron-live-fixtures.sh: unknown job '$JOB'" >&2
        exit 2
        ;;
esac

DB="${DATABASE_PRIVATE_URL:-${DATABASE_URL:-}}"
if [ -z "$DB" ]; then
    echo "cron-live-fixtures.sh: DATABASE_PRIVATE_URL or DATABASE_URL must be set" >&2
    exit 1
fi

PSQL_BIN="${SCORACLE_PSQL_BIN:-psql}"
SEED_BIN="${SCORACLE_SEED_BIN:-scoracle-seed}"
FLOCK_BIN="${SCORACLE_FLOCK_BIN:-flock}"
TODAY="${SCORACLE_TODAY:-$(date +%F)}"

# NBA and NFL share a BDL key, while the client's throttle is process-local.
# Serialize those jobs so overlapping cron ticks cannot multiply request rate.
if [[ "$JOB" == nba-* || "$JOB" == nfl-* ]]; then
    LOCK_DIR="${SCORACLE_LOCK_DIR:-$REPO_ROOT/logs}"
    mkdir -p "$LOCK_DIR"
    LOCK_FILE="$LOCK_DIR/live-bdl.lock"
    exec 9>"$LOCK_FILE"
    if ! "$FLOCK_BIN" -n 9; then
        echo "[$(date -Iseconds)] $JOB skipped: another BDL ingestion job is running"
        exit 0
    fi
fi

current_season() {
    local sport="$1"
    local season

    season="$(
        "$PSQL_BIN" "$DB" -v ON_ERROR_STOP=1 -Atqc \
            "SELECT current_season FROM public.sports WHERE id = '${sport}';"
    )"
    if [[ ! "$season" =~ ^[0-9]{4}$ ]]; then
        echo "cron-live-fixtures.sh: invalid current season for $sport: '${season}'" >&2
        return 1
    fi
    printf '%s\n' "$season"
}

date_shift() {
    local base="$1" delta="$2"
    date -d "${base} ${delta} day" +%F
}

run_seed() {
    echo "[$(date -Iseconds)] $SEED_BIN $*"
    "$SEED_BIN" "$@"
}

case "$JOB" in
    nba-refresh)
        season="$(current_season NBA)"
        from_date="$(date_shift "$TODAY" "${NBA_REFRESH_DAYS_BACK:--1}")"
        to_date="$(date_shift "$TODAY" "${NBA_REFRESH_DAYS_FORWARD:-10}")"
        echo "[$(date -Iseconds)] nba-refresh season=$season window=${from_date}..${to_date}"
        run_seed event load-fixtures nba --season "$season" --from-date "$from_date" --to-date "$to_date"
        ;;
    nba-process)
        season="$(current_season NBA)"
        echo "[$(date -Iseconds)] nba-process season=$season max=${NBA_PROCESS_MAX:-24}"
        run_seed event process --sport nba --season "$season" --max "${NBA_PROCESS_MAX:-24}"
        ;;
    nfl-refresh)
        season="$(current_season NFL)"
        from_date="$(date_shift "$TODAY" "${NFL_REFRESH_DAYS_BACK:--3}")"
        to_date="$(date_shift "$TODAY" "${NFL_REFRESH_DAYS_FORWARD:-21}")"
        echo "[$(date -Iseconds)] nfl-refresh season=$season window=${from_date}..${to_date}"
        run_seed event load-fixtures nfl --season "$season" --from-date "$from_date" --to-date "$to_date"
        ;;
    nfl-process)
        season="$(current_season NFL)"
        echo "[$(date -Iseconds)] nfl-process season=$season max=${NFL_PROCESS_MAX:-20}"
        run_seed event process --sport nfl --season "$season" --max "${NFL_PROCESS_MAX:-20}"
        ;;
    football-refresh)
        season="$(current_season FOOTBALL)"
        echo "[$(date -Iseconds)] football-refresh season=$season"
        run_seed event load-fixtures football --season "$season"
        ;;
    football-process)
        season="$(current_season FOOTBALL)"
        echo "[$(date -Iseconds)] football-process season=$season"
        run_seed event process --sport football --season "$season"
        ;;
    football-meta)
        season="$(current_season FOOTBALL)"
        echo "[$(date -Iseconds)] football-meta season=$season"
        run_seed meta seed football --season "$season"
        ;;
esac
