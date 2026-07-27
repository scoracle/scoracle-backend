#!/usr/bin/env bash
# Run a fixture gate against a CANDIDATE character model, on the Mac, inside a GPU rest window.
#
# WHY A REST WINDOW: the gate needs the Mac's GPU, and production needs the same one. The
# 2h-on/1h-off stagger (scoracle-cognition-{pause,resume}.timer on Archbox) leaves the Mac idle
# for a full hour every three, which is exactly when a gate can run without competing with the
# drain or being slowed by it. Rest windows open at 00,03,06,09,12,15,18,21:00 local.
#
# WHY THIS EXISTS: decode on the M4 is memory-bandwidth-bound and measured at ~98% of the chip's
# 120 GB/s, so model SIZE is the only speed lever left on this box -- and a smaller model also
# frees the KV bytes that currently pin the Mac to max_concurrent=1, which is what serializes the
# six voices. The question is never "is it faster" (it is, in proportion to bytes) but "does the
# voice survive". That is what the gate answers, against a known baseline.
#
# Usage:  model-gate.sh <task> <incumbent> [candidate]
#   model-gate.sh momentum ministral-3:14b mistral-nemo:12b   # A/B, side by side
#   model-gate.sh momentum mistral-nemo:12b                   # single model vs the baselines below
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
# Baselines (2026-07-26, ministral-3:14b):
#   momentum s13 : 36/37   -- the one red is a genuine `steady band` leak
#   oracle   or8 : 94/98   -- all four reds are `reading_max_peers`, left red on purpose
set -uo pipefail

TASK="${1:?usage: model-gate.sh <task> <incumbent> [candidate]}"
MODEL="${2:?usage: model-gate.sh <task> <incumbent> [candidate]}"
CANDIDATE="${3:-}"
case "$TASK" in
    momentum) ROUTE_ENV="COGNITION_ROUTE_MOMENTUM_LOGIC" ;;
    oracle)   ROUTE_ENV="COGNITION_ROUTE_ORACLE_LOGIC" ;;
    *)        echo "unknown task '$TASK'" >&2; exit 2 ;;
esac

REPO=/Users/scotty/scoracle/scoracle-backend
OUT_DIR="$REPO/logs/model-eval"
mkdir -p "$OUT_DIR"
STAMP=$(date +%Y%m%d-%H%M)
OUT="$OUT_DIR/${TASK}-${MODEL//[:\/]/_}-${STAMP}.log"

cd "$REPO/rust"

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
