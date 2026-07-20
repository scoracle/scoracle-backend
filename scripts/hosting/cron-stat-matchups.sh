#!/usr/bin/env bash
# Cron wrapper for the stat-matchup refresh (migration 156): recomputes pairwise
# player-vs-team performance edges (stat_matchups) from event_box_scores. Pure SQL,
# set-based, no model calls; full recompute per (sport, stat) is seconds. Stats only
# move on game days, so offseason runs are effectively no-ops — safe to run nightly.
#
# Stat list is the serving surface: add a line when a new stat should feed the
# enrichment layer. Count stats only (absent key = real zero); minutes floor keeps
# cameos out of baselines (NBA 10, FOOTBALL 30).

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

set -a
[[ -f .env ]] && source .env
[[ -f .env.local ]] && source .env.local
set +a

DB="${DATABASE_PRIVATE_URL:-${DATABASE_URL:-}}"
if [[ -z "$DB" ]]; then
    echo "cron-stat-matchups: no DATABASE_PRIVATE_URL / DATABASE_URL" >&2
    exit 1
fi

exec psql "$DB" -v ON_ERROR_STOP=1 -c "
SELECT 'NBA/reb' AS run, now() AS ran_at, * FROM refresh_stat_matchups('NBA','reb')
UNION ALL SELECT 'NBA/pts', now(), * FROM refresh_stat_matchups('NBA','pts')
UNION ALL SELECT 'NBA/ast', now(), * FROM refresh_stat_matchups('NBA','ast')
UNION ALL SELECT 'FOOTBALL/goals', now(), * FROM refresh_stat_matchups('FOOTBALL','goals','career',5,30,10)
UNION ALL SELECT 'FOOTBALL/assists', now(), * FROM refresh_stat_matchups('FOOTBALL','assists','career',5,30,10)"
