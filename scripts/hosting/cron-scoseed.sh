#!/usr/bin/env bash
# Cron wrapper for scoracle-seed.
#
# Cron strips the environment down to almost nothing (bare PATH,
# no venv, no project .env). This wrapper rebuilds the shell state
# each run so scoracle-seed finds its python, its DB URL, and its
# provider API keys.
#
# Usage from crontab:
#   0 23 * * * /home/sheneveld/scoracle-data/scripts/hosting/cron-scoseed.sh event process --sport football --season 2025

set -euo pipefail
cd /home/sheneveld/scoracle-data

# Activate the project venv.
# shellcheck source=/dev/null
source .venv/bin/activate

# Load env vars. .env is committed template (safe defaults); .env.local
# has real creds and wins because set -a exports every assignment.
set -a
# shellcheck source=/dev/null
[ -f .env ] && source .env
# shellcheck source=/dev/null
[ -f .env.local ] && source .env.local
set +a

exec scoracle-seed "$@"
