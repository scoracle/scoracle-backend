#!/usr/bin/env bash
# cron-vibesynth.sh — nightly Sigil (crown) reconciliation (FIRST-GPT-AUDIT Session 12).
#
# Session 12 converted `-mode nightly` from inline synthesis into bounded
# RECONCILIATION: it enumerates CURRENT-SEASON rated entities whose current-season
# Sigil is missing or stale (an input generation is newer than the Sigil) and ENQUEUES
# a durable `sigil` pipeline_work item for each. The always-on in-API derive worker
# drains the queue — resolving current_season and hash-gating the Gemma call — so the
# schedule NEVER synthesizes inline and never regenerates an unchanged Sigil because a
# schedule fired. Real-time convergence (news vibe→sigil, composite_shift→sigil) keeps
# the corpus warm between runs; this is the backstop that fills coverage gaps.
#
# Because it only enqueues, this run needs ONLY the database (no Ollama / GPU). -limit
# caps the number of items enqueued per run; -throttle-ms is ignored in nightly mode.
# To populate current + historical gaps directly (needs Ollama), run a backfill:
#   ./go/bin/vibesynth -mode backfill -limit N        # one-time, GPU-bound
#
# Off-peak (05:00) avoids the corpus pipeline (00:00) and statcommentary (03:00).
#
# Observability + overlap (FIRST-GPT-AUDIT Session 13): nightly/reconcile + backfill
# share a per-job advisory lock ("vibesynth"), so a manual backfill and the nightly
# reconcile can't overlap — the loser exits 0. Each run is recorded in pipeline_runs.
# Exit codes: 0 = success/overlap-skip; 3 = partial (some enqueues/syntheses failed);
# 1 = all sports failed to enumerate (or, in backfill, every synthesis failed).
#
# Recommended crontab entry (NOT installed by default — enable per coverage needs):
#   0 5 * * * /home/sheneveld/scoracle/scoracle-backend/scripts/hosting/cron-vibesynth.sh \
#       -mode nightly -limit 500 \
#       >> /home/sheneveld/scoracle/scoracle-backend/logs/vibesynth.log 2>&1

set -euo pipefail
cd /home/sheneveld/scoracle/scoracle-backend
set -a
# shellcheck source=/dev/null
[ -f .env ] && source .env
# shellcheck source=/dev/null
[ -f .env.local ] && source .env.local
set +a
exec ./go/bin/vibesynth "$@"
