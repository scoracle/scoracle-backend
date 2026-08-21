#!/usr/bin/env bash
# Freshness watchdog — the alarm the Aug 2026 silent-starvation week did not have.
#
# The failure this exists to catch: every part of the machine "runs" (cron
# green, daemon up, API serving) while production is actually dead — articles
# stored but never read (the D-T21 cap collision), voices failing against a
# sick GPU lane (the oMLX memory-pressure week), a queue quietly dead-lettering.
# Each check reads the DATA, not the process list: data freshness is the only
# signal that cannot lie about whether the pipeline produced anything.
#
# Checks (each one line in the log, OK or ALARM):
#   ingest_recency   — newest news_articles.fetched_at within 26h (sweep ran)
#   editor_reads     — per sport: of yesterday's SWEPT TEAMS, the share with at
#                      least one editor_reads row. Team coverage, not article
#                      share: the D-T21 cap deliberately reads only ~10 of a
#                      team's articles a day (NFL sweeps ~70/team, so article
#                      share sits at ~15% BY DESIGN). The failure this hunts is
#                      whole teams at zero — the Aug 7-14 starvation signature.
#                      <80% of teams covered = ALARM.
#   voice_output     — per sport: newest vibe_scores.generated_at within 48h
#                      (the voices are producing, not just queued)
#   packet_compile   — newest packets.compiled_at within 36h (the rail compiles)
#   dead_letters     — pipeline_work failed at the attempt cap (>25 = ALARM)
#   drain_alive      — claimable work exists but NOTHING produced in 30 min
#                      (a dead/wedged daemon; depth alone is recovery, not failure)
#   queue_depth      — claimable count sanity bound (>20k = runaway inflow)
#
# Reporting: one pipeline_runs row per run (job='watchdog'; status failed +
# the alarm lines in error), so `SELECT * FROM pipeline_runs_latest` shows it
# beside the jobs it watches. Non-zero exit on any alarm (cron surfaces it).
# Optional: set WATCHDOG_ALERT_URL in .env.local (e.g. an ntfy.sh topic) and
# alarms are POSTed there as plain text.
#
# Cron: twice daily. Cognition runs continuously since the 2026-08-20
# consolidation (work-driven, no duty cycle — empty queue is the rest window),
# so there is no on-hours constraint; drain_alive only fires when claimable
# work exists but nothing was produced in 30 min, which a running daemon with
# an empty queue never trips:
#   30 8,20 * * * .../cron-watchdog.sh >> .../logs/watchdog.log 2>&1

set -euo pipefail
cd /home/sheneveld/scoracle/scoracle-backend

set -a
# shellcheck source=/dev/null
[ -f .env.local ] && source .env.local
set +a

STAMP="$(date '+%Y-%m-%dT%H:%M:%S%z')"

# One SQL pass; every check emits: name|status|detail.
RESULT="$(psql "$DATABASE_URL" -X -q -A -t -F'|' <<'SQL'
WITH ingest AS (
  SELECT max(fetched_at) AS newest FROM news_articles
),
reads AS (
  -- editor_reads is the one-rail Editor's ledger; news_article_readings was the
  -- legacy rail's (dropped in mig 224). Checking the dead table made this alarm
  -- fire forever on a healthy pipeline (Aug 15-16, 2026). Counted per TEAM, not
  -- per article: the D-T21 cap holds article share at ~15% for high-volume
  -- sports on purpose, but a swept team with ZERO reads is the starvation bug.
  SELECT sport, count(*) AS swept, count(*) FILTER (WHERE read_n > 0) AS read
    FROM (
      SELECT a.raw->>'query_sport' AS sport,
             a.raw->>'query_team_id' AS team,
             count(er.article_id) AS read_n
        FROM news_articles a
        LEFT JOIN editor_reads er ON er.article_id = a.id
       WHERE a.fetched_at BETWEEN now() - interval '36 hours' AND now() - interval '12 hours'
         AND a.raw ? 'query_team_id' AND a.raw ? 'query_sport'
       GROUP BY 1, 2
    ) per_team
   GROUP BY 1
),
vibes AS (
  SELECT sport, max(generated_at) AS newest
    FROM vibe_scores GROUP BY 1
),
pack AS (
  SELECT max(compiled_at) AS newest FROM packets
),
dead AS (
  SELECT count(*) AS n FROM pipeline_work
   WHERE status = 'failed' AND attempts >= 5
),
recent AS (
  SELECT count(*) AS produced FROM cognition_ledger
   WHERE generated_at > now() - interval '30 minutes'
),
claimable AS (
  SELECT count(*) AS n, min(available_at) AS oldest FROM pipeline_work
   WHERE status IN ('pending','failed') AND attempts < 5
     AND available_at < now()
)
SELECT 'ingest_recency',
       CASE WHEN newest > now() - interval '26 hours' THEN 'OK' ELSE 'ALARM' END,
       'newest article ' || coalesce(newest::text, 'none')
  FROM ingest
UNION ALL
SELECT 'editor_reads[' || sport || ']',
       CASE WHEN swept = 0 OR read * 100 >= swept * 80 THEN 'OK' ELSE 'ALARM' END,
       read || '/' || swept || ' swept teams have a read'
  FROM reads
UNION ALL
SELECT 'voice_output[' || sport || ']',
       CASE WHEN newest > now() - interval '48 hours' THEN 'OK' ELSE 'ALARM' END,
       'newest vibe ' || coalesce(newest::text, 'none')
  FROM vibes
UNION ALL
SELECT 'packet_compile',
       CASE WHEN newest > now() - interval '36 hours' THEN 'OK' ELSE 'ALARM' END,
       'newest packet ' || coalesce(newest::text, 'none')
  FROM pack
UNION ALL
SELECT 'dead_letters',
       CASE WHEN n <= 25 THEN 'OK' ELSE 'ALARM' END,
       n || ' at attempt cap'
  FROM dead
UNION ALL
-- A deep queue draining at speed is recovery, not failure (the 08-15 backlog
-- morning): stall means claimable work exists AND nothing was produced in 30
-- minutes — a dead or wedged daemon, whatever the depth.
SELECT 'drain_alive',
       CASE WHEN c.n = 0 OR r.produced > 0 THEN 'OK' ELSE 'ALARM' END,
       r.produced || ' products/30m vs ' || c.n || ' claimable (oldest ' || coalesce(c.oldest::text, 'n/a') || ')'
  FROM claimable c, recent r
UNION ALL
SELECT 'queue_depth',
       CASE WHEN n <= 20000 THEN 'OK' ELSE 'ALARM' END,
       n || ' claimable'
  FROM claimable;
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
psql "$DATABASE_URL" -X -q -c "INSERT INTO pipeline_runs (job, started_at, finished_at, status, attempted, failed, error)
      VALUES ('watchdog', now(), now(), '$STATUS', $CHECKS, $NALARMS, $ERR_SQL);"

if [ "$NALARMS" -gt 0 ]; then
  echo "$STAMP watchdog: $NALARMS alarm(s)"
  if [ -n "${WATCHDOG_ALERT_URL:-}" ]; then
    curl -m 10 -s -o /dev/null -d "scoracle watchdog: $ALARMS" "$WATCHDOG_ALERT_URL" || true
  fi
  exit 1
fi
echo "$STAMP watchdog: all checks OK"
