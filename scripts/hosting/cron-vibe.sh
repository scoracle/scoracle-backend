#!/usr/bin/env bash
# Cron wrapper for the vibe binary.
#
# Cron strips the environment to almost nothing — no shell env, no .env,
# no venv. This wrapper rebuilds shell state from the repo's .env files
# so the vibe binary can resolve DATABASE_*, OLLAMA_*, and provider keys.
#
# Recommended cron — corpus mode at noon and midnight (local time):
#   0 0,12 * * * /home/sheneveld/scoracle-data/scripts/hosting/cron-vibe.sh -mode corpus
#
# corpus mode RSS-sweeps every team in NBA/NFL/FOOTBALL, then runs Gemma
# only against entities whose news corpus picked up something fresh in the
# sweep. No fixture filter, no tier filter, no no-corpus markers.
#
# Legacy: -mode batch is still wired up for one-off backfills, but the
# corpus path is the canonical scheduled job.

set -euo pipefail
cd /home/sheneveld/scoracle-data

# Load env vars. .env is committed template (safe defaults); .env.local
# has real creds and wins because set -a exports every assignment.
set -a
# shellcheck source=/dev/null
[ -f .env ] && source .env
# shellcheck source=/dev/null
[ -f .env.local ] && source .env.local
set +a

exec ./go/bin/vibe "$@"
