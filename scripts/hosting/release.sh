#!/usr/bin/env bash
# Single release command for the self-hosted backend.
#
# Builds the LIVE Go binaries AND the Rust cognition binaries from one commit,
# (re)installs the systemd units, restarts the API + the Rust cognition daemon,
# and verifies the API health probe.
#
# Post Step-3 cutover (2026-06-28) the Go LLM derive stages are retired: Go now
# serves the API, sweeps RSS (pipeline -mode ingest), and runs the nightly sigil
# reconcile backstop (vibesynth, DB-only — no Gemma). The Rust cognition layer
# owns every LLM stage: the long-running daemon (scoracle-cognition) + the rating
# batch (statcommentary). Go's retired statcommentary binary is no longer present.
#
# Usage:
#   scripts/hosting/release.sh                 # full release (build + install + restart + verify)
#   scripts/hosting/release.sh --build-only    # build + place binaries only (no live changes)
#
# Env:
#   RELEASE_BIN_DIR   where binaries land (default: Go → <repo>/go/bin, Rust →
#                     <repo>/rust/bin). Setting this redirects BOTH — the
#                     verification/inspection mode (one scratch dir for all 5).
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
        -h|--help) sed -n '2,28p' "${BASH_SOURCE[0]}"; exit 0 ;;
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

# Placement targets. In production each track lives in its own dir
# (go/bin for Go, rust/bin for Rust — the .path watchers are scope-pinned to
# each). When RELEASE_BIN_DIR is set (the --build-only verification pattern),
# BOTH tracks land in the same scratch dir so an inspection run touches zero
# live binaries + triggers zero path-watcher restarts.
if [ -n "${RELEASE_BIN_DIR:-}" ]; then
    GO_BIN_DIR="$RELEASE_BIN_DIR"
    RUST_BIN_DIR="$RELEASE_BIN_DIR"
else
    GO_BIN_DIR="$REPO_ROOT/go/bin"
    RUST_BIN_DIR="$REPO_ROOT/rust/bin"
fi
mkdir -p "$GO_BIN_DIR" "$RUST_BIN_DIR"

# Go cmd subdirectory under go/cmd  ->  output binary name.
# statcommentary is INTENTIONALLY absent: retired by the Step-3 cutover; the
# Rust rating batch in rust/bin/statcommentary replaces it (cron-rust-statcommentary.sh).
GO_CMDS=(api pipeline vibesynth)
GO_OUTS=(scoracle-api pipeline vibesynth)

# Rust live binaries (from rust/Cargo.toml). The daemon + the rating batch —
# deliberately NOT all bins (the offline parity / eval harnesses would waste a
# release cycle compiling).
RUST_BINS=(scoracle-cognition statcommentary)

echo "==> building ${#GO_CMDS[@]} Go + ${#RUST_BINS[@]} Rust binaries @ ${COMMIT} (built ${BUILD_TIME})"

# Stage on the same filesystem as the placement dirs (their shared parent =
# REPO_ROOT in production; RELEASE_BIN_DIR/.. in verification mode), so every
# final placement is an atomic rename and the staging writes don't trip the
# go/bin/ or rust/bin/ path watchers mid-build. Building EVERY binary (Go +
# Rust) before moving ANY is the key invariant: a failed build aborts (set -e)
# before a single binary is placed, so the cron binaries + the daemon can never
# end up on a different commit than the API.
STAGE_PARENT="$(cd "$GO_BIN_DIR/.." >/dev/null && pwd)"
STAGE="$(mktemp -d "$STAGE_PARENT/.scoracle-release.XXXXXX")"
# scoracle-api.path watches go/bin/ and scoracle-cognition.path watches rust/bin/;
# both fire a restart service on any close-write in their dir. We mask both across
# the placement below (see the placement step) so the renames don't each fire a
# spurious restart that cancels in-flight work (the in-API workers' Gemma drains
# on the Go side, the in-flight pipeline_work drain on the Rust side); cleanup
# re-arms whatever we actually stopped. The masks are records, so we only
# restart what we stopped.
API_PATH_MASKED=0
COGNITION_PATH_MASKED=0
cleanup() {
    rm -rf "$STAGE"
    if [ "$API_PATH_MASKED" -eq 1 ]; then
        systemctl --user start scoracle-api.path 2>/dev/null || true
    fi
    if [ "$COGNITION_PATH_MASKED" -eq 1 ]; then
        systemctl --user start scoracle-cognition.path 2>/dev/null || true
    fi
}
trap cleanup EXIT

# --- Build Go binaries -----------------------------------------------------
(
    cd "$REPO_ROOT/go"
    for i in "${!GO_CMDS[@]}"; do
        echo "    building ${GO_OUTS[$i]}  (./cmd/${GO_CMDS[$i]})"
        CGO_ENABLED=0 go build -ldflags "$LDFLAGS" -o "$STAGE/${GO_OUTS[$i]}" "./cmd/${GO_CMDS[$i]}"
    done
)

# --- Build Rust binaries ---------------------------------------------------
# The daemon + the rating batch only; cargo's incremental cache makes a no-op
# build ~instant (an unchanged commit is a stat-then-finish). cargo writes to
# rust/target/debug/<bin>; we stage from there into STAGE so the final move
# into rust/bin/ is the same atomic-rename discipline the Go bins get.
echo "    cargo build --bin ${RUST_BINS[*]}"
cargo build --manifest-path "$REPO_ROOT/rust/Cargo.toml" --bin "${RUST_BINS[0]}" --bin "${RUST_BINS[1]}"
for bin in "${RUST_BINS[@]}"; do
    cp "$REPO_ROOT/rust/target/debug/$bin" "$STAGE/$bin"
done

# Mask the rebuild watchers across placement (full release only). The single
# explicit restarts after install are authoritative; without this, each rename
# trips scoracle-api.path → scoracle-api-restart.service AND (within rust/bin/)
# scoracle-cognition.path → scoracle-cognition-restart.service, flapping both
# services ~5x in a second and cancelling in-flight work. Ad-hoc `go build` /
# `cargo build + cp` outside release.sh still auto-restarts (the watchers are
# re-armed by cleanup). Skipped for --build-only and when a watcher isn't running.
if [ "$BUILD_ONLY" -eq 0 ]; then
    if systemctl --user -q is-active scoracle-api.path 2>/dev/null; then
        echo "==> masking scoracle-api.path during placement (re-armed on exit)"
        if systemctl --user stop scoracle-api.path; then
            API_PATH_MASKED=1
        fi
    fi
    if systemctl --user -q is-active scoracle-cognition.path 2>/dev/null; then
        echo "==> masking scoracle-cognition.path during placement (re-armed on exit)"
        if systemctl --user stop scoracle-cognition.path; then
            COGNITION_PATH_MASKED=1
        fi
    fi
fi

echo "==> placing Go binaries into $GO_BIN_DIR"
for out in "${GO_OUTS[@]}"; do
    mv -f "$STAGE/$out" "$GO_BIN_DIR/$out"
    printf '    %-16s %s\n' "$out" "$(sha256sum "$GO_BIN_DIR/$out" | cut -d' ' -f1)"
done

echo "==> placing Rust binaries into $RUST_BIN_DIR"
for bin in "${RUST_BINS[@]}"; do
    mv -f "$STAGE/$bin" "$RUST_BIN_DIR/$bin"
    printf '    %-24s %s\n' "$bin" "$(sha256sum "$RUST_BIN_DIR/$bin" | cut -d' ' -f1)"
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
echo "==> verifying API readiness at $HEALTH_URL"
code=""
for _ in $(seq 1 30); do
    code="$(curl -s -o /dev/null -w '%{http_code}' "$HEALTH_URL" || true)"
    [ "$code" = "200" ] && break
    sleep 1
done
if [ "$code" != "200" ]; then
    echo "ERROR: API /health/db did not return 200 within 30s (last: ${code:-none})" >&2
    systemctl --user --no-pager status scoracle-api || true
    exit 1
fi

# Confirm the RUNNING API process reports the commit we just built.
served="$(curl -s "http://localhost:${PORT}/" | grep -oE '"commit": *"[^"]*"' | head -1 | sed -E 's/.*"([^"]*)"$/\1/')"
if [ "$served" = "$COMMIT" ]; then
    echo "==> API healthy; serving commit ${served}"
else
    echo "WARNING: served commit '${served}' != built '${COMMIT}' — check the API restart" >&2
fi

# The Rust cognition daemon has no HTTP health probe (its readiness is the
# systemd unit state + the journal: journalctl --user -u scoracle-cognition -f).
# Restart it now (the explicit authoritative restart after the path masking),
# then confirm it came up active. A failed boot is non-fatal for the API release
# verification above — the API is healthy and serving from precomputed tables; a
# failed cognition daemon is logged loudly and aborts THIS script so a 1-success-
# followed-by-silent-cognition-down state can't pass silently.
echo "==> restarting scoracle-cognition"
systemctl --user restart scoracle-cognition.service
# Brief settle window: the worker connects to Postgres + Ollama on boot; an
# immediate is-active could race the very first start. Give it the same 30s the
# API gets, but poll is-active every 1s.
COGNITION_UP=0
for _ in $(seq 1 30); do
    if systemctl --user --quiet is-active scoracle-cognition.service 2>/dev/null; then
        COGNITION_UP=1
        break
    fi
    sleep 1
done
if [ "$COGNITION_UP" -ne 1 ]; then
    echo "ERROR: scoracle-cognition is not active after restart" >&2
    systemctl --user --no-pager status scoracle-cognition || true
    journalctl --user -u scoracle-cognition --no-pager -n 40 || true
    exit 1
fi

echo "==> release complete @ ${COMMIT}"