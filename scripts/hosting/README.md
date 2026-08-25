# Self-Hosting Scripts

Everything needed to run Scoracle as a proper service on the Arch
desktop: systemd units, cron wrappers, Postgres backups, log rotation,
Cloudflare Tunnel stub.

See `run_docs/RUNBOOK.md` for the operations runbook (release/rollback, backup/restore,
jobs, durable work queue, incident quick-reference) and `../../../scoracle-wiki/progress_docs/scoracle-backend/SELF_HOSTING_OPS.md`
for the original strategy + rationale.

## Install

```bash
scripts/hosting/install.sh
```

The installer is safe to re-run. It **renders** the systemd units (substituting
the real repo root for the `__SCORACLE_REPO_ROOT__` placeholder) and sets
permissions; it never touches crontab or sudo-gated state. The script prints the
remaining manual steps at the end.

Because the units are templated, a clone in **any** location installs correct
paths — there is no hardcoded path to edit. To inspect the rendered units
without touching the live ones:

```bash
SCORACLE_SYSTEMD_DIR=$(mktemp -d) scripts/hosting/install.sh
```

## Release

```bash
scripts/hosting/release.sh                # build all 5 binaries + install + restart + verify
scripts/hosting/release.sh --build-only   # build + place binaries only (no live changes)
```

`release.sh` is the single release command: post the Step-3 cutover it builds
the three live Go binaries (`scoracle-api`, `pipeline`, `vibesynth`) and the two
Rust cognition binaries (`scoracle-cognition` daemon, `statcommentary` rating
batch) **from one commit**, stamps the commit + build time into the Go binaries
(queryable at `GET /` and logged at startup), masks both the `scoracle-api.path`
and `scoracle-cognition.path` rebuild watchers during placement, (re)installs
the units, restarts the API + the Rust daemon, and verifies `/health/db`. All
five binaries are built before any is placed, so a failed build can never leave
the cron binaries or the daemon on a different commit than the API.

## What's in here

| File | Purpose |
|---|---|
| `../systemd/scoracle-cognition.service` | systemd user unit (templated) — long-running Rust cognition daemon |
| `../systemd/scoracle-cognition.path` | path watcher — auto-restart when a Rust binary is deployed to `rust/bin/` |
| `../systemd/scoracle-cognition-restart.service` | oneshot restart helper fired by the cognition path watcher |
| `../systemd/scoracle-api.service` | systemd user unit (templated) — long-running Go API |
| `../systemd/scoracle-api.path` | path watcher — auto-restart when `go build` replaces the binary |
| `../systemd/scoracle-api-restart.service` | oneshot restart helper fired by the path watcher |
| `../systemd/cloudflared.service` | CF Tunnel runner |
| `release.sh` | single release command — build all 5 binaries (3 Go + 2 Rust) from one commit, install, restart, verify |
| `cron-pipeline.sh` | wrapper for the Go ingestion binary (`-mode ingest` — the only data ingestion layer; RSS sweep, Rust curates) |
| `cron-narrative-links.sh` | nightly narrative-graph co-mention refresh (pure SQL, mig 154) |
| `cron-rust-statcommentary.sh` | wrapper for the Rust stats-rail rating batch (the post Step-3 cutover path) |
| `cron-stat-matchups.sh` | nightly stat-matchup refresh (pure SQL, mig 156) |
| `cron-vibesynth.sh` | wrapper for nightly Sigil reconciliation (DB-only enqueue) |
| `recompute-tiers.sh` | weekly entity-tier recomputation |
| `crontab.example` | paste-ready crontab — nightly ingest/derive window, weekly tiers, nightly backup |
| `backup-postgres.sh` | nightly `pg_dump` with 14-daily + 12-monthly retention |
| `restore-drill.sh` | tests a backup restore into a throwaway DB and diffs row counts |
| `tunnel-smoke.sh` | endpoint smoke test (local or via CF Tunnel) |
| `logrotate.conf` | daily rotation + 14-day retention for `logs/*.log` |
| `cloudflared-config.example.yml` | template for `~/.cloudflared/config.yml` |
| `install.sh` | one-shot installer; renders units, prints remaining manual steps |

## The rebuild gotcha — solved

Previously: after `go build`, the disk binary was fresh but the running
service was stale. Easy to miss in a dev loop.

Now: `scoracle-api.path` watches the binary via inotify. The moment
`go build -o bin/scoracle-api ./cmd/api` finishes its atomic rename,
systemd restarts the service. No mental tax.

Disable with `systemctl --user disable scoracle-api.path` if you need
to pin a running binary while the source changes — useful during
long-running tests.

## Logs

```bash
# API + listener + maintenance (goes to journal)
journalctl --user -u scoracle-api -f

# Cron (plaintext, rotated by logrotate)
tail -f logs/pipeline-ingest.log
tail -f logs/narrative-links.log
tail -f logs/statcommentary.log
tail -f logs/vibesynth.log
tail -f logs/backup.log

# Cloudflare Tunnel
journalctl --user -u cloudflared -f
```
