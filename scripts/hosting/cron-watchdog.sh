#!/usr/bin/env bash
# Data freshness and durable Classifier work. A deliberate pause is not starvation.
# WATCHDOG_MODE=paused|acquire|classifier overrides detection for a remote worker.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
set -a
[ ! -f .env.local ] || source .env.local
set +a
DB="${1:-${DATABASE_PRIVATE_URL:-${DATABASE_URL:-}}}"
[ -n "$DB" ] || { echo "watchdog: database URL required" >&2; exit 1; }
MODE="${WATCHDOG_MODE:-}"
if [ -z "$MODE" ]; then
    MODE=paused
    if systemctl --user -q is-active scoracle-classifier-source.service; then MODE=acquire; fi
    if systemctl --user -q is-active scoracle-cognition.service; then MODE=classifier; fi
fi
case "$MODE" in paused|acquire|classifier) ;; *) echo "watchdog: invalid WATCHDOG_MODE=$MODE" >&2; exit 2 ;; esac
STAMP="$(date '+%Y-%m-%dT%H:%M:%S%z')"
RESULT="$(psql "$DB" -X -q -A -t -F'|' -v ON_ERROR_STOP=1 -v mode="$MODE" <<'SQL'
WITH active_work AS (
    SELECT * FROM pipeline_work WHERE
      (:'mode'='acquire' AND stage='classifier_acquire') OR
      (:'mode'='classifier' AND stage IN ('classifier_acquire','classifier','graph','investigate_entity',
        'fixture_boxscore','rating','vibe','narratives','transfers','momentum','sigil'))
), queue AS (
    SELECT count(*) FILTER(WHERE status IN ('pending','failed') AND attempts<5 AND available_at<=now()) AS ready,
        count(*) FILTER(WHERE status='failed' AND attempts>=5) AS dead,
        count(*) FILTER(WHERE status='running' AND updated_at<now()-interval '2 hours') AS stuck
    FROM active_work
), progress AS (
    SELECT GREATEST((SELECT max(created_at) FROM classifier_sources),
        (SELECT max(created_at) FROM classifier_measurements),
        (SELECT max(generated_at) FROM cognition_ledger)) AS newest
)
SELECT 'ingest_recency',CASE WHEN max(fetched_at)>now()-interval '26 hours' THEN 'OK' ELSE 'ALARM' END,
    'newest discovery '||coalesce(max(fetched_at)::text,'none') FROM news_articles
UNION ALL SELECT 'worker_mode','INFO',:'mode'
UNION ALL SELECT 'queue_depth','INFO',ready||' claimable; retained backlog is expected' FROM queue
UNION ALL SELECT 'held_deliveries','INFO',count(*)||' held; no automatic policy promotion'
    FROM classifier_deliveries WHERE status='held'
UNION ALL SELECT 'dead_letters',CASE WHEN dead=0 THEN 'OK' ELSE 'ALARM' END,dead||' active-stage rows at attempt cap' FROM queue
UNION ALL SELECT 'stuck_running',CASE WHEN stuck=0 THEN 'OK' ELSE 'ALARM' END,stuck||' active claims older than 2 hours' FROM queue
UNION ALL SELECT 'drain_alive',CASE WHEN ready=0 OR newest>now()-interval '30 minutes' THEN 'OK' ELSE 'ALARM' END,
    ready||' claimable; newest progress '||coalesce(newest::text,'none') FROM queue,progress WHERE :'mode'<>'paused';
SQL
)"

echo "$RESULT" | while IFS='|' read -r name status detail; do
  [ -n "$name" ] && echo "$STAMP watchdog $status $name: $detail"
done

ALARMS="$(echo "$RESULT" | awk -F'|' '$2 == "ALARM" { print $1 ": " $3 }')"
CHECKS="$(echo "$RESULT" | grep -c '|' || true)"
NALARMS="$(printf '%s' "$ALARMS" | grep -c . || true)"

# The run lands beside the jobs it watches (pipeline_runs_latest).
STATUS=success
ERR_SQL=NULL
if [ "$NALARMS" -gt 0 ]; then
  STATUS=failed
  ERR_SQL="'$(echo "$ALARMS" | tr '\n' ';' | sed "s/'/''/g")'"
fi
psql "$DB" -X -q -c "INSERT INTO pipeline_runs (job, started_at, finished_at, status, attempted, failed, error)
      VALUES ('watchdog', now(), now(), '$STATUS', $CHECKS, $NALARMS, $ERR_SQL);"

if [ "$NALARMS" -gt 0 ]; then
  echo "$STAMP watchdog: $NALARMS alarm(s)"
  if [ -n "${WATCHDOG_ALERT_URL:-}" ]; then
    curl -m 10 -s -o /dev/null -d "scoracle watchdog: $ALARMS" "$WATCHDOG_ALERT_URL" || true
  fi
  exit 1
fi
echo "$STAMP watchdog: all checks OK"
