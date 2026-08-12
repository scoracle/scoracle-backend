#!/usr/bin/env bash
# Cron wrapper for the RSS ingest sweep (pipeline -mode ingest).
#
# THE ONLY DATA INGESTION LAYER. This wrapper runs the daily Google News RSS
# sweep that writes news_articles and enqueues the Editor's read in the same
# transaction; Google does the relevancy work, and every curation stage is
# drained from the durable pipeline_work queue by the Rust Cognition Harness
# (scoracle-cognition daemon + the rust/bin/statcommentary rating batch).
#
# Cron strips the environment to almost nothing — no shell env, no .env.local.
# This wrapper rebuilds shell state from .env.local (the only env file) so the
# pipeline binary can resolve DATABASE_* (no OLLAMA_* needed — Go does no
# model calls).
#
# Cron schedule — the ingest sweep once daily at 02:00 (local time):
#   0 2 * * * /home/sheneveld/scoracle/scoracle-backend/scripts/hosting/cron-pipeline.sh -mode ingest
#
# The cadence and the RSS lookback window MUST move together. `rssLookbackHours`
# in go/internal/thirdparty/news.go is 24h to match this daily schedule; running this
# less often than that window is wide leaves an unswept gap that no later run
# recovers, because the window is also the client-side freshness cutoff.
#
# Observability: the sweep logs rss_ok / rss_fail / fresh_articles per run.
# Exit codes: 0 = success; 3 = partial (some RSS calls failed, retryable next
# run); 1 = every RSS call failed. The durable derive stages are owned by Rust
# and observed via pipeline_runs + the cognition journal.

set -euo pipefail
cd /home/sheneveld/scoracle/scoracle-backend

# Load env vars from .env.local (the only env file; gitignored). set -a
# exports every assignment; already-set vars win.
set -a
# shellcheck source=/dev/null
[ -f .env.local ] && source .env.local
set +a

exec ./go/bin/pipeline "$@"
