#!/usr/bin/env bash
# Weekly entity-tier recomputation.
#
# Rankings shift as seasons progress. This runs recompute_entity_tiers()
# for each sport so the headliner / starter / bench boundaries reflect
# recent activity. Cheap — <1s per sport. Scheduled weekly from cron.

set -euo pipefail
cd /home/sheneveld/scoracle-data

# Load DB URL from .env.local. Parse out password so PGPASSWORD works —
# beats embedding creds in a crontab.
# shellcheck source=/dev/null
set -a
[ -f .env ]       && source .env
[ -f .env.local ] && source .env.local
set +a

: "${DATABASE_URL:?DATABASE_URL not set}"

# Extract password from postgres URL of the form
#   postgresql://user:pass@host:port/db?options
# Using | as sed delimiter to sidestep the @ in the URL itself.
PGPASSWORD=$(printf '%s' "$DATABASE_URL" | sed -En 's|^[^:]+://[^:]+:([^@]+)@.*$|\1|p')
export PGPASSWORD

SEASON=${SEASON:-2025}

echo "[$(date -Iseconds)] recomputing tiers for season=$SEASON"
psql -h localhost -U scoracle -d scoracle <<SQL
\echo === NBA ===
SELECT * FROM recompute_entity_tiers('NBA', $SEASON);
\echo === NFL ===
SELECT * FROM recompute_entity_tiers('NFL', $SEASON);
\echo === FOOTBALL ===
SELECT * FROM recompute_entity_tiers('FOOTBALL', $SEASON);
SQL
echo "[$(date -Iseconds)] done"
