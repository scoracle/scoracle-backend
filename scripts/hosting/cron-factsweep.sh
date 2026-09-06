#!/usr/bin/env bash
# Cron wrapper for the dynamic-metadata adjudication sweep (factsweep).
#
# Scott, 2026-09-06: all entity metadata is dynamic — the sweep adjudicates person
# affiliations from the news the system already reads, policy-gated and fail-closed.
# Nightly per sport; a quiet person debounces via meta.affiliation_checked_at.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

set -a
[[ -f .env ]] && source .env
[[ -f .env.local ]] && source .env.local
set +a

for sport in FOOTBALL NBA NFL; do
    ./rust/bin/factsweep -sport "$sport" -limit 60 || echo "factsweep: $sport sweep failed (continuing)"
done
