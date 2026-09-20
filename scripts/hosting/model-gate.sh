#!/usr/bin/env bash
# Run current-contract quality cases; read the generated work and manual review criteria.
# Mechanical checks alone do not establish model quality. eval returns nonzero on failure.
set -euo pipefail

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
