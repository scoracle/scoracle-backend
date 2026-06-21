#!/usr/bin/env bash
# Single release command for the self-hosted backend.
#
# Builds ALL FOUR Go binaries from one commit, stamps the commit + build time
# into them, (re)installs the systemd units, restarts the API, and verifies
# health. This is the answer to two Session-2 audit findings:
#   * binaries were deployed from different commits (each built by hand); and
#   * only the API was built reproducibly (the cron binaries drifted).
#
# Usage:
#   scripts/hosting/release.sh                 # full release (build + install + restart + verify)
#   scripts/hosting/release.sh --build-only    # build + stamp + place binaries only (no live changes)
#
# Env:
#   RELEASE_BIN_DIR   where binaries land (default: <repo>/go/bin)
#   PORT / API_PORT   API port for the post-restart health probe (default: 8000)
#
# Non-destructive verification (no systemd/cron/process changes):
#   RELEASE_BIN_DIR=$(mktemp -d) scripts/hosting/release.sh --build-only

set -euo pipefail

# `>/dev/null` guards against a shell where `cd` is wrapped to echo its target:
# without it the echo is captured alongside pwd and REPO_ROOT gets two lines.
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." >/dev/null && pwd)"

BUILD_ONLY=0
for arg in "$@"; do
    case "$arg" in
        --build-only) BUILD_ONLY=1 ;;
        -h|--help) sed -n '2,22p' "${BASH_SOURCE[0]}"; exit 0 ;;
        *) echo "release.sh: unknown argument '$arg'" >&2; exit 2 ;;
    esac
done

# --- Resolve the commit + build time to stamp -----------------------------
COMMIT="$(git -C "$REPO_ROOT" rev-parse --short=12 HEAD 2>/dev/null || echo unknown)"
if ! git -C "$REPO_ROOT" diff --quiet || ! git -C "$REPO_ROOT" diff --cached --quiet; then
    echo "WARNING: working tree is dirty — stamping commit as ${COMMIT}-dirty" >&2
    COMMIT="${COMMIT}-dirty"
fi
BUILD_TIME="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

BUILDINFO=github.com/albapepper/scoracle-data/internal/buildinfo
LDFLAGS="-w -s -X ${BUILDINFO}.Commit=${COMMIT} -X ${BUILDINFO}.BuildTime=${BUILD_TIME}"

BIN_DIR="${RELEASE_BIN_DIR:-$REPO_ROOT/go/bin}"
mkdir -p "$BIN_DIR"

# cmd subdirectory under go/cmd  ->  output binary name
CMDS=(api pipeline statcommentary vibesynth)
OUTS=(scoracle-api pipeline statcommentary vibesynth)

echo "==> building ${#CMDS[@]} binaries @ ${COMMIT} (built ${BUILD_TIME})"

# Stage on the same filesystem as BIN_DIR (its parent dir), so the final
# placement is an atomic rename and the staging writes don't trip the
# go/bin/ path watcher mid-build. Building EVERY binary before moving ANY is
# the key invariant: a failed build aborts (set -e) before a single binary is
# placed, so the cron binaries can never end up on a different commit than the
# API.
STAGE_PARENT="$(cd "$BIN_DIR/.." >/dev/null && pwd)"
STAGE="$(mktemp -d "$STAGE_PARENT/.scoracle-release.XXXXXX")"
cleanup() { rm -rf "$STAGE"; }
trap cleanup EXIT

(
    cd "$REPO_ROOT/go"
    for i in "${!CMDS[@]}"; do
        echo "    building ${OUTS[$i]}  (./cmd/${CMDS[$i]})"
        CGO_ENABLED=0 go build -ldflags "$LDFLAGS" -o "$STAGE/${OUTS[$i]}" "./cmd/${CMDS[$i]}"
    done
)

echo "==> placing binaries into $BIN_DIR"
for out in "${OUTS[@]}"; do
    mv -f "$STAGE/$out" "$BIN_DIR/$out"
    printf '    %-16s %s\n' "$out" "$(sha256sum "$BIN_DIR/$out" | cut -d' ' -f1)"
done

if [ "$BUILD_ONLY" -eq 1 ]; then
    echo "==> --build-only: skipping unit install + service restart + health probe"
    echo "==> build complete @ ${COMMIT}"
    exit 0
fi

# --- Install units, restart, verify ---------------------------------------
echo "==> (re)installing systemd units"
"$REPO_ROOT/scripts/hosting/install.sh"

echo "==> restarting scoracle-api"
systemctl --user restart scoracle-api.service

PORT="${PORT:-${API_PORT:-8000}}"
HEALTH_URL="http://localhost:${PORT}/health/db"
echo "==> verifying readiness at $HEALTH_URL"
code=""
for _ in $(seq 1 30); do
    code="$(curl -s -o /dev/null -w '%{http_code}' "$HEALTH_URL" || true)"
    [ "$code" = "200" ] && break
    sleep 1
done
if [ "$code" != "200" ]; then
    echo "ERROR: /health/db did not return 200 within 30s (last: ${code:-none})" >&2
    systemctl --user --no-pager status scoracle-api || true
    exit 1
fi

# Confirm the RUNNING process reports the commit we just built.
served="$(curl -s "http://localhost:${PORT}/" | grep -oE '"commit": *"[^"]*"' | head -1 | sed -E 's/.*"([^"]*)"$/\1/')"
if [ "$served" = "$COMMIT" ]; then
    echo "==> healthy; serving commit ${served}"
else
    echo "WARNING: served commit '${served}' != built '${COMMIT}' — check the restart" >&2
fi

echo "==> release complete @ ${COMMIT}"
