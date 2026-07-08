#!/usr/bin/env bash
# Cron wrapper for the one-shot F2 bucket-labeling batch (friction-prune plan, F2
# verify). Resumable/idempotent: once the sample is fully labeled, re-runs no-op —
# remove the crontab line after the output TSV is complete.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

set -a
[[ -f .env ]] && source .env
[[ -f .env.local ]] && source .env.local
set +a

exec ./rust/bin/bucketlabel "$@"
