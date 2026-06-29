# Scoracle Backend — Operations Runbook

What you need during an incident or a machine rebuild. Companion to:

- `CLAUDE.md` — architecture + route/env conventions
- `ENDPOINTS.md` — API contracts (authoritative route inventory at the top)
- `scripts/hosting/README.md` — script reference
- `planning_docs/SELF_HOSTING_OPS.md` — original strategy / first-time setup

**Source of truth, always:** the code (`go/internal/api/server.go` for routes,
`go/internal/config/config.go` for env, `scripts/hosting/crontab.example` for cron,
`scripts/hosting/release.sh` for the build). Where a doc and the code disagree, the code wins.

Prod runs on **archbox** (Arch desktop): Postgres 18 (system unit), Ollama + `gemma4:e4b`
(`systemd --user`), the Go API (`systemd --user`, `scoracle-api.service`), the Rust Cognition Harness
daemon (`systemd --user`, `scoracle-cognition.service`), cron jobs, and a Cloudflare Tunnel exposing
`api.scoracle.com`.

Post the **Step-3 cutover (2026-06-28)** the Go LLM derive stages are retired; Rust owns all LLM
cognition (scrub → transfers → narratives → vibe → sigil as queue stages, rating as a batch bin).
The Go API serves precomputed data + enqueues scrub work; `DERIVE_WORKER_ENABLED=false` keeps its
derive path off. Rollback is flag-gated (§3).

---

## 1. System map — what runs where

```
                       cron (crontab.example)            systemd --user
  providers ──► seeder ──► Postgres 18 ◄──────────────── scoracle-api.service
 (BDL/SM)     (Python)     │   │   ▲                       ├─ HTTP serving (read-only, precomputed)
                           │   │   │  NOTIFY               ├─ news-scrub ticker (enqueues scrub work)
                           │   │   └──── pipeline_work ────┤  notifications (FCM)
                           │   │              ready        │  pipeline-stats snapshot ticker
                           │   │                            `─ DERIVE_WORKER_ENABLED=false (Step-3)
                           │   └──────► scoracle-cognition.service  ◄── drains the durable
                           │                     │              pipeline_work queue:
                           ▼                     │                scrub, transfers, narratives,
                      Ollama + Gemma  ◄──────────┘                vibe, sigil
                      (single 8GB GPU, OLLAMA_MAX_CONCURRENT=1; the Rust
                       GPU governor + sequential drain ARE the bound)
```

Five deployed binaries, all built from one commit by `release.sh` (3 Go + 2 Rust):

| Binary | Role | Lifecycle |
|---|---|---|
| `scoracle-api` | HTTP serving (precomputed) + enqueue scrub work + maintenance tickers | `scoracle-api.service` (always on) |
| `pipeline` | RSS ingest funnel (`-mode ingest`; the Go LLM drainer is retired) | cron (`cron-pipeline.sh`) |
| `vibesynth` | nightly Sigil reconciliation backstop (DB-only; enqueues durable `sigil` work) | cron (`cron-vibesynth.sh`) |
| `scoracle-cognition` | the Rust daemon: drains scrub → transfers → narratives → vibe → sigil | `scoracle-cognition.service` (always on, GPU box) |
| `statcommentary` | Rust rating batch (single / nightly / backfill, NOT a queue stage) | cron (`cron-rust-statcommentary.sh`) |

The Python seeder runs from the host venv via `cron-scoseed.sh` / `cron-live-fixtures.sh` — it is
ingestion-only and is **not** a `release.sh` binary. The retired `go/bin/statcommentary` binary is
the Step-3 rollback aid (its cron wrapper + crontab restore path) — NOT rebuilt by `release.sh`.

The running API reports its build at `GET /` (`{"commit": "...", "build_time": "..."}`) and logs it
at startup — the authoritative "what's deployed" check for the Go side. The Rust daemon has no HTTP
probe; check its state with `systemctl --user status scoracle-cognition` and the journal.

---

## 2. Release

`scripts/hosting/release.sh` is the **single** release command. It builds all five binaries from one
commit (3 Go + 2 Rust; stamping commit + build time into the Go side), masks both the
`scoracle-api.path` and `scoracle-cognition.path` rebuild watchers during placement, reinstalls the
systemd units, restarts the API + the Rust cognition daemon, and verifies `/health/db` + the served
commit + the daemon's `is-active`.

```bash
cd /home/sheneveld/scoracle/scoracle-backend
git fetch && git status            # confirm synced with origin/main FIRST (CLAUDE.md rule)
scripts/hosting/release.sh          # build all 5 + install + restart + verify
scripts/hosting/release.sh --build-only   # build + place only (no live changes)
```

Why "all five from one commit" matters: a Session-2 audit finding was that hand-built binaries
drifted across commits. `release.sh` builds **every** binary (Go + Rust) before placing **any**, so a
failed build aborts (`set -e`) before a single binary moves — the cron binaries + the daemon can
never end up on a different commit than the API.

**Migrations + restart ordering** (the boot guard, `sql/README-migrations.md`):
`db.New` prepares every statement at boot, validating columns + functions against the live schema,
so a restart against a drifted schema **refuses to boot** instead of serving degraded.

- **Additive migration** (new column/table/function): apply the migration **first**, then
  `release.sh`. `DATABASE_PRIVATE_URL=… ./sql/migrate.sh`
- **Destructive migration** (drop column — F-022 landmine): release the new binary that **no longer
  references** the column **first**, then run the migration. Reverse order = the running binary
  references a dropped column and the next restart won't boot.

After any migration, refresh the versioned schema snapshot and commit it:
`scripts/hosting/snapshot-schema.sh` (keeps `sql/schema/` == live; the CI schema job and the restore
drill both diff against it). **Next free migration number = 107.**

---

## 3. Rollback

```bash
cd /home/sheneveld/scoracle/scoracle-backend
git fetch
git checkout <last-good-commit>     # detached HEAD is fine for a hotfix rollback
scripts/hosting/release.sh          # rebuilds + restarts all 5 (3 Go + 2 Rust) at the good commit
curl -s localhost:8000/ | grep commit   # confirm the served commit matches
```

- **Schema-coupled rollback:** if the bad release shipped a migration, rolling the *binary* back may
  re-introduce the boot guard mismatch (old binary vs new schema). For an **additive** migration the
  old binary boots fine (it just ignores the new column). For a **destructive** one you must restore
  the schema too — see §4 restore, or hand-apply the inverse migration.
- **Pin a running binary** while you investigate (stop the path-watcher from auto-restarting on a
  stray `go build`): `systemctl --user disable --now scoracle-api.path`. Re-arm with
  `systemctl --user enable --now scoracle-api.path`.
- **Never** `pkill` backend processes by name pattern — prod shares the repo `bin/` path and a
  pattern-kill caused a prod outage once (F-001). Always use `systemctl --user restart
  scoracle-api.service` or a PID-specific kill.
- **Step-3 cognition rollback (one-flag, no rebuild):** if the Rust cognition path fails (a stage
  regresses, daemon wedges, etc.), re-arm the Go derive path without rolling the commit:
  ```bash
  # 1. Re-arm Go's derive worker:
  sed -i 's/^DERIVE_WORKER_ENABLED=.*/DERIVE_WORKER_ENABLED=true/' .env.local
  systemctl --user restart scoracle-api.service
  # 2. Stop the Rust daemon (it WILL keep draining if you don't):
  systemctl --user stop scoracle-cognition.service
  # 3. Restore the crontab backup (~/.cache/crontab/crontab.bak) if the Go nightly
  #    statcommentary cron must resume (cron-statcommentary.sh execs go/bin/statcommentary,
  #    the retired binary left in place precisely for this rollback).
  crontab ~/.cache/crontab/crontab.bak
  ```
  Go's derive worker resumes draining from `pipeline_work`. Reverse the steps to flip back to Rust.
  The legacy Go `statcommentary` binary at `go/bin/statcommentary` is the rollback aid; **not** rebuilt
  by `release.sh` — kept deliberately in place post cutover.

---

## 4. Backup & restore

### Backup — `scripts/hosting/backup-postgres.sh` (cron, nightly 04:00 local)

- **Primary:** `pg_dump -Fc -Z 6` to `BACKUP_DIR` (default `/mnt/data/backup/scoracle`, the NVMe).
  Retention: 14 daily + day-01 monthlies for a year. ⚠️ This is on the **same physical drive as
  Postgres** — protects against bad drops/migrations, not drive failure.
- **Off-disk mirror:** copies each dump to `OFFHOST_BACKUP_DIR` (default
  `/home/sheneveld/scoracle-offdisk-backup` on the root SSD — a different drive). Guards: refuses to
  mirror onto the same filesystem, skips (loudly) rather than fill the OS disk, tighter retention
  (`OFFHOST_KEEP_DAYS`=7 + day-01 monthlies). Set `OFFHOST_BACKUP_DIR=""` to disable.
- **Off-SITE is still open (F-040):** the mirror is off-DISK, not off-SITE. Point
  `OFFHOST_BACKUP_DIR` at a NAS/USB/cloud mount to survive losing the host — no code change. Scott
  picks the target before launch.

### Restore drill — `scripts/hosting/restore-drill.sh <dump>` (after every migration; ≥quarterly)

Proves the dump restores into a backend you could actually **boot**, not just data that loads. Five
stages, all fatal-on-failure except where noted:

1. `pg_restore --exit-on-error` (errors are never swallowed).
2. Every critical table exists + is non-empty; row-count drift vs the live source is **informational**
   (the source moved on since the dump) — only missing/empty is fatal.
3. Migration-ledger lineage: the restore must be a **prefix** of the source's `schema_migrations`
   (a version the restore has that the source lacks ⇒ forked lineage ⇒ FAIL; the source being ahead
   is just "N migrations behind", reported).
4. Stable structural objects present (`finalize_fixture`, `enqueue_derive_on_vetted` + its trigger,
   every critical table's PK, the `vibe_scores_trigger_type_check`) — catches `pg_restore` silently
   dropping objects.
5. **Prepared-statement boot check** (`go run ./cmd/validate-stmts`): the restored backend registers
   every prepared statement — i.e. it would boot, not boot degraded.

```bash
# Verify the mirror, not just the primary:
scripts/hosting/restore-drill.sh "$OFFHOST_BACKUP_DIR/scoracle-<date>.dump"
# Drilling a dump that predates the current binary's schema? run migrate.sh on the restore first,
# or skip just the boot check: SKIP_STMT_CHECK=1 scripts/hosting/restore-drill.sh <dump>
```

### Real restore into prod (disaster)

```bash
createdb -h localhost -U scoracle scoracle_new
pg_restore -h localhost -U scoracle -d scoracle_new --no-owner --no-privileges --exit-on-error <dump>
# point DATABASE_PRIVATE_URL at scoracle_new (or rename), then:
DATABASE_PRIVATE_URL=… ./sql/migrate.sh        # catch the restore up to the latest schema
scripts/hosting/release.sh                       # boot the API against it
```

---

## 5. Migrations

- **Apply (prod/incremental):** `DATABASE_PRIVATE_URL=… ./sql/migrate.sh` — applies every file not in
  `public.schema_migrations`, in lexical order, recording each. Idempotent (re-run = no-op). Apply +
  record happen in **one `psql` process**: plain DDL is wrapped with its ledger INSERT in a single
  transaction (crash ⇒ neither applied nor recorded); `CONCURRENTLY`/`VACUUM` files run autocommit;
  self-managed-transaction files self-record before their `COMMIT` (required convention).
- **Fresh environment** (sandbox/dev): `./sql/build.sh "$PROD_URL" "$NEW_ENV_URL"` clones the prod
  **schema only** (incl. `schema_migrations`). **Do not replay** migrations on an empty DB —
  data-dependent gates (045/046/048) fail. Same rule in CI: provision from `sql/schema/schema.sql`
  (after `CREATE ROLE web_user`), never replay.
- Full conventions: `sql/README-migrations.md`. Template: `sql/migration_template.sql`.

---

## 6. Jobs — cron-driven vs event-driven

The correctness path is **event-driven**; cron is **reconciliation/backstop**. No pipeline stage
depends on an ephemeral notification for correctness — every stage hands off through the durable
`pipeline_work` queue, so a missed NOTIFY or a process death never loses work.

### Event-driven (real time, inside `scoracle-api`)

| Trigger | What happens |
|---|---|
| Scrub vets a news link (`enqueue_derive_on_vetted` trigger, migration 103) → `NOTIFY pipeline_work_ready` | The **derive worker** wakes and drains `pipeline_work` in stage order: narratives → Vibe (teams also transfers). Single goroutine, ≤1 Gemma call in flight (shared GPU). |
| A Vibe generation completes | enqueues the terminal **Sigil** convergence for that entity. |
| Rating/Vibe/Momentum input changes | enqueues **one debounced** Sigil convergence (input-hash + round/slope debounce — no duplicate Sigil for unchanged inputs). |
| Startup, and every `DERIVE_DRAIN_INTERVAL_SECONDS` (30s) | the worker re-drains + `RequeueStale` recovers rows abandoned mid-lease — so a missed NOTIFY costs latency, never correctness. |

### Cron-driven (see `scripts/hosting/crontab.example` for exact times — `CRON_TZ=America/New_York`)

| Job | Cadence | Role |
|---|---|---|
| `cron-live-fixtures.sh` (NBA/NFL refresh + process) | refresh 08:xx; process every 30m / :15,:45 | live ingestion (current-season-aware; serializes the shared BDL key) |
| `cron-live-fixtures.sh football-process` | daily 23:00 | drain finished European matches |
| `cron-live-fixtures.sh football-refresh/-meta` | weekly Mon 23:00 | schedule + roster/meta refresh |
| `cron-pipeline.sh -mode corpus` | daily 00:00 | the staged Gemma corpus: sweep → transfers → narratives → vibe (in order; each enriches the next) |
| `cron-statcommentary.sh -mode nightly -limit 400` | daily 03:00 | stats-rail commentary; regenerates only when a rating snapshot's `input_hash` changed |
| `cron-vibesynth.sh -mode nightly -limit 500` | daily 05:00 | **Sigil BACKSTOP only** — enqueues current-season entities whose Sigil is missing/stale for the derive worker to drain; no inline synthesis, no Ollama, no unchanged-Sigil duplicates |
| `recompute-tiers.sh` | weekly Mon 02:00 | entity tier recompute (drives vibe scheduling) |
| `backup-postgres.sh` | daily 04:00 | nightly dump + off-disk mirror (§4) |

**Sigil is event-driven + debounced.** The nightly `vibesynth` line is reconciliation only — it
never creates an unchanged duplicate and never calls Gemma inline.

**Transfers** are a **News scope** (a facet of the news pipeline), even though `/transfers` remains a
supporting per-entity contract.

### Overlap + observability

The three Gemma batch jobs (`pipeline`, `statcommentary`, `vibesynth`) each take a per-job Postgres
advisory lock, so a manual run racing the cron exits 0 cleanly instead of overlapping. Every run is
recorded in `pipeline_runs`:

```sql
SELECT * FROM pipeline_runs_latest;   -- did last night's jobs finish? (no log-grep needed)
```

Cron exit codes: `0` success (or clean overlap-skip) · `3` partial (retryable per-entity failures —
the queue retries) · `1` systemic (enumeration/stage failure, or dead-lettered work remains).

---

## 7. The pipeline: compile → scrub → derive → reveal

The once-daily `cmd/pipeline -mode corpus` run (and the real-time derive worker share one drainer):

```
requeue stale → sweep → scrub(fresh batch) → drain transfers → narratives → vibe → sigil
```

1. **Compile (sweep):** RSS-sweep every team in NBA/NFL/FOOTBALL (no fixture/tier filter, so
   offseason coverage doesn't collapse); persist to `news_articles` / `news_article_entities`.
   Returns the articles that gained a **fresh** link this run.
2. **Scrub:** those exact articles are Gemma ID-gated **in-run** (`vetted=TRUE`). The scrub `UPDATE`
   fires the migration-103 trigger, which is the **sole enqueuer** of derive work — each vetted
   entity's narratives + Vibe (teams also transfers) land on `pipeline_work`.
3. **Derive:** the shared `derive.Drainer` claims → runs → completes/fails each queued stage **in
   declared order** (Claim → run → Complete/Fail). A completed Vibe enqueues its **Sigil**
   convergence.
4. **Reveal:** the precomputed products (`news_summaries`, `transfer_rumors`, `vibe_scores`,
   `sigil_synthesis`, `stat_summaries`) are what the per-product serving endpoints read.

The async maintenance **news-scrub ticker** (`NEWS_SCRUB_*`) is backlog/repair only — it vets the
links the nightly sweep left for it.

**Live table → product names:** `vibe_scores` = Vibe · `sigil_synthesis` = Sigil crown ·
`news_summaries` = narratives · `stat_summaries` (`divined_peak`) = stat commentary ·
`transfer_rumors` = transfers.

---

## 8. Durable work tables & repair commands

| Table | What it holds |
|---|---|
| `pipeline_work` | the derive queue — one row per (entity, stage); claimed under a lease, retried to a cap, then dead-lettered |
| `pipeline_runs` | one row per batch-job run (lock, status, counts) — query via `pipeline_runs_latest` |
| `season_recompute_needed` | seasons flagged for a deferred full rating recompute (finalize defers O(M²) work) |

Operator CLI (`go run ./cmd/work …` from `go/`):

```bash
go run ./cmd/work status                 # pending/running/failed by stage
go run ./cmd/work requeue-stale [lease]  # recover rows abandoned mid-lease (default 15m, e.g. 30m)
go run ./cmd/work dead-letters [cap]     # pipeline_work past the retry cap + fixtures at the seed-retry cap (default 3)
```

If `dead-letters` shows stuck work: inspect the row's last error, fix the cause (often a transient
Ollama timeout or a bad provider payload), then `requeue-stale` (for leased rows) or re-enqueue. A
cron exit `1` with `dead-letters` non-empty is the signal something needs a human.

---

## 9. CI gate (S16 — `.github/workflows/ci.yml`)

Runs on every main-bound change; five jobs:

- **go** — `gofmt`/`vet`/build + `go test -race` (incl. DB-gated queue tests) + `validate-stmts`
  against a `postgres:18` provisioned **from `sql/schema/schema.sql`** (the F-025/F-039 "prepared
  statements register against a migrated test DB" check; `CREATE ROLE web_user` before loading).
- **python** — compile + offline `pytest`.
- **shell** — `bash -n` + ShellCheck (`--severity=warning -x`).
- **docker** — `docker build go/` (the serving artifact). Note: `docker compose build` would fail on
  the `seed` service — `seed/Dockerfile` doesn't exist (F-043, minor).
- **schema** — static: every migration ⊆ snapshot lineage.

DB-backed Go tests gate on `TEST_DATABASE_URL` and skip when unset, so local `go test ./...` /
`pytest` stay offline.

---

## 10. Health, observability, incident quick-reference

```bash
# Is the API up + DB-ready?
curl -s localhost:8000/health/db          # 200 = serving-ready (validates the pool)
curl -s localhost:8000/ | grep commit     # what commit is actually running
systemctl --user status scoracle-api      # process state
journalctl --user -u scoracle-api -f      # API + workers (journal)

# Is Ollama up?
curl -s localhost:11434/api/tags

# Did last night's batch jobs finish?
psql "$DATABASE_PRIVATE_URL" -c "SELECT * FROM pipeline_runs_latest;"
go run ./cmd/work status                   # from go/ — derive queue health

# Cron logs (plaintext, logrotated):
tail -f logs/cron-nba.log logs/pipeline-corpus.log logs/statcommentary.log logs/vibesynth.log logs/backup.log
```

Common incidents:

- **API won't boot after a deploy** → almost always schema drift (the boot guard). Apply the pending
  migration (`sql/migrate.sh`), or roll the binary back (§3). Check `journalctl` for the failing
  prepared statement.
- **Gemma products stale** → check `cmd/work status` / `dead-letters`; check Ollama is up and the
  model is resident (`OLLAMA_KEEP_ALIVE`); a cold GPU reload can time out — the work re-drains.
- **GPU thrash** → `OLLAMA_MAX_CONCURRENT=1` serializes Gemma; the systemd drop-in
  `OLLAMA_NUM_PARALLEL=1` + `OLLAMA_MAX_LOADED_MODELS=1` is **not yet set** (F-035, needs sudo).

---

## 11. Machine rebuild (bare-metal recovery)

Full first-time setup + rationale: `planning_docs/SELF_HOSTING_OPS.md`. Mechanics:
`scripts/hosting/README.md`. Short path:

1. Install Postgres 18, Ollama + `gemma4:e4b`, Go toolchain, the Python venv.
2. Clone the repo; create `.env.local` (DB creds, provider keys, `JWT_SECRET`).
3. Restore the latest **off-disk/off-site** dump (§4) and `sql/migrate.sh` it to the latest schema.
4. `scripts/hosting/install.sh` (renders systemd units), `loginctl enable-linger sheneveld`,
   `systemctl --user enable --now scoracle-api.path scoracle-api.service`.
5. `crontab scripts/hosting/crontab.example`; `sudo cp scripts/hosting/logrotate.conf
   /etc/logrotate.d/scoracle`.
6. `scripts/hosting/release.sh` to build/stamp/verify; `scripts/hosting/restore-drill.sh` to prove
   the backup is bootable.
7. Cloudflare Tunnel (`cloudflared`, `cloudflared-config.example.yml`) for `api.scoracle.com`.

---

## 12. Launch-gate carryovers (tracked, not yet done)

These surfaced during the audit and are pre-launch work (see `planning_docs/FIRST-GPT-AUDIT-FINDINGS.md`):

- **F-030** — NFL (1072) + FOOTBALL (2147) current-season entities have **zero** season-2025-stamped
  Sigils. Run a larger reconcile/backfill (`vibesynth -mode nightly` with a higher `-limit`, or a
  dedicated backfill) before launch.
- **F-040** — pick the off-SITE backup target (cloud/NAS); mechanism is ready via
  `OFFHOST_BACKUP_DIR`.
- **F-035** — set the Ollama systemd drop-in `OLLAMA_NUM_PARALLEL=1` + `OLLAMA_MAX_LOADED_MODELS=1`
  (needs sudo).
- **F-043** — `docker-compose build: seed/` references a non-existent `seed/Dockerfile` (minor).
- **F-045** — regenerate Swagger (`swag init`) so `/docs/` stops advertising the removed `/twitter/*`
  + `/api/v1/news/*` routes; ships on the next `release.sh`.
- **F-046 🔴 (security)** — credential leak, scope wider than first thought: **4 distinct secrets** (Neon
  cloud pw, local archbox `scoracle` pw, `API_SPORTS_KEY`, `TWITTER_BEARER_TOKEN`) across **3 repos**
  (`scoracle-backend`, `dotfiles`, the capital-`Scoracle` legacy clone) and a historically-tracked
  `.env.local`. **S18 done:** working tree fully scrubbed + `.claude/settings.local.json` untracked +
  gitignored. **Still gated on Scott:** rotate/revoke all 4 (the only real fix — treat as compromised),
  then purge history (`git filter-repo` — install first — + force-push, coordinating archbox + archx220
  + the Rust session). **Repair runbook: `PASSWORD-LEAK-REPAIR.md`** (repo root — Steps 1–3, redacted
  re-derivation, rollback, next-session prompt). Full scope:
  `planning_docs/FIRST-GPT-AUDIT-FINDINGS.md` F-046 +
  `progress_docs/2026-06-24_F-046-credential-leak-remediation.md`.

The remaining pre-launch milestone is the **Final launch gate** in `FIRST-GPT-AUDIT.md` —
stats/news/convergence/operations end-to-end proofs, per sport.
