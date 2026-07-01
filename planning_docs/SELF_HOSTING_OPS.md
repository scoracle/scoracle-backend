# Self-Hosting Operations Plan

> **This is the original strategy / first-time-setup doc.** For day-to-day operations
> (release/rollback, backup/restore, jobs, the durable work queue, incident quick-reference)
> use **[`../RUNBOOK.md`](../RUNBOOK.md)**, which is reconciled to the live system. The concrete
> scripts live under `scripts/hosting/` (`scripts/hosting/README.md`). Some inline paths below are
> historical (`/home/sheneveld/scoracle-data`); the live repo root is
> `/home/sheneveld/scoracle/scoracle-backend`.

What it takes to run the Scoracle stack reliably from the Arch desktop
without needing a terminal open 24/7.

## Current state recap

| Component | Lifecycle | Survives logout? |
|---|---|---|
| Postgres 18.3 | systemd system unit | ✅ yes |
| Ollama + local model e4b | `systemd --user` service (default install) | ❌ no (needs linger) |
| Go API (`scoracle-api`) | manual `./go/bin/scoracle-api` in a terminal | ❌ no |
| Python seeder | manual CLI calls | ❌ no |

The Go API and Python seeder are the gaps. Postgres is already
production-grade. Ollama stays up as long as you're logged in, but drops
when you log out unless linger is enabled.

## Priority ordering

1. **Required for unattended operation** (sections A, B, C below)
2. **Required before trusting the corpus** (section D)
3. **Optional polish** (sections E, F)

---

## A. systemd user unit for the Go API (required)

Makes the API auto-start on boot, auto-restart on crash, and survive
terminal close.

**Steps:**

1. Enable user linger so services keep running when you're logged out:
   ```bash
   loginctl enable-linger sheneveld
   ```

2. Create `~/.config/systemd/user/scoracle-api.service`:
   ```ini
   [Unit]
   Description=Scoracle Data API
   After=network-online.target postgresql.service
   Wants=network-online.target

   [Service]
   Type=simple
   WorkingDirectory=/home/sheneveld/scoracle-data
   EnvironmentFile=-/home/sheneveld/scoracle-data/.env
   EnvironmentFile=-/home/sheneveld/scoracle-data/.env.local
   ExecStart=/home/sheneveld/scoracle-data/go/bin/scoracle-api
   Restart=on-failure
   RestartSec=5
   StandardOutput=journal
   StandardError=journal

   [Install]
   WantedBy=default.target
   ```

   The leading `-` on `EnvironmentFile` makes them optional (doesn't
   fail if missing). `.env.local` loads last so it wins on conflicts.

3. Enable + start:
   ```bash
   systemctl --user daemon-reload
   systemctl --user enable --now scoracle-api.service
   systemctl --user status scoracle-api
   journalctl --user -u scoracle-api -f    # tail logs
   ```

After this, `reboot` will come back with the API already running.
No terminal required. You can close everything, log out, walk away.

**One gotcha to know about:** after a `go build`, the binary on disk
changes but the running process doesn't. Run
`systemctl --user restart scoracle-api` after every rebuild.

---

## B. Cron for the Python seeder (required)

The daily/weekly jobs from `planning_docs/CRON_SEEDING_STRATEGY.md` need
to actually fire. Two ways:

### Option B1: traditional cron (simplest)

1. Create a wrapper script at `scripts/cron-scoseed.sh`:
   ```bash
   #!/usr/bin/env bash
   set -euo pipefail
   cd /home/sheneveld/scoracle-data
   source .venv/bin/activate
   set -a
   source .env.local
   set +a
   exec scoracle-seed "$@"
   ```
   `chmod +x scripts/cron-scoseed.sh`

2. `crontab -e` and add (assuming system TZ = ET; adjust if UTC):
   ```cron
   # Daily 23:00 ET — drain newly-eligible football fixtures
   0 23 * * * /home/sheneveld/scoracle-data/scripts/cron-scoseed.sh event process --sport football --season 2025 >> /home/sheneveld/scoracle-data/logs/cron-football.log 2>&1

   # Weekly Monday 23:00 ET — schedule + roster refresh
   0 23 * * 1 /home/sheneveld/scoracle-data/scripts/cron-scoseed.sh event load-fixtures football --season 2025 >> /home/sheneveld/scoracle-data/logs/cron-football.log 2>&1
   30 23 * * 1 /home/sheneveld/scoracle-data/scripts/cron-scoseed.sh meta seed football --season 2025 >> /home/sheneveld/scoracle-data/logs/cron-football.log 2>&1
   ```
   `mkdir -p logs/` first. NBA/NFL entries go here when BDL webhooks
   are wired (see below).

### Option B2: systemd timers (cleaner, more Arch-native)

Create matching `~/.config/systemd/user/scoracle-seed-*.service` +
`.timer` files. Benefit: same journal as the API, explicit TZ
handling (`Persistent=true` catches missed runs after suspend).
Slightly more setup; can migrate to this later.

**Recommendation:** start with B1, migrate to B2 if you find yourself
wanting unified logs or missed-run recovery.

---

## C. Postgres backups (required before trusting the corpus)

The NVMe is your single point of failure. One disk failure or one
`rm -rf` in the wrong directory and you lose the vibe corpus, all
seeded game data, and every custom tuning.

### Minimum viable: nightly `pg_dump` to a second location

1. Identify a second storage target. Options:
   - Second physical disk mounted at `/mnt/backup`
   - External USB drive
   - NAS mount
   - Cloud bucket (S3, B2, R2) — safer but needs credentials

2. Create `scripts/backup-postgres.sh`:
   ```bash
   #!/usr/bin/env bash
   set -euo pipefail
   BACKUP_DIR=${BACKUP_DIR:-/mnt/backup/scoracle}
   DATE=$(date -u +%Y%m%dT%H%M%SZ)
   FILE="$BACKUP_DIR/scoracle-$DATE.dump"
   mkdir -p "$BACKUP_DIR"

   # Read the password from .env.local — never hardcode it (the live
   # scripts/hosting/backup-postgres.sh greps it from .env.local).
   PGPASSWORD="${PGPASSWORD:?set PGPASSWORD or source .env.local}" pg_dump \
     -h localhost -U scoracle -d scoracle \
     -Fc -Z 6 -f "$FILE"

   # Keep 14 daily + 12 monthly (snapshots taken on day 1 of month are kept longer)
   find "$BACKUP_DIR" -name 'scoracle-*.dump' -mtime +14 ! -name 'scoracle-*01T*' -delete
   find "$BACKUP_DIR" -name 'scoracle-*01T*.dump' -mtime +365 -delete

   ls -lh "$FILE"
   ```

3. Cron entry (04:00 UTC = well after the 23:00 ET seeder, gives
   plenty of room for that to finish):
   ```cron
   0 4 * * * /home/sheneveld/scoracle-data/scripts/backup-postgres.sh >> /home/sheneveld/scoracle-data/logs/backup.log 2>&1
   ```

**Rough sizing:** compressed dump today is probably 50–200MB, grows
maybe 100MB/month as the corpus accumulates. 14 dailies + 12 monthlies
= ~5GB storage ceiling. Trivially cheap.

### Restore drill (do this once)

Don't skip: the only real test of a backup is restoring it.

```bash
# Spin up a throwaway DB (or just use scripts/hosting/restore-drill.sh, which does
# this plus a full bootable-backend proof — see RUNBOOK.md §4)
createdb scoracle_restore
pg_restore -h localhost -U scoracle \
  -d scoracle_restore /mnt/backup/scoracle/scoracle-<date>.dump   # PGPASSWORD from env/.env.local

# Verify: same tables + same row counts
psql scoracle_restore -c "SELECT count(*) FROM news_articles;"
# Compare against production
psql scoracle         -c "SELECT count(*) FROM news_articles;"

dropdb scoracle_restore
```

Do the drill once on day one. Do it again after any schema migration.

---

## D. Log rotation (before logs fill the disk)

The Go API logs to journal automatically via systemd (bounded).
The Python cron logs are plaintext and will grow forever.

Simplest fix: `logrotate` config at `/etc/logrotate.d/scoracle`:
```
/home/sheneveld/scoracle-data/logs/*.log {
    daily
    rotate 14
    compress
    missingok
    notifempty
    copytruncate
}
```

One-line test: `sudo logrotate -d /etc/logrotate.d/scoracle`.

---

## E. Public access (optional)

Only matters if you want a production frontend (Vercel, etc.) hitting
your home box, or if you want to use it from your phone away from
home WiFi.

Three approaches, ranked by hassle:

1. **Tailscale** — zero-config private VPN. Install on home machine
   and on whatever device consumes the API. Gets a stable
   `100.x.x.x` IP that Just Works. Nothing exposed to public
   internet. Free for personal use. **Best choice if you don't need
   truly public access.**

2. **Cloudflare Tunnel** — free, public, auto-TLS, no port
   forwarding or DDNS. Install `cloudflared`, map
   `scoracle.yourdomain.com` to `localhost:8000`. Requires owning a
   domain on Cloudflare. **Best choice if you do need public.**

3. **Traditional**: port forwarding on the router → Caddy reverse
   proxy → Let's Encrypt. Works but most friction. Only if you enjoy
   that kind of thing.

**Recommendation:** Tailscale now, Cloudflare Tunnel when you're
ready to ship publicly.

---

## F. Monitoring (optional)

For a hobby project the basics are enough:

- **systemd status**: `systemctl --user status scoracle-api` tells
  you if the API is up.
- **Health endpoint**: `curl localhost:8000/health/db` for DB
  connectivity.
- **Status script** at `scripts/status.sh` that checks all four:
  ```bash
  #!/usr/bin/env bash
  check() { curl -sf -o /dev/null "$1" && echo "✓ $2" || echo "✗ $2"; }
  check http://localhost:8000/health    "Go API"
  check http://localhost:8000/health/db "Postgres"
  check http://localhost:11434/api/tags "Ollama"
  systemctl --user is-active scoracle-api
  ```

If you want a browser dashboard, **Uptime Kuma** is the self-hosted
go-to — a single Docker container, bookmarks any URL + interval +
alert channel. Overkill unless this ends up serving real users.

---

## Execution order

Concrete scripts now live under `scripts/hosting/` (and `scripts/systemd/`).
Strategy is in this doc; mechanics are in `scripts/hosting/README.md`.

Paste-ready checklist:

1. [ ] `scripts/hosting/install.sh` — copies units, reloads daemon
2. [ ] `sudo loginctl enable-linger $USER`
3. [ ] `systemctl --user enable --now scoracle-api.path scoracle-api.service`
4. [ ] Verify API survives terminal close + reboot + `go build` auto-restart
5. [ ] Pick a backup target (2nd disk / NAS / bucket) + mount if needed
6. [ ] `crontab scripts/hosting/crontab.example`
7. [ ] `sudo cp scripts/hosting/logrotate.conf /etc/logrotate.d/scoracle`
8. [ ] Run first backup manually; then `scripts/hosting/restore-drill.sh`
9. [ ] Cloudflare Tunnel — install `cloudflared`, follow
       `scripts/hosting/cloudflared-config.example.yml`
10. [ ] **Stop here until needed:** status dashboard

---

## Risks to know about

- **Power loss during seed**: Postgres ACID covers you. The partial
  seed-worker state doesn't matter — `event process` is idempotent.
  Re-runs naturally.
- **OS upgrade breaks the unit file**: Arch pacman upgrades have
  broken user systemd paths before. After big updates, re-verify
  `systemctl --user status scoracle-api`.
- **Disk fills from article corpus**: news_articles grows ~1M
  rows/year. At ~1KB per row ≈ 1GB/year. Not tight; but monitor.
- **Ollama VRAM exhaustion**: local-model:tag uses ~8GB. If you add other
  models, watch `nvidia-smi` — they'll swap in/out and slow
  inference dramatically.
- **Port 8000 conflict**: if you install any other web tool locally
  (`uvicorn`, Grafana, etc.) double-check the port.
- **Clock drift on the desktop**: cron schedules drift if NTP is
  disabled. `timedatectl status` should show `NTP service: active`.

---

## Followups worth tracking

- BDL webhooks for NBA/NFL — removes two cron entries once wired
  (see `planning_docs/CRON_SEEDING_STRATEGY.md`)
- Off-site backup copy (cloud) once the corpus has enough value to
  protect against desktop-level catastrophe (house fire, theft)
