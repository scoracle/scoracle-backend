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

## Classifier cutover with cognition paused

The replacement is prepared on main. Production activation is a separate action.
Keep daily RSS cron and `DERIVE_WORKER_ENABLED=false`. Harvester/Editor stage lists
are rejected by the new daemon. `scoracle-classifier-source.service` forces
`classifier_acquire` after loading the environment, initializes no model host, and
can retain full sources while all inference remains stopped.

On the deployment host, after a backup and checkout of the intended commit:

```bash
set -a; source .env.local; set +a
sql/migrate.sh                         # apply 291–298 before matching binaries
scripts/hosting/verify-classifier.sh  # read-only schema and durable work report
scripts/hosting/release.sh --keep-cognition-paused
```

`release.sh` checks the native schema before placement. It never starts previously
inactive cognition, refuses a paused release when cognition or its watcher is active,
and restarts acquisition only if that separate worker was already running. It renders
units without enabling them. For an isolated build, use both `RELEASE_BIN_DIR=<scratch>`
and `--build-only`; build-only without redirection still replaces live files.

Update `.env.local` so `COGNITION_STAGES` matches the new cognition unit: classifier,
graph, investigate_entity, fixture_boxscore and the six existing character task names.
Acquisition is owned by the separate source unit. To enable the Classifier, set
`CLASSIFIER_URL` to the llama.cpp server exposing `/v1/systemone` (the Kev decision model);
the daemon refuses to start the classifier stage without it. The spectrum, questions and gate
live in `rust/src/plugins/classifier/decision.rs` and
`fixtures/classifier/classification-criteria-v1.json`. The retired GLiNER/candle model and the
generative `COGNITION_ROUTE_CLASSIFIER_*` settings are gone.

The October 15 native path uses `scoracle-kev.service` on Archbox with eight
8,192-token decision slots and four concurrent article workers. The extra native slots let
all six questions share one article prefill instead of splitting across two groups. Install the CUDA-compatible `llama-server` at
`~/.local/bin/scoracle-llama-server` and `Kev-0.8B-Q8_0.gguf` in `~/models/`.
`run-classifier-model.sh` hashes the GGUF and uses that fingerprint as its serving alias;
the client rejects an unversioned or different-model alias. Apply migrations 302–303 before
the matching worker: it retains request/reply receipts, source heads and separate spectra
for changed sources, and supersedes pending HTML/partial measurements.

`CLASSIFIER_URL=http://127.0.0.1:8081` is independent of character routing.
All generative routes should use SmolLM3 on the Mac via native llama.cpp, with one
shared four-slot budget (32,768 tokens per SmolLM3 slot). The deployed loopback SSH tunnel is
`127.0.0.1:18080` on Archbox to `127.0.0.1:8080` on the Mac; set
`MODEL_BASE_URL=http://127.0.0.1:18080`, `MODEL_NAME=alibayram/smollm3`, and
`COGNITION_BACKEND_CONCURRENCY=http://127.0.0.1:18080=4`. Remove obsolete per-route
host/backend overrides. During plumbing verification, `COGNITION_STAGES=classifier`
runs decision work while character work remains queued. Enable the full task list when
that work is ready to run. `CLASSIFIER_REPLAY_BACKLOG=true` explicitly enables acquisition
backlog replay; RSS continues to enqueue newly discovered articles independently.

On the Mac, `com.scoracle.smollm3` and `com.scoracle.model-tunnel` in
`~/Library/LaunchAgents/` keep the loopback server and SSH forwarding alive.
The tunnel also exposes Archbox Kev locally at `127.0.0.1:18081` for read-only replays.
The October 15 cutover retained a focused Classifier backup at
`/home/sheneveld/tmp/classifier-before302-20261015.dump` on Archbox (about 2 GB).
Workers are running the tested October 15 source snapshot; its archive and binary hashes
are retained in `/home/sheneveld/tmp/kev-native-deployment-20261015.sha256`.

Calibration follows plumbing qualification. The six questions and thresholds are unchanged.
A failed/oversized decision creates an error receipt and retries through the existing durable
queue; it cannot score only the opening and dispatch character work as complete evidence.
Watchdog detects paused/acquire/classifier mode from active services; `WATCHDOG_MODE`
overrides detection for another host.

Recovery keeps stored discovery/sources/receipts, replays acquisition idempotently and
uses normal retry/lease/outbox recovery. Stop the source worker to pause retrieval;
stop cognition and its watcher to pause inference. Avoid rolling back to retired intake
or deleting/down-migrating retained receipts. Restore a backup into an isolated database
and run `restore-drill.sh` in its default Classifier mode; it migrates the restore and
verifies the native schema and API prepared statements before it is considered usable.

Runnable checks: `python3 scripts/hosting/test-classifier-release.py` and
`TEST_DATABASE_URL=<empty migrated classifier_test database> python3 scripts/hosting/test-classifier-watchdog.py`.

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

`journalctl --user -u scoracle-classifier-source -f` shows independent acquisition.

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
