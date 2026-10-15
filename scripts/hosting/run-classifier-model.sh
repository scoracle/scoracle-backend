#!/usr/bin/env bash
set -euo pipefail

model="$HOME/models/Kev-0.8B-Q8_0.gguf"
read -r revision _ < <(sha256sum "$model")
exec "$HOME/.local/bin/scoracle-llama-server" -m "$model" \
    --alias "ggml-org/Kev-0.8B-GGUF@sha256:$revision" \
    --host 127.0.0.1 --port 8081 --ctx-size 65536 --parallel 8 \
    --batch-size 2048 --ubatch-size 512 --n-gpu-layers 99
