#!/usr/bin/env bash
# Cron wrapper for the sentiment binary (cmd/sentiment, formerly 'vibe').
# NOTE: superseded by cron-pipeline.sh — the staged pipeline runs the RSS sweep
# and sentiment as stages now. Kept for manual/standalone sentiment-corpus runs.
#
# Cron strips the environment to almost nothing — no shell env, no .env,
# no venv. This wrapper rebuilds shell state from the repo's .env files
# so the sentiment binary can resolve DATABASE_*, OLLAMA_*, and provider keys.
#
# Cron schedule — corpus mode once daily at midnight (local time):
#   0 0 * * * /home/sheneveld/scoracle/scoracle-backend/scripts/hosting/cron-vibe.sh -mode corpus
# (Was twice daily; the noon pass is now redundant — the in-API news-spike
#  LISTEN/NOTIFY worker handles intraday breaking news in real time.)
#
# Corpus mode RSS-sweeps every team in NBA/NFL/FOOTBALL, then runs Gemma
# only against entities whose news corpus picked up something fresh in the
# sweep. No fixture filter, no tier filter, no no-corpus markers.
#
# Real-time coverage between corpus runs is handled inside the API by the
# news-volume LISTEN/NOTIFY worker — when an entity's article count crosses
# the threshold (5 articles in 60 min), the SQL trigger fires NOTIFY and the
# worker runs Gemma immediately. No CLI involvement.

set -euo pipefail
cd /home/sheneveld/scoracle/scoracle-backend

# Load env vars. .env is committed template (safe defaults); .env.local
# has real creds and wins because set -a exports every assignment.
set -a
# shellcheck source=/dev/null
[ -f .env ] && source .env
# shellcheck source=/dev/null
[ -f .env.local ] && source .env.local
set +a

exec ./go/bin/sentiment "$@"
