#!/usr/bin/env bash
set -euo pipefail

root="${D3_RELEASE_DIR:?set the verified d3 release directory}"
model="$root/models/d3-2b-Q8_0.gguf"
expected=47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707
read -r actual _ < <(sha256sum "$model")
[[ "$actual" == "$expected" ]] || { echo 'd3 model checksum mismatch' >&2; exit 1; }
export LD_LIBRARY_PATH="$root/runtime/lib/ollama:$root/runtime/lib/ollama/cuda_v12"
export GGML_BACKEND_PATH="$root/runtime/lib/ollama/cuda_v12/libggml-cuda.so"
exec "$root/runtime/lib/ollama/llama-server" -m "$model" \
    --alias "vllm-sr/d3-2b-Q8_0@sha256:$expected" \
    --host 127.0.0.1 --port "${D3_CLASSIFIER_PORT:-8081}" \
    --ctx-size 16384 --parallel 4 --batch-size 4096 --ubatch-size 4096 \
    --n-gpu-layers 99 --device CUDA0
