# Scoracle Backend — Operations Runbook

What you need during an incident or a machine rebuild. Companion to:

- `README.md` — repo entry point, architecture, route/env overview
- `docs/DEVELOPMENT.md` — development rules and implementation boundaries
- `ENDPOINTS.md` — API contracts (authoritative route inventory at the top)
- `scripts/hosting/README.md` — script reference
- `../scoracle-wiki/progress_docs/scoracle-backend/SELF_HOSTING_OPS.md` — original strategy / first-time setup

**Source of truth, always:** the code (`go/internal/api/server.go` for routes,
`go/internal/config/config.go` for env, `scripts/hosting/crontab.example` for cron,
`scripts/hosting/release.sh` for the build). Where a doc and the code disagree, the code wins.

Prod runs on **archbox** (Arch desktop): Postgres 18 (system unit), Ollama (the small busy-work
model, `systemd --user`), the Go API (`systemd --user`, `scoracle-api.service`), the Rust
Cognition Harness daemon (`systemd --user`, `scoracle-cognition.service`), cron jobs, and a
Cloudflare Tunnel exposing `api.scoracle.com`.

Post the **Step-3 cutover (2026-06-28)** and follow-up Go prune (2026-06-29), the Go LLM derive
stages are retired; Rust owns all LLM cognition (scrub -> transfers -> narratives ->
vibe -> sigil as queue stages, rating as a batch bin). The Go API serves precomputed data and runs
SQL-only maintenance/notification workers.

---

## 1. System map — what runs where

```
                       cron (crontab.example)            systemd --user
 Google News ──► pipeline ──► Postgres 18 ◄──────────── scoracle-api.service
   (RSS)         (Go)        │   │   ▲                      ├─ HTTP serving (read-only, precomputed)
                             │   │   │  NOTIFY              ├─ SQL maintenance (pipeline-stats,
                             │   │   └──── pipeline_work ───┤   ranks, cohorts, cleanup)
                             │   │              ready       │  notifications/listener (FCM + enqueue)
                             │   └──────► scoracle-cognition.service  ◄── durable queue stages:
                             │                     │                editor, investigate_entity, graph,
                             │                     ▼                transfers, narratives, vibe,
                             │                 statcommentary        peak, momentum, sigil
                             │                  (cron batch)
                             ▼
              Ollama (archbox 1070 Ti) + Mac mini model host (§1.1)
```

### 1.1 Model topology — two machines, two models, model-agnostic by design

The cognition daemon routes each seat to a model@host resolved from the
`COGNITION_ROUTE_<ROLE>[_BASE_URL]` env keys (`rust/src/route.rs`) — nothing about
a specific model is compiled in. The CURRENT pinning is a hardware-constrained
choice, not a contract: as hardware improves, any seat can move model or machine
by config alone, and the fixture gates (`eval --task <seat> --fixtures`,
`scripts/hosting/model-gate.sh`) exist to prove a candidate model keeps the voice
before it ships.

| Machine | Hardware | Model | Seats |
|---|---|---|---|
| **archbox** | 1070 Ti (Ollama, `localhost:11434`) | `ministral-3:3b` | the LOW-THOUGHT BUSY WORK: Editor (`editor`), Investigator (`investigator`), Graph (`emotional-news`), plus utility roles (`sql`, `multilang`) |
| **Mac mini** | `192.168.1.77:8000` | `ministral-3:8b` | the CHARACTER WORK THAT SURFACES: Journalist (`narrative-logic`), Insider (`transfer-logic`), Influencer (`vibe-logic`), Analyst (`momentum-logic`), Scout (`stats-logic`), Oracle (`oracle-logic`) |

Five deployed binaries, all built from one commit by `release.sh` (3 Go + 2 Rust):

| Binary | Role | Lifecycle |
|---|---|---|
| `scoracle-api` | HTTP serving (precomputed) + enqueue Editor reads at ingest + maintenance tickers | `scoracle-api.service` (always on) |
| `pipeline` | the ONLY data ingestion layer: nightly Google News RSS sweep (persist + enqueue the Editor's read) | cron (`cron-pipeline.sh`) |
| `vibesynth` | nightly Sigil reconciliation backstop (DB-only; enqueues durable `sigil` work) | cron (`cron-vibesynth.sh`) |
| `scoracle-cognition` | the Rust daemon: drains editor → investigate_entity → graph → transfers → narratives → vibe → peak → momentum → sigil | `scoracle-cognition.service` (always on, GPU box) |
| `statcommentary` | Rust rating batch (single / nightly / backfill, NOT a queue stage) | cron (`cron-rust-statcommentary.sh`) |

Google does the relevancy work at fetch time; the Rust junctions curate everything
downstream. There is no other ingestion path — no provider clients, no live polling.

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
git fetch && git status            # confirm synced with origin/main FIRST (README rule)
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

### Event-driven (real time; queue is drained by Rust)

| Trigger | What happens |
|---|---|
| Scrub vets a news link (`enqueue_derive_on_vetted` trigger, migration 103) → `NOTIFY pipeline_work_ready` | Durable `pipeline_work` items are created; `scoracle-cognition` drains stage order and retries on failure. |
| A Vibe generation completes | enqueues the terminal **Sigil** convergence for that entity. |
| Rating/Vibe/Momentum input changes | enqueues **one debounced** Sigil convergence (input-hash + round/slope debounce — no duplicate Sigil for unchanged inputs). |
| Startup + safety-net cadence (`COGNITION_SAFETY_NET_SECONDS`) | the Rust worker re-drains + `RequeueStale` recovers rows abandoned mid-lease — a missed NOTIFY costs latency, never correctness. |

### Cron-driven (see `scripts/hosting/crontab.example` for exact times — `CRON_TZ=America/New_York`)

| Job | Cadence | Role |
|---|---|---|
| `cron-pipeline.sh -mode ingest` | daily 02:00 | THE ONLY DATA INGESTION LAYER: RSS sweep, persist + enqueue the Editor's read; durable queue stages derive the products |
| `cron-narrative-links.sh` | daily 02:45 | narrative-graph co-mention refresh (pure SQL); cadence is the heating/cooling baseline |
| `cron-rust-statcommentary.sh -mode nightly -limit 400` | daily 03:00 | stats-rail commentary + PEAK trajectory metadata; regenerates only when a rating snapshot's `input_hash` changed |
| `cron-stat-matchups.sh` | daily 03:30 | stat-matchup refresh (pure SQL) |
| `cron-vibesynth.sh -mode nightly -limit 500` | daily 05:00 | **Sigil BACKSTOP only** — enqueues current-season entities whose Sigil is missing/stale for Rust to drain; no inline synthesis, no unchanged-Sigil duplicates |
| `recompute-tiers.sh` | weekly Mon 02:00 | entity tier recompute (drives vibe scheduling) |
| `backup-postgres.sh` | daily 04:00 | nightly dump + off-disk mirror (§4) |

**Sigil is event-driven + debounced.** The nightly `vibesynth` line is reconciliation only — it
never creates an unchanged duplicate and never calls the model inline.

**Transfers** are a **News scope** (a facet of the news pipeline), even though `/transfers` remains a
supporting per-entity contract. Transfers and Narratives share the same historical scopes
(`current_week`, `last_week`, `two_weeks_ago`, `three_weeks_ago`, `last_month`) and the
same live staleness rule: current-week rows marked `cooling_off` stay visible only while
their source/update timestamp is within the last three days.

### Overlap + observability

The three batch jobs (`pipeline`, `statcommentary`, `vibesynth`) each take a per-job Postgres
advisory lock, so a manual run racing the cron exits 0 cleanly instead of overlapping. Every run is
recorded in `pipeline_runs`:

```sql
SELECT * FROM pipeline_runs_latest;   -- did last night's jobs finish? (no log-grep needed)
```

Cron exit codes: `0` success (or clean overlap-skip) · `3` partial (retryable per-entity failures —
the queue retries) · `1` systemic (enumeration/stage failure, or dead-lettered work remains).

---

## 7. The pipeline: ingest → queue → derive → reveal

The once-daily `cmd/pipeline -mode ingest` run feeds the durable queue; Rust drains the stages:

```
sweep ingest -> enqueue/notify -> editor -> investigate_entity -> graph -> transfers -> narratives -> vibe -> peak -> momentum -> sigil
```

1. **Compile (sweep):** RSS-sweep every team in NBA/NFL/FOOTBALL (no fixture/tier filter, so
   offseason coverage doesn't collapse); persist to `news_articles` and enqueue the Editor's
   read in the same transaction.
2. **Enqueue:** vetted transitions (and other stage handoffs) enqueue durable `pipeline_work` rows.
3. **Derive:** `scoracle-cognition` claims → runs → completes/fails each queued stage in declared
   order (claim → run → complete/fail), with retry + stale-lease recovery.
4. **Reveal:** the precomputed products (`news_summaries`, `transfer_rumors`, `vibe_scores`,
   `sigil_synthesis`, `stat_summaries`) are what the per-product serving endpoints read.

The news-scrub ticker (`NEWS_SCRUB_ENABLED`) is GONE (PLAN-one-rail 8.8) along with the `scrub`
stage it fed. `vetted` is the Editor's fact now: it reads the article and confirms or denies the
links (8.5).

**Live table → product names:** `vibe_scores` = Vibe · `sigil_synthesis` = Sigil crown ·
`news_summaries` = narratives · `stat_summaries` (`divined_peak`, `peak_trajectory`) = stat commentary ·
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

Runs on every main-bound change; three jobs (the python and docker jobs died with
the seeder/Docker purge of 2026-08-11):

- **go** — `gofmt`/`vet`/build + `go test -race` (incl. DB-gated queue tests) + `validate-stmts`
  against a `postgres:18` provisioned **from `sql/schema/schema.sql`** (the F-025/F-039 "prepared
  statements register against a migrated test DB" check; `CREATE ROLE web_user` before loading).
- **shell** — `bash -n` + ShellCheck (`--severity=warning -x`).
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
tail -f logs/pipeline-ingest.log logs/narrative-links.log logs/statcommentary.log logs/vibesynth.log logs/backup.log
```

Common incidents:

- **API won't boot after a deploy** → almost always schema drift (the boot guard). Apply the pending
  migration (`sql/migrate.sh`), or roll the binary back (§3). Check `journalctl` for the failing
  prepared statement.
- **Derived products stale** → check `cmd/work status` / `dead-letters`; check `scoracle-cognition`
  service health and Ollama reachability; a cold model load can time out, then retry/re-drain.
- **GPU thrash** → `OLLAMA_MAX_CONCURRENT=1` serializes model calls; the systemd drop-in
  `OLLAMA_NUM_PARALLEL=1` + `OLLAMA_MAX_LOADED_MODELS=1` is **not yet set** (F-035, needs sudo).

---

## 11. Machine rebuild (bare-metal recovery)

Full first-time setup + rationale: `../scoracle-wiki/progress_docs/scoracle-backend/SELF_HOSTING_OPS.md`. Mechanics:
`scripts/hosting/README.md`. Short path:

1. Install Postgres 18, Ollama + the busy-work model (§1.1), Go toolchain, Rust toolchain.
2. Clone the repo; create `.env.local` — **the only env file** (see §11.1 for the key list).
3. Restore the latest **off-disk/off-site** dump (§4) and `sql/migrate.sh` it to the latest schema.
4. `scripts/hosting/install.sh` (renders systemd units), `loginctl enable-linger sheneveld`,
   `systemctl --user enable --now scoracle-api.path scoracle-api.service`.
5. `crontab scripts/hosting/crontab.example`; `sudo cp scripts/hosting/logrotate.conf
   /etc/logrotate.d/scoracle`.
6. `scripts/hosting/release.sh` to build/stamp/verify; `scripts/hosting/restore-drill.sh` to prove
   the backup is bootable.
7. Cloudflare Tunnel (`cloudflared`, `cloudflared-config.example.yml`) for `api.scoracle.com`.

### 11.1 The environment: ONE file (consolidated 2026-08-01)

**`.env.local` is the only env file.** There is no `.env` any more — it was deleted from the repo
and both machines. Do not recreate one, and **never make a `.bak` copy inside the repo**: a backup
of `.env.local` holds the same live secrets, and that is precisely how credentials reached git
history twice (F-046, then `.env.local.bak.20260726-111052` on 2026-07-26). Backups belong in
`~/env-backups` (0700/0600), outside the repo. `.gitignore` covers `.env.local*`, `.env*.bak*`
and `.env`.

Both systemd units load exactly one file:
`EnvironmentFile=-/home/sheneveld/scoracle/scoracle-backend/.env.local`. The old two-file
`.env` → `.env.local` overlay (later file silently overwrites earlier) is gone — that overlay was
the source of the confusion, since the effective value of a key depended on load order.

The consolidation merged both files, dropped **17 dead keys** and rotated three secrets. Retired
and deliberately absent — the rail derives everything from Google News queries now, so no
third-party provider credential is needed at all: `API_SPORTS_KEY`, `BALLDONTLIE_API_KEY`,
`SPORTMONKS_API_TOKEN`, all nine `TWITTER_*`, plus legacy toggles `NEWS_SCRUB_VIA_QUEUE`,
`CACHE_BACKEND`, `CACHE_WARMUP_ENABLED`, `OLLAMA_KEEP_ALIVE`, `OLLAMA_SHORT_TIMEOUT_SECONDS`.

**The 42 live keys**, by group:

| group | keys |
|---|---|
| Environment | `ENVIRONMENT` |
| Database | `DATABASE_URL`, `DATABASE_PRIVATE_URL`, `DB_POOL_MIN_CONNS`, `DB_POOL_MAX_CONNS`, `DB_POOL_MAX_LIFE_MINUTES` |
| API server | `API_HOST`, `API_PORT`, `CORS_ALLOW_ORIGINS`, `CORS_PRODUCTION_ORIGINS` |
| Auth | `JWT_SECRET`, `JWT_ACCESS_TTL_MINUTES`, `JWT_REFRESH_TTL_DAYS`, `FIREBASE_CREDENTIALS_FILE` |
| Rate limiting | `RATE_LIMIT_ENABLED`, `RATE_LIMIT_REQUESTS`, `RATE_LIMIT_WINDOW`, `RATE_LIMIT_INTERNAL_KEY` |
| Cache | `CACHE_ENABLED` |
| Ollama | `OLLAMA_BASE_URL`, `OLLAMA_MODEL`, `OLLAMA_TIMEOUT_SECONDS`, `OLLAMA_MAX_CONCURRENT` |
| Cognition | `COGNITION_STAGES`, `COGNITION_BACKEND_CONCURRENCY`, `COGNITION_HANDLER_TIMEOUT_SECONDS`, `COGNITION_ARTICLE_READ_TOP_K`, `DERIVE_WORKER_ENABLED`, and the per-role routes `COGNITION_ROUTE_<ROLE>[_BASE_URL]` (12 keys today) |

`COGNITION_ROUTE_*` keys are built at runtime by `format!("COGNITION_ROUTE_{}", role.env_suffix())`
(`rust/src/config.rs:265`), so a plain grep for them finds nothing — **do not "clean them up" as
unused.** `DERIVE_WORKER_ENABLED` is likewise referenced outside `rust/src`.

---

## 12. Launch-gate carryovers (tracked, not yet done)

These surfaced during the audit and are pre-launch work (see `../scoracle-wiki/progress_docs/scoracle-backend/FIRST-GPT-AUDIT-FINDINGS.md`):

- **F-030** — NFL (1072) + FOOTBALL (2147) current-season entities have **zero** season-2025-stamped
  Sigils. Run larger reconciliation passes (`vibesynth -mode nightly` with a higher `-limit`) before launch.
- **F-040** — pick the off-SITE backup target (cloud/NAS); mechanism is ready via
  `OFFHOST_BACKUP_DIR`.
- **F-035** — set the Ollama systemd drop-in `OLLAMA_NUM_PARALLEL=1` + `OLLAMA_MAX_LOADED_MODELS=1`
  (needs sudo).
- **F-046 🟠 (security) — reopened and largely closed 2026-08-01.** The 2026-06-24 plan was never
  executed: rotation never happened and the history purge never ran, so **the leaked archbox
  `scoracle` Postgres password was still the live one** as of 2026-08-01. It then got worse — a
  new `.env.local.bak.20260726-111052` was **committed and pushed** on 2026-07-26 carrying 40
  populated values (DB URLs, `JWT_SECRET`, `RATE_LIMIT_INTERNAL_KEY`, provider keys, five
  Twitter/X tokens). The bare `.env.local` ignore rule never matched suffixed copies.
  **Done 2026-08-01:** Postgres password, `JWT_SECRET` and `RATE_LIMIT_INTERNAL_KEY` all rotated
  and verified live; env consolidated to a single `.env.local` with every third-party credential
  deleted outright (§11.1); ignore rule widened to `.env.local*` / `.env*.bak*` / `.env`; history
  purged and force-pushed across all 12 branches, both clones re-synced to 0 occurrences. Scott
  completed the credential side the same day: all provider keys revoked at their dashboards,
  subscriptions ended, Neon projects deleted. **Net: every leaked value is now dead.**
  The superseded public `albapepper/Scoracle` repo was deleted from GitHub (and its orphaned local
  checkout removed), ending the last *public* trace. One residual is *recorded but harmless* —
  GitHub's read-only `refs/pull/1|2/head` cannot be rewritten by any force-push and still carry
  4 literals, all four of them now revoked or rotated; a Support GC request is optional tidiness.
  **F-046 is CLOSED.** **Repair runbook: `PASSWORD-LEAK-REPAIR.md`.** Full scope:
  `../scoracle-wiki/progress_docs/scoracle-backend/FIRST-GPT-AUDIT-FINDINGS.md` F-046 +
  `progress_docs/2026-06-24_F-046-credential-leak-remediation.md`.

The remaining pre-launch milestone is the **Final launch gate** in `FIRST-GPT-AUDIT.md` —
stats/news/convergence/operations end-to-end proofs, per sport.
