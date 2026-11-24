#!/usr/bin/env bash
# Data freshness and durable Classifier work. A deliberate pause is not starvation.
# WATCHDOG_MODE=paused|classifier overrides detection for a remote worker.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
set -a
[ ! -f .env.local ] || source .env.local
set +a
LIVE=false
if [ "${1:-}" = --report ]; then LIVE=true; shift; fi
DB="${1:-${DATABASE_PRIVATE_URL:-${DATABASE_URL:-}}}"
[ -n "$DB" ] || { echo "watchdog: database URL required" >&2; exit 1; }
MODE="${WATCHDOG_MODE:-}"
if [ -z "$MODE" ]; then
    MODE=paused
    if systemctl --user -q is-active scoracle-cognition.service; then MODE=classifier; fi
fi
case "$MODE" in paused|classifier) ;; *) echo "watchdog: invalid WATCHDOG_MODE=$MODE" >&2; exit 2 ;; esac
STAMP="$(date '+%Y-%m-%dT%H:%M:%S%z')"
query=(psql "$DB" -X -qAt -F'|' -v ON_ERROR_STOP=1 -v mode="$MODE"
    -v stages="${COGNITION_STAGES-classifier,investigate_entity,fixture_boxscore,rating,vibe,narratives,transfers,momentum,sigil}"
    -v live="$LIVE" -f "$ROOT/scripts/hosting/watchdog.sql")
if [ "$LIVE" = true ]; then
    exec "${query[@]}"
fi
RESULT="$("${query[@]}")"

echo "$RESULT" | while IFS='|' read -r name status detail; do
  [ -n "$name" ] && echo "$STAMP watchdog $status $name: $detail"
done

ALARMS="$(echo "$RESULT" | awk -F'|' '$2 == "ALARM" { print $1 ": " $3 }')"
CHECKS="$(echo "$RESULT" | grep -c '|' || true)"
NALARMS="$(printf '%s' "$ALARMS" | grep -c . || true)"

# The run lands beside the jobs it watches (pipeline_runs_latest).
STATUS=success
if [ "$NALARMS" -gt 0 ]; then STATUS=failed; fi
CHECKS=$((CHECKS + 1))
OUTPUT_REPORT="$(psql "$DB" -X -qAt -v ON_ERROR_STOP=1 -v status="$STATUS" -v checks="$CHECKS" -v alarms="$NALARMS" -v detail="$ALARMS" <<'SQL'
BEGIN;
INSERT INTO pipeline_runs (job, started_at, finished_at, status, attempted, failed, error)
VALUES ('watchdog',now(),now(),:'status',:checks,:alarms,NULLIF(:'detail','')) RETURNING id AS run_id \gset
SELECT public.collect_character_output_report(:run_id);
COMMIT;
SQL
)"
printf '%s watchdog INFO character_outputs: %s\n' "$STAMP" "$OUTPUT_REPORT"

if [ "$NALARMS" -gt 0 ]; then
  echo "$STAMP watchdog: $NALARMS alarm(s)"
  if [ -n "${WATCHDOG_ALERT_URL:-}" ]; then
    curl -m 10 -s -o /dev/null -d "scoracle watchdog: $ALARMS" "$WATCHDOG_ALERT_URL" || true
  fi
  exit 1
fi
echo "$STAMP watchdog: all checks OK"
