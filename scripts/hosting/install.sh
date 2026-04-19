#!/usr/bin/env bash
# One-shot installer for the Scoracle self-hosting setup.
#
# What this script does:
#   1. Copies systemd user units to ~/.config/systemd/user/
#   2. Creates the logs/ directory the cron jobs write to
#   3. Sets executable bits on the hosting scripts
#   4. Runs systemctl --user daemon-reload
#
# What it does NOT do (manual steps with sudo or user cron/sudo):
#   * loginctl enable-linger <user>         (sudo)
#   * sudo cp .../logrotate.conf /etc/logrotate.d/scoracle
#   * crontab scripts/hosting/crontab.example
#   * mkdir + mount the backup destination
#   * cloudflared install + tunnel creation (interactive)
#
# Idempotent — safe to re-run.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
USER_SYSTEMD_DIR="$HOME/.config/systemd/user"

echo "==> copying systemd units"
mkdir -p "$USER_SYSTEMD_DIR"
cp "$REPO_ROOT/scripts/systemd/scoracle-api.service"         "$USER_SYSTEMD_DIR/"
cp "$REPO_ROOT/scripts/systemd/scoracle-api-restart.service" "$USER_SYSTEMD_DIR/"
cp "$REPO_ROOT/scripts/systemd/scoracle-api.path"            "$USER_SYSTEMD_DIR/"
cp "$REPO_ROOT/scripts/systemd/cloudflared.service"          "$USER_SYSTEMD_DIR/"

echo "==> ensuring logs directory exists"
mkdir -p "$REPO_ROOT/logs"

echo "==> setting executable bits on hosting scripts"
chmod +x \
    "$REPO_ROOT/scripts/hosting/cron-scoseed.sh" \
    "$REPO_ROOT/scripts/hosting/backup-postgres.sh" \
    "$REPO_ROOT/scripts/hosting/restore-drill.sh"

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
EOF
