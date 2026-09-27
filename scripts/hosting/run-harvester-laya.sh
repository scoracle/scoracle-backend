#!/usr/bin/env bash
# Local-only Laya endpoint for Harvester. Publisher text stays on the worker host.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." >/dev/null && pwd)"
PYTHON="${HARVESTER_LAYA_PYTHON:-$HOME/harvester-laya-venv/bin/python}"
MODEL_DIR="${HARVESTER_LAYA_MODEL_DIR:-$HOME/harvester-laya-model/english}"
REVISION="${HARVESTER_LAYA_REVISION:-55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851}"
DEVICE="${HARVESTER_LAYA_DEVICE:-cpu}"
export HF_HUB_OFFLINE=1
exec "$PYTHON" "$REPO_ROOT/rust/examples/harvest_laya_server.py" \
    --model-dir "$MODEL_DIR" --revision "$REVISION" --device "$DEVICE" --port 8019
