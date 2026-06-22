# Self-Hosting Scripts

Everything needed to run Scoracle as a proper service on the Arch
desktop: systemd units, cron wrappers, Postgres backups, log rotation,
Cloudflare Tunnel stub.

See `planning_docs/SELF_HOSTING_OPS.md` for the strategy + rationale.

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
scripts/hosting/release.sh                # build all 4 binaries + install + restart + verify
scripts/hosting/release.sh --build-only   # build + stamp + place binaries only (no live changes)
```

`release.sh` is the single release command: it builds `scoracle-api`,
`pipeline`, `statcommentary`, and `vibesynth` **from one commit**, stamps the
commit + build time into them (queryable at `GET /` and logged at startup),
(re)installs the units, restarts the API, and verifies `/health/db`. All four
binaries are built before any is placed, so a failed build can never leave the
cron binaries on a different commit than the API.

## What's in here

| File | Purpose |
|---|---|
| `../systemd/scoracle-api.service` | systemd user unit (templated) — long-running Go API |
| `../systemd/scoracle-api.path` | path watcher — auto-restart when `go build` replaces the binary |
| `../systemd/scoracle-api-restart.service` | oneshot restart helper fired by the path watcher |
| `../systemd/cloudflared.service` | CF Tunnel runner |
| `release.sh` | single release command — build all 4 binaries from one commit, install, restart, verify |
| `cron-scoseed.sh` | wrapper that loads `.venv` + env vars so cron can invoke `scoracle-seed` |
| `cron-live-fixtures.sh` | current-season-aware live polling wrapper for NBA/NFL/Football fixture jobs |
| `cron-pipeline.sh` | wrapper for the staged Gemma pipeline (sweep → transfers → narratives → vibe) |
| `cron-statcommentary.sh` | wrapper for the stats-rail commentary job |
| `cron-vibesynth.sh` | wrapper for nightly Sigil reconciliation/backfill |
| `recompute-tiers.sh` | weekly entity-tier recomputation |
| `crontab.example` | paste-ready crontab — NBA/NFL polling, football refresh/drain, nightly backup |
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
tail -f logs/cron-nba.log
tail -f logs/cron-nfl.log
tail -f logs/cron-football.log
tail -f logs/backup.log

# Cloudflare Tunnel
journalctl --user -u cloudflared -f
```
