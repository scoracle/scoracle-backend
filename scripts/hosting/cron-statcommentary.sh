#!/usr/bin/env bash
# Cron wrapper for the stats-rail commentary (the on-field IDENTITY analysis).
#
# Cron strips the environment to almost nothing — no shell env, no .env, no venv.
# This wrapper rebuilds shell state from the repo's .env files so the binary can
# resolve DATABASE_*, OLLAMA_*, and provider keys.
#
# Cron schedule — nightly, current season only, regenerate only on new data:
#   0 3 * * * /home/sheneveld/scoracle/scoracle-backend/scripts/hosting/cron-statcommentary.sh -mode nightly
#
# Stats only move on game days, so `-mode nightly` enumerates the current season's
# rated entities and regenerates one ONLY when its rating snapshot changed (the
# input_hash differs from its last commentary) — most nights that's just the
# entities who played. Scheduled at 03:00, after the news pipeline (00:00) and the
# football event drain (23:00) so ratings are fresh; offset eases the shared GPU.
#
# Previous seasons are immutable — backfilled ONCE, out of band:
#   ./scripts/hosting/cron-statcommentary.sh -mode backfill        # all sports, all missing entity-seasons
#   ./scripts/hosting/cron-statcommentary.sh -mode backfill -sport NBA -limit 100   # chunked

set -euo pipefail
cd /home/sheneveld/scoracle/scoracle-backend

# Load env vars. .env is committed template (safe defaults); .env.local has real
# creds and wins because set -a exports every assignment.
set -a
# shellcheck source=/dev/null
[ -f .env ] && source .env
# shellcheck source=/dev/null
[ -f .env.local ] && source .env.local
set +a

exec ./go/bin/statcommentary "$@"
