#!/usr/bin/env bash
# Cron wrapper for the narrative-graph co-mention refresh (migration 154): recomputes
# narrative_links co_mention edges for every sport from the vetted news rail. Pure SQL,
# set-based, sub-second per sport, no model calls — safe to run any time.
#
# CADENCE IS THE TRAJECTORY BASELINE: each link's heating_up/cooling_off classification
# is its strength delta vs the PREVIOUS refresh (±10 buckets, the shared vocabulary).
# Daily — after the 00:00 ingest sweep has landed and the cognition daemon has had time
# to scrub it — makes trajectory mean day-over-day movement. Running it much more often
# flattens every delta into "stable"; don't.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

set -a
[[ -f .env ]] && source .env
[[ -f .env.local ]] && source .env.local
set +a

DB="${DATABASE_PRIVATE_URL:-${DATABASE_URL:-}}"
if [[ -z "$DB" ]]; then
    echo "cron-narrative-links: no DATABASE_PRIVATE_URL / DATABASE_URL" >&2
    exit 1
fi

# Order matters: refresh links (now-state), seal confirmed outcomes from roster ground
# truth (mig 157 — MUST precede the roll so confirmation beats same-night quiet-seal),
# roll episodes (open/peak/quiet-seal), then re-measure source performance.
exec psql "$DB" -v ON_ERROR_STOP=1 -c "
SELECT 'FOOTBALL' AS sport, now() AS ran_at, * FROM refresh_co_mention_links('FOOTBALL')
UNION ALL
SELECT 'NBA', now(), * FROM refresh_co_mention_links('NBA')
UNION ALL
SELECT 'NFL', now(), * FROM refresh_co_mention_links('NFL')" -c "
SELECT 'FOOTBALL' AS sport, now() AS ran_at, * FROM seal_confirmed_episodes('FOOTBALL')
UNION ALL
SELECT 'NBA', now(), * FROM seal_confirmed_episodes('NBA')
UNION ALL
SELECT 'NFL', now(), * FROM seal_confirmed_episodes('NFL')" -c "
SELECT 'FOOTBALL' AS sport, now() AS ran_at, * FROM roll_narrative_episodes('FOOTBALL')
UNION ALL
SELECT 'NBA', now(), * FROM roll_narrative_episodes('NBA')
UNION ALL
SELECT 'NFL', now(), * FROM roll_narrative_episodes('NFL')" -c "
SELECT 'FOOTBALL' AS sport, now() AS ran_at, refresh_source_performance('FOOTBALL') AS sources
UNION ALL
SELECT 'NBA', now(), refresh_source_performance('NBA')
UNION ALL
SELECT 'NFL', now(), refresh_source_performance('NFL')"
