#!/usr/bin/env bash
# Cron wrapper for the vibe binary.
#
# Cron strips the environment to almost nothing — no shell env, no .env,
# no venv. This wrapper rebuilds shell state from the repo's .env files
# so the vibe binary can resolve DATABASE_*, OLLAMA_*, and provider keys.
#
# Usage from crontab:
#   0 3 * * * /home/sheneveld/scoracle-data/scripts/hosting/cron-vibe.sh -mode batch -sport all -since-hours 24

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
