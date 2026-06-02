#!/usr/bin/env bash
# Cron wrapper for the transfer binary (transfer/trade rumor heat + Gemma vetting).
#
# Cron strips the environment to almost nothing — no shell env, no .env,
# no venv. This wrapper rebuilds shell state from the repo's .env files
# so the transfer binary can resolve DATABASE_*, OLLAMA_*, and provider keys.
#
# Cron schedule — corpus mode, STAGGERED ~30 min AFTER the daily vibe corpus run:
#   30 0 * * * /home/sheneveld/scoracle-backend/scripts/hosting/cron-transfer.sh -mode corpus
#
# Why staggered after vibe (cron-vibe.sh at 0 0):
#   1. The vibe corpus run does the RSS sweep that refreshes news_article_entities.
#      Transfer analysis reads that corpus — so it must run AFTER, not before.
#   2. Both share the single Archbox GPU. Offsetting reduces (doesn't eliminate)
#      contention; Ollama serializes requests regardless, so overlap degrades to
#      "slower", never "broken". If the vibe run routinely overruns 30 min, widen
#      the offset.
#
# Real-time coverage between corpus runs is handled inside the API by the
# transfer LISTEN/NOTIFY worker (internal/listener/transfer_worker.go) — when a
# TEAM's article count crosses the threshold (5 in 60 min) the SQL trigger fires
# pg_notify('transfer_trigger', ...) and the worker re-vets that team's rumors
# immediately (60-min per-team debounce, global concurrency cap). No CLI here.

set -euo pipefail
cd /home/sheneveld/scoracle-backend

# Load env vars. .env is committed template (safe defaults); .env.local
# has real creds and wins because set -a exports every assignment.
set -a
# shellcheck source=/dev/null
[ -f .env ] && source .env
# shellcheck source=/dev/null
[ -f .env.local ] && source .env.local
set +a

exec ./go/bin/transfer "$@"
