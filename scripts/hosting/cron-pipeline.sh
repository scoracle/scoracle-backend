#!/usr/bin/env bash
# Cron wrapper for the staged Gemma pipeline (sweep -> transfers -> narratives -> vibe).
#
# Cron strips the environment to almost nothing — no shell env, no .env, no venv.
# This wrapper rebuilds shell state from the repo's .env files so the pipeline
# binary can resolve DATABASE_*, OLLAMA_*, and provider keys.
#
# Cron schedule — the full chain once daily, overnight (local time):
#   0 0 * * * /home/sheneveld/scoracle/scoracle-backend/scripts/hosting/cron-pipeline.sh -mode corpus
#
# This REPLACES the former separate cron-vibe.sh + cron-transfer.sh entries: the
# pipeline runs the stages IN ORDER in one process so each stage enriches the next
# (transfers ground the narratives; the vibe reads the fresh narratives + heat).
# Running it alongside the old two would double-run vibe/transfer — pick one.
#
# Real-time coverage between daily runs is handled inside the API by the
# LISTEN/NOTIFY workers: a news-volume spike (5 articles in 60 min) refreshes the
# entity's narratives then its vibe; a team transfer spike re-vets that team's
# rumors. No CLI involvement.

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

exec ./go/bin/pipeline "$@"
