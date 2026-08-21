#!/usr/bin/env bash
# Run a fixture gate against a CANDIDATE character model on the local Ollama.
#
# Since the 2026-08-20 single-box consolidation there are no scheduled rest windows (cognition
# is work-driven; the duty-cycle timers are gone). Run this on archbox when the drain is quiet
# — it competes with production for the same card otherwise — or on any box with Ollama and the
# model pulled (OLLAMA_MAX_LOADED_MODELS=1 makes every incumbent/candidate swap a full reload).
#
# WHY THIS EXISTS: the question is never "is it faster" but "does the voice survive" — a
# candidate model runs the same fixtures as the incumbent, against a known baseline.
#
# Usage:  model-gate.sh <task> <incumbent> [candidate]
#   model-gate.sh momentum ministral-3:3b some-candidate:3b   # A/B, side by side
#   model-gate.sh momentum some-candidate:3b                  # single model vs the incumbent's floor
#
# With a CANDIDATE the eval binary runs both over the same fixtures and prints its own side-by-side
# report -- that is what `EvalReport` exists for ("the side-by-side the human reads"). It runs
# all-incumbent then all-candidate so exactly ONE model swap happens, which matters on a 16 GB box
# where OLLAMA_MAX_LOADED_MODELS=1 makes every swap a full reload.
#
# Read the PROSE, not just the check counts. The property checks (word counts, required and banned
# phrases) say the contract holds; they cannot say the voice is right. That judgment is the whole
# point of running two models over identical inputs.
#
# HISTORICAL baselines (2026-07-26, ministral-3:14b — a retired model at retired prompt
# versions; kept only as provenance). Establish a fresh floor by running the current
# incumbent (ministral-3:3b) over the fixtures FIRST, then compare candidates to that —
# a baseline that predates the prompt it gates cannot detect a regression:
#   momentum s13 : 36/37   -- the one red was a genuine `steady band` leak
#   oracle   or8 : 94/98   -- all four reds were `reading_max_peers`, left red on purpose
set -uo pipefail

TASK="${1:?usage: model-gate.sh <task> <incumbent> [candidate]}"
MODEL="${2:?usage: model-gate.sh <task> <incumbent> [candidate]}"
CANDIDATE="${3:-}"
case "$TASK" in
    momentum) ROUTE_ENV="COGNITION_ROUTE_MOMENTUM_LOGIC" ;;
    oracle)   ROUTE_ENV="COGNITION_ROUTE_ORACLE_LOGIC" ;;
    *)        echo "unknown task '$TASK'" >&2; exit 2 ;;
esac

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
OUT_DIR="$REPO/logs/model-eval"
mkdir -p "$OUT_DIR"
STAMP=$(date +%Y%m%d-%H%M)
OUT="$OUT_DIR/${TASK}-${MODEL//[:\/]/_}-${STAMP}.log"

cd "$REPO/rust" || exit 1

# OLLAMA_TIMEOUT_SECONDS is NOT optional: the default is 60s (config.rs) and two fixtures exceed
# it, so omitting it reports failures that are the harness timing out rather than the model losing.
export DATABASE_URL="postgres://unused/unused"
export OLLAMA_BASE_URL="http://127.0.0.1:11434"
export OLLAMA_TIMEOUT_SECONDS=600

{
    if [ -n "$CANDIDATE" ]; then
        echo "=== $TASK A/B | incumbent=$MODEL | candidate=$CANDIDATE | $(date '+%F %T') ==="
        env "$ROUTE_ENV=$MODEL" "${ROUTE_ENV}_CANDIDATE=$CANDIDATE" \
            ./target/debug/eval --task "$TASK" --fixtures 2>&1
    else
        echo "=== $TASK gate | model=$MODEL | via $ROUTE_ENV | $(date '+%F %T') ==="
        env "$ROUTE_ENV=$MODEL" ./target/debug/eval --task "$TASK" --fixtures 2>&1
    fi
    echo "=== finished $(date '+%F %T') ==="
} | tee "$OUT"

echo
echo "saved: $OUT"
