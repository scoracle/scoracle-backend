#!/usr/bin/env bash
# Cron wrapper for the pipeline binary's daily sweep steps.
#
# THE INGESTION LAYER, both halves of it (PLAN-weekly-fantasy-rail.md):
#
#   -mode data    schedules + rosters from the free stat feeds, then the
#                 gap-driven stats import: every fixture the feed says is
#                 finished and the DB has no stat rows for is fetched and
#                 promoted through finalize_fixture(). Idempotent and
#                 self-healing — a missed night is still in the gap tomorrow.
#   -mode ingest  the daily Google News RSS sweep that writes news_articles
#                 and enqueues the Editor's read in the same transaction.
#
# Google does the news relevancy work, the feeds detect their own events, and
# every curation stage is drained from the durable pipeline_work queue by the
# Rust Cognition Harness (scoracle-cognition daemon + the rust/bin/
# statcommentary rating batch). This binary performs no model work.
#
# Cron strips the environment to almost nothing — no shell env, no .env.local.
# This wrapper rebuilds shell state from .env.local (the only env file) so the
# pipeline binary can resolve DATABASE_* (no OLLAMA_* needed — Go does no
# model calls).
#
# Cron schedule — data first (fresh fixtures for the Editor), news at 02:00:
#   30 1 * * * /home/sheneveld/scoracle/scoracle-backend/scripts/hosting/cron-pipeline.sh -mode data
#   0  2 * * * /home/sheneveld/scoracle/scoracle-backend/scripts/hosting/cron-pipeline.sh -mode ingest
#
# The ingest cadence and the RSS lookback window MUST move together.
# `rssLookbackHours` in go/internal/thirdparty/news.go is 24h to match this
# daily schedule; running this less often than that window is wide leaves an
# unswept gap that no later run recovers, because the window is also the
# client-side freshness cutoff. (-mode data has no such coupling: its
# "window" is the gap query, which never forgets.)
#
# Observability: both steps record pipeline_runs rows (jobs "pipeline" and
# "pipeline-data") and log their funnels. Exit codes: 0 = success; 3 = partial
# (retryable next run); 1 = failed. The durable derive stages are owned by
# Rust and observed via pipeline_runs + the cognition journal.

set -euo pipefail
cd /home/sheneveld/scoracle/scoracle-backend

# Load env vars from .env.local (the only env file; gitignored). set -a
# exports every assignment; already-set vars win.
set -a
# shellcheck source=/dev/null
[ -f .env.local ] && source .env.local
set +a

exec ./go/bin/pipeline "$@"
