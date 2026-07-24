#!/usr/bin/env bash
# Cron wrapper for the RSS ingest sweep (pipeline -mode ingest).
#
# RSS sweep only; Rust owns LLM derivation. This wrapper runs the every-six-hours
# Google News RSS sweep that writes news_articles + news_article_entities;
# every model stage (scrub, transfers, narratives, vibe, sigil, rating) is
# drained from the durable pipeline_work queue by the Rust Cognition Harness
# (scoracle-cognition daemon + the rust/bin/statcommentary rating batch).
#
# Cron strips the environment to almost nothing — no shell env, no .env, no venv.
# This wrapper rebuilds shell state from the repo's .env files so the pipeline
# binary can resolve DATABASE_* and provider keys (no OLLAMA_* needed — Go does
# no model calls).
#
# Cron schedule — the ingest sweep every six hours (local time):
#   0 */6 * * * /home/sheneveld/scoracle/scoracle-backend/scripts/hosting/cron-pipeline.sh -mode ingest
#
# Observability: the sweep logs rss_ok / rss_fail / fresh_articles per run.
# Exit codes: 0 = success; 3 = partial (some RSS calls failed, retryable next
# run); 1 = every RSS call failed. The durable derive stages are owned by Rust
# and observed via pipeline_runs + the cognition journal.

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
