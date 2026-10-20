# Self-Hosting Scripts

Everything needed to run Scoracle as a proper service on the Arch
desktop: systemd units, cron wrappers, Postgres backups, log rotation,
Cloudflare Tunnel stub.

See `run_docs/RUNBOOK.md` for the operations runbook (release/rollback, backup/restore,
jobs, durable work queue, incident quick-reference) and `../../../scoracle-wiki/progress_docs/scoracle-backend/SELF_HOSTING_OPS.md`
for the original strategy + rationale.

## Current production release

The [October 20 release](../../rust/docs/headline-qwen35-deployment-2026-10-20.md)
is live from commit `e4fa9bd3`, with migration 315 and matching Go/Rust binaries.
Archbox runs known-entity headline scoring and hourly team RSS; the Mac runs the
six character stages. Influencer uses Qwen3.5-4B Q4_K_M with compact instructions
and score-first decoding. Other character routes retain SmolLM3; d1 is unchanged.
The retired source-worker and browser-reader units were removed.

The release record includes verification and coordinated recovery. The
[headline corpus](../../rust/docs/headline-corpus.md) and
[entity pipeline contract](../../rust/docs/headline-entity-pipeline.md) describe
current evidence and identity handling. Older deployment notes below retain
their original historical context.

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
scripts/hosting/release.sh                # build 7 binaries; restart API and previously active workers
scripts/hosting/release.sh --build-only   # build + place binaries only (no live changes)
```

`release.sh` is the single release command: post the Step-3 cutover it builds
the four live Go binaries (`scoracle-api`, `pipeline`, `vibesynth`, `scoracle-memory-study`) and the three
Rust cognition binaries (`scoracle-cognition` daemon, `statcommentary` rating
batch, and `factsweep`) **from one commit**, stamps the commit + build time into the Go binaries
(queryable at `GET /` and logged at startup), masks both the `scoracle-api.path`
and `scoracle-cognition.path` rebuild watchers during placement, (re)installs
the units, restarts the API + the Rust daemon, and verifies `/health/db`. All
seven binaries are built before any is placed, so a failed build can never leave
the cron binaries or the daemon on a different commit than the API.

## Headline entity cleanup

Migration 315 and matching Go/Rust binaries are deployed. Headline extraction
includes known players and all canonical person roles; reporting APIs accept
`person`. The browser reader and source worker are removed. Unknown-person
discovery remains a subsequent session task. See the
[entity pipeline change](../../rust/docs/headline-entity-pipeline.md).

## Graph retirement

Deployed October 16, 2026: migrations 307–308 and the matching Archbox worker are live. The Mac worker was updated and remains paused. See the [deployment record](../../rust/docs/graph-retirement-deployment-2026-10-16.md) for verification and recovery locations.

Graph inference and relationship maintenance are retired. Unknown entity search
is deferred to a future Investigator change. Keep the historical evidence tables;
DuckDB and older reporting readers can still use retained records.

For the coordinated release, stop cognition and source workers and their restart
watchers on every enrolled host. Apply migrations through 308, remove `graph` from
`COGNITION_STAGES` in `.env.local` and other worker configurations, and remove
`COGNITION_ROUTE_EMOTIONAL_NEWS_*` overrides. Install the matching Graph-free binary
before restarting previously active workers and watchers. An old binary still
tries to enqueue Graph from acquisition and will fail against migration 308.

Migration 308 removes outstanding Graph claims, fencing late publications, and
rejects new Graph queue rows. It retains sources, extractions, candidates, persons,
relationships and story history. The existing `cron-narrative-links.sh` filename
remains valid, but now runs only source-outcome, existing-entity and calendar
maintenance; it no longer refreshes relationships or promotes legacy discoveries.
`verify-classifier.sh` requires the retirement migration and queue constraint.

## Headline Classifier operation

Archbox runs `COGNITION_STAGES=classifier`, `CLASSIFIER_URL`,
`CLASSIFIER_REPLAY_BACKLOG=true` and the shared coverage controller. The hourly
`scoracle-coverage.service` searches team RSS feeds; canonical headline entities
are scored independently and qualifying reporting deliveries freeze into the
nightly edition. The Mac runs the character stages. d1 and character model routes
are unchanged by the source cleanup.

`run-classifier-model.sh` starts the configured native checkpoint with a SHA-256
model alias. Its checkpoint settings live in
`~/.config/scoracle/classifier-model.env`; the worker refuses an unfingerprinted
model identity. The historical service name `scoracle-kev.service` remains for
host compatibility. The production packet has five headline questions; old
article-reader measurements do not qualify its model quality or throughput.

`release.sh` performs schema preflight before placing binaries. It restarts only
previously active cognition/coverage workers. It never starts the retired source
worker. `--keep-cognition-paused` requires cognition and its watcher to be stopped.
For an isolated build, set `RELEASE_BIN_DIR=<scratch>` and `--build-only`.

The watchdog recognizes `paused` and `classifier` modes. It checks active stages
separately; model error receipts do not count as successful progress. Run its
regressions with an empty migrated `classifier_test` database:
`TEST_DATABASE_URL=<disposable URL> python3 scripts/hosting/test-classifier-watchdog.py`.
The release/install simulation is `python3 scripts/hosting/test-classifier-release.py`.

Saved evidence, publication fences, daily schedules, source attribution and
unknown values remain intact. Historical activation and recovery records live in
[the October 19 deployment record](../../rust/docs/headline-plumbing-deployment-2026-10-19.md).

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
| `release.sh` | single release command — build all 7 binaries (4 Go + 3 Rust) from one commit, install, restart, verify |
| `cron-pipeline.sh` | wrapper for the Go ingestion binary (`-mode ingest` — the only data ingestion layer; RSS sweep, Rust curates) |
| `cron-narrative-links.sh` | reporting maintenance: source outcomes, existing-entity refresh and reporting calendar |
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

`journalctl --user -u scoracle-cognition -f` shows headline scoring and queue progress.

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

The API embeds DuckDB for bounded cohort studies. Release builds require CGO and a C/C++ toolchain; `release.sh` enables CGO. The API starts the single-producer refresh on startup and every five minutes.
