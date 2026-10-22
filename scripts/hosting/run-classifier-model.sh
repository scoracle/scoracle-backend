#!/usr/bin/env bash
set -euo pipefail

model="${CLASSIFIER_MODEL_FILE:?set the verified d1 GGUF path}"
name="${CLASSIFIER_MODEL_NAME:-LiquidAI/d1-3B-GGUF}"
read -r revision _ < <(sha256sum "$model")
exec "$HOME/.local/bin/scoracle-llama-server" -m "$model" \
    --alias "$name@sha256:$revision" \
    --host 127.0.0.1 --port 8081 --ctx-size "${CLASSIFIER_CONTEXT_SIZE:-65536}" --parallel "${CLASSIFIER_PARALLEL:-4}" \
    --batch-size 2048 --ubatch-size 512 --n-gpu-layers 99
