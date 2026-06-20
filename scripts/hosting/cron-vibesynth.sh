#!/usr/bin/env bash
# cron-vibesynth.sh — nightly Sigil (crown) synthesis (Optimization Ledger O2).
#
# Resolves the Sigil warm gap: after A1 removed the on-read lazy synth, a rated
# entity has no sigil_synthesis row until a background trigger fires — and the
# composite-shift trigger is gated behind a follower check, so most entities
# never warm. As of 2026-06-19, ~95% of rated entities had no Sigil (the Sigil
# card + the O19 Sigil leaderboard board serve `current:null` for them).
#
# This runs `vibesynth -mode nightly`, which regenerates entities whose synthesis
# inputs changed AND backfills those with no synthesis row yet, capped by -limit.
# nightly mode skips unchanged inputs, so steady-state cost is small; the initial
# backlog drains -limit entities per night (~35s/generation on gemma4:e4b).
#
# Scheduling is a GPU-budget decision (see the recommended crontab line below).
# Off-peak (05:00) avoids the corpus pipeline (00:00) and statcommentary (03:00).
# Tune -limit to the nightly GPU budget; to drain the backlog faster, run a
# one-time larger backfill manually: ./go/bin/vibesynth -mode backfill -limit N
#
# Recommended crontab entry (NOT installed by default — enable per GPU budget):
#   0 5 * * * /home/sheneveld/scoracle/scoracle-backend/scripts/hosting/cron-vibesynth.sh \
#       -mode nightly -limit 150 -throttle-ms 250 \
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
