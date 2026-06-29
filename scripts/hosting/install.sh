#!/usr/bin/env bash
# One-shot installer for the Scoracle self-hosting setup.
#
# What this script does:
#   1. RENDERS the systemd user units (substituting the real repo root for the
#      __SCORACLE_REPO_ROOT__ placeholder) into ~/.config/systemd/user/
#   2. Creates the logs/ directory the cron jobs write to
#   3. Sets executable bits on the hosting scripts
#   4. Runs systemctl --user daemon-reload
#
# Why render instead of copy: the unit templates carry a __SCORACLE_REPO_ROOT__
# placeholder rather than a hardcoded path, so a clone in ANY location installs
# correct units. (The old templates hardcoded a stale pre-consolidation path,
# so installing them on a replacement machine recreated a known-broken
# deployment — audit Session 2.)
#
# What it does NOT do (manual steps with sudo or user cron/sudo):
#   * loginctl enable-linger <user>         (sudo)
#   * sudo cp .../logrotate.conf /etc/logrotate.d/scoracle
#   * crontab scripts/hosting/crontab.example
#   * mkdir + mount the backup destination
#   * cloudflared install + tunnel creation (interactive)
#
# Verification / dry target: set SCORACLE_SYSTEMD_DIR to render the units into a
# scratch directory and inspect the rendered paths without touching the live
# user units (skips daemon-reload and the manual-steps banner):
#   SCORACLE_SYSTEMD_DIR=$(mktemp -d) scripts/hosting/install.sh
#
# Idempotent — safe to re-run.

set -euo pipefail

# `>/dev/null` guards against a shell where `cd` is wrapped to echo its target
# (a chpwd-style hook): without it the echo would be captured alongside pwd and
# REPO_ROOT would contain two lines, breaking the sed render below.
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." >/dev/null && pwd)"
DEFAULT_SYSTEMD_DIR="$HOME/.config/systemd/user"
USER_SYSTEMD_DIR="${SCORACLE_SYSTEMD_DIR:-$DEFAULT_SYSTEMD_DIR}"

# render_unit SRC DST — copy a unit template, substituting the repo root for the
# __SCORACLE_REPO_ROOT__ placeholder. Uses | as the sed delimiter so the path's
# slashes don't need escaping.
render_unit() {
    local src="$1" dst="$2"
    sed "s|__SCORACLE_REPO_ROOT__|${REPO_ROOT}|g" "$src" > "$dst"
}

echo "==> rendering systemd units into $USER_SYSTEMD_DIR"
mkdir -p "$USER_SYSTEMD_DIR"
for unit in scoracle-api.service scoracle-api-restart.service scoracle-api.path \
            scoracle-cognition.service scoracle-cognition-restart.service scoracle-cognition.path \
            cloudflared.service; do
    render_unit "$REPO_ROOT/scripts/systemd/$unit" "$USER_SYSTEMD_DIR/$unit"
done

echo "==> ensuring logs directory exists"
mkdir -p "$REPO_ROOT/logs"

echo "==> setting executable bits on hosting scripts"
# All hosting wrappers, not just the seeder/backup set — the cron-pipeline,
# cron-statcommentary, and cron-vibesynth wrappers are active hosting entry
# points and must be executable on a fresh machine too (audit Session 2).
chmod +x "$REPO_ROOT"/scripts/hosting/*.sh

# Non-default target = verification render only; don't touch the live daemon.
if [ "$USER_SYSTEMD_DIR" != "$DEFAULT_SYSTEMD_DIR" ]; then
    echo
    echo "==> rendered into non-default target $USER_SYSTEMD_DIR"
    echo "    (skipping daemon-reload and live setup — verification mode)"
    exit 0
fi

echo "==> reloading systemd user daemon"
systemctl --user daemon-reload

cat <<EOF

==> Installed. Remaining manual steps:

  1. Enable linger so user services survive logout:
       sudo loginctl enable-linger "\$USER"

  2. Start the API + its auto-restart-on-rebuild path watcher:
       systemctl --user enable --now scoracle-api.path
       systemctl --user enable --now scoracle-api.service
       systemctl --user status scoracle-api

  2b. (GPU box only) Start the Rust Cognition Harness daemon. In the Step-3 cutover
      it owns scrub, transfers, narratives, vibe, and sigil; keep
      DERIVE_WORKER_ENABLED=false in .env.local so the Go API does not also claim
      the queue. Deploy the binary first: cargo build --manifest-path rust/Cargo.toml && \
        cp rust/target/debug/scoracle-cognition rust/bin/scoracle-cognition
       systemctl --user enable --now scoracle-cognition.path
       systemctl --user enable --now scoracle-cognition.service
       systemctl --user status scoracle-cognition

  3. Install crontab (edits user cron, no sudo needed):
       crontab scripts/hosting/crontab.example

  4. Install logrotate rules:
       sudo cp scripts/hosting/logrotate.conf /etc/logrotate.d/scoracle
       sudo logrotate -d /etc/logrotate.d/scoracle   # dry-run check

  5. Pick a backup target (edit BACKUP_DIR in scripts/hosting/backup-postgres.sh
     or export via the crontab line) and make sure it exists + is writable.
     Run the first backup manually to seed the directory:
       scripts/hosting/backup-postgres.sh

  6. Do the restore drill ONCE on day one:
       scripts/hosting/restore-drill.sh /path/to/scoracle-<date>.dump

  7. Cloudflare Tunnel (optional, whenever you're ready to expose publicly):
       See scripts/hosting/cloudflared-config.example.yml for full flow.

  Subsequent releases: use scripts/hosting/release.sh — it builds the Go
  binaries from one commit, re-renders + reinstalls the units, restarts the
  API, and verifies health.
EOF
