#!/usr/bin/env bash
# Cron wrapper for the Rust stats-rail commentary batch (Step 3 cutover).

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

set -a
[[ -f .env ]] && source .env
[[ -f .env.local ]] && source .env.local
set +a

exec ./rust/bin/statcommentary "$@"
