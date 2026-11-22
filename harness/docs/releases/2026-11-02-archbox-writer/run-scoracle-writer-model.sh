#!/usr/bin/env bash
set -euo pipefail
root=/mnt/data/backup/scoracle/archbox-writer-20261102
runtime=/mnt/data/backup/scoracle/d3-rollout-20261027/runtime/lib/ollama
model="$root/models/Qwen3.5-4B-Q4_K_M.gguf"
read -r actual _ < <(sha256sum "$model")
[[ "$actual" == 00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4 ]] || exit 1
export LD_LIBRARY_PATH="$runtime:$runtime/cuda_v12"
export GGML_BACKEND_PATH="$runtime/cuda_v12/libggml-cuda.so"
exec "$runtime/llama-server" --model "$model" --alias Qwen3.5-4B-Q4_K_M \
 --host 127.0.0.1 --port 8082 --ctx-size 10240 --parallel 1 \
 --n-gpu-layers 99 --flash-attn auto --batch-size 512 --ubatch-size 128 \
 --threads 4 --cache-ram 0 --slots --jinja --reasoning-budget 0 \
 --chat-template-kwargs '{"enable_thinking":false}' --no-context-shift
