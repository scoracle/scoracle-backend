# First GPT Audit — Session 2: Service paths, release mechanics, health checks

**Worked:** 2026-06-21 (archbox)

**Plan:** `planning_docs/FIRST-GPT-AUDIT.md`, Session 2

**Baseline:** Session 1 (`progress_docs/2026-06-21_first-gpt-audit-session-1-production-baseline.md`, commit `9e2b7fb`)

**Product authority:** wiki `Product Narrative`

## Goal

Make a clean machine reproduce the intended backend deployment without manual path surgery, give
the system one reproducible release command, and make health/readiness reflect actual serving
ability — closing Session 1 findings #1 (stale `.path` watcher), #2 (binaries not revision-stamped),
and #3 (API + cron binaries deployed from different commits).

## Decisions (confirmed with Scott)

1. **Production fails fast when Postgres is unreachable.** Every serving endpoint is a precomputed
   read from Postgres, so a DB-less API serves nothing. In production the API now `exit(1)`s at
   startup if it can't connect; systemd's `Restart=always`/`RestartSec=3` brings it straight back
   when Postgres returns. Non-production keeps degraded startup (local dev / CI can boot the HTTP
   surface without a DB). Chosen over degraded-mode-with-reconnect because it matches the audit's
   guiding principle (explicit/durable over clever recovery) and the proven live restart policy.
2. **Repo changes + non-destructive verification only.** No live systemd/cron reinstall and no
   production API restart this session. Scott runs `scripts/hosting/release.sh` to deploy.
3. **Templated unit paths over hardcoded.** The systemd units carry a `__SCORACLE_REPO_ROOT__`
   placeholder rendered by `install.sh`, so any clone location installs correct paths.

## What changed

### systemd templates (`scripts/systemd/`)

- `scoracle-api.service`: stale `/home/sheneveld/scoracle-backend` → `__SCORACLE_REPO_ROOT__`
  placeholder; restart policy aligned with the proven live policy (`Restart=always`, `RestartSec=3`,
  `StartLimitIntervalSec=0` in `[Unit]`); added `SyslogIdentifier=scoracle-api` to match live.
- `scoracle-api.path`: **fixed the broken watcher** — `PathChanged` pointed at the stale
  `/home/sheneveld/scoracle-backend/go/bin/`, so auto-restart-on-rebuild never fired (Session 1
  finding #1). Now `__SCORACLE_REPO_ROOT__/go/bin/`.
- `scoracle-api-restart.service`, `cloudflared.service`: no repo paths; rendered unchanged.

### Installer (`scripts/hosting/install.sh`)

- Renders units (sed-substitutes the placeholder) instead of copying stale paths.
- `chmod +x scripts/hosting/*.sh` — now covers `cron-pipeline.sh`, `cron-statcommentary.sh`,
  `cron-vibesynth.sh` (previously omitted), plus all other wrappers.
- `SCORACLE_SYSTEMD_DIR` override renders into a scratch dir for inspection (skips daemon-reload).

### Release command (`scripts/hosting/release.sh`, new)

- Builds `scoracle-api`, `pipeline`, `statcommentary`, `vibesynth` from one commit.
- Stamps commit + build time via `-ldflags -X .../buildinfo.{Commit,BuildTime}`.
- **Atomic across siblings:** builds all four into a staging dir on the same filesystem, then
  moves them in — a failed build aborts before any binary is placed, so the cron binaries can never
  land on a different commit than the API (Session 1 finding #3).
- (Re)installs units, restarts the API, polls `/health/db`, and confirms the served commit matches.
- `--build-only` + `RELEASE_BIN_DIR` for non-destructive verification.

### Go API

- `internal/buildinfo/` (new): `Commit`/`BuildTime` vars stamped at link time; logged at startup and
  reported at `GET /` (Session 1 finding #2 — running process is now mappable to a source commit).
- `cmd/api/main.go`: added `syscall.SIGTERM` to graceful-shutdown signals (systemd `stop`/`restart`
  and container runtimes send SIGTERM, not SIGINT); production fail-fast on DB connect failure.
- `internal/api/handler/handler.go`: `/health` and `/health/db` now share a `dbReady()` check and
  return **503 when the DB is unreachable** — a readiness probe pointed at `/health` no longer sees a
  DB-less API as healthy. `GET /` reports `commit`/`built`.
- `internal/api/server_test.go`: `TestHealthReadinessRequiresDB` asserts both endpoints return 503
  with no pool.

### Docker (`go/Dockerfile`)

- Builds all four binaries (was API-only) from one commit with `GIT_COMMIT`/`BUILD_TIME` build-args;
  copies all into the runtime image (ENTRYPOINT still the API). Note: self-hosted prod deploys via
  `release.sh`, not Docker — Docker is the local-dev stack.

## Operational finding

`cd` is wrapped on archbox to echo its target directory, and that leaks into non-interactive scripts
here. `$(cd … && pwd)` therefore captured the echo **plus** `pwd`, doubling `REPO_ROOT` and breaking
the sed render. `install.sh`/`release.sh` now redirect `cd`'s stdout (`cd … >/dev/null && pwd`).
Existing hardcoded-`cd` cron wrappers are unaffected (they don't capture `cd` in a substitution).

## Verification

- `go vet ./...`, `gofmt -l` (clean), `go test ./... -count=1` — all pass.
- `systemd-analyze --user verify` on all four rendered units — no directive errors.
- Temp-dir render: canonical paths everywhere, no `__SCORACLE_REPO_ROOT__` leak, `.path` watcher
  now points at the real bin dir.
- `release.sh --build-only` (temp `RELEASE_BIN_DIR`): all four binaries built, stamped
  `9e2b7fbb55a1-dirty`, atomically placed, staging dir cleaned up by trap.
- Runtime (port 18080, unreachable DB, live API on 8000 untouched):
  - production → `exit 1` ("refusing to start in production");
  - development → stays up, `/health` and `/health/db` both **503**, `GET /` reports the commit;
  - **SIGTERM** → "Shutting down…" → "Server stopped" → **exit 0** (graceful).

### Audit "Done when" — A clean machine can reproduce the deployment without manual path surgery

Met for the repo artifacts. Not yet applied to the live machine (Decision 2): the running API still
serves the Session-1 binary, the installed `.path` watcher is still the stale one, and the live
restart policy is unchanged until Scott runs `release.sh`.

## Deliberately NOT done (scope discipline)

- No live deploy / unit reinstall / production restart (Decision 2).
- `tunnel-smoke.sh` still labels `/health` "Liveness" and still probes retired `/news/status` +
  `/twitter/status` routes — documentation/route reconciliation is **Session 17**.
- `cmd/api/main.go` Swagger `@description` still mentions "journalist tweets" — **Session 17**.
- Did not touch the unrelated untracked `planning_docs/TOP_DOWN_ROSTER_COVERAGE.md` (parallel session).

## To deploy (when ready)

```bash
scripts/hosting/release.sh      # build all 4 @ one commit, reinstall units, restart, verify health
```

Re-enable the now-correct path watcher (it was stale): `systemctl --user restart scoracle-api.path`
is implicit via `install.sh` daemon-reload, but confirm with
`systemctl --user show scoracle-api.path -p Paths`.

## Files changed

- `go/internal/buildinfo/buildinfo.go` (new)
- `go/cmd/api/main.go`
- `go/internal/api/handler/handler.go`
- `go/internal/api/server_test.go`
- `go/Dockerfile`
- `scripts/systemd/scoracle-api.service`
- `scripts/systemd/scoracle-api.path`
- `scripts/hosting/install.sh`
- `scripts/hosting/release.sh` (new)
- `scripts/hosting/README.md`
- `progress_docs/2026-06-21_first-gpt-audit-session-2-service-paths-release-health.md` (this doc)
