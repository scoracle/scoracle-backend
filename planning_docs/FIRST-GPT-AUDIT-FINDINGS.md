# First GPT Audit — Findings Ledger

Companion to `FIRST-GPT-AUDIT.md`. A running, **append-only** record of out-of-scope things
surfaced while executing the audit: surprises, cross-session dependencies, deliberate deferrals,
operational gotchas, and "do this in Session N" notes.

This is **not** a session summary — what a session actually *did* belongs in its
`progress_docs/` entry. This ledger is for what a *future* session, the launch gate, or an
operator should know but that has no other durable home.

## How to use

- **At the end of every session,** add an entry for anything you learned that outlives the
  session. One finding per entry.
- Append, don't rewrite. When a later session acts on a finding, update its **Status** line
  (and add the resolving commit) rather than deleting it.
- Keep IDs sequential (`F-NNN`).

**Status vocabulary:** `Open` · `Watch (Session N)` · `Folded into Session N` ·
`Resolved (<commit>)` · `Ops note` (durable operational fact, not a to-do).

**Provenance:** entries marked _(carried)_ were recorded retroactively from earlier sessions /
runbook memory when this ledger was created (2026-06-22); reconfirm against current code before
relying on them.

---

## Entries

### F-001 — Go binaries do not auto-restart on rebuild; never pattern-kill _(carried)_
- **Found:** Session 2 / runbook · **Status:** Ops note
- The repo path-watcher is inert (watches a stale pre-consolidation path), so rebuilding a Go
  binary does **not** restart the running service — restart manually
  (`systemctl --user restart scoracle-api.service`). **Never** kill backend processes by name
  pattern: prod shares the repo `bin/` path and a pattern-kill caused a prod outage once.
- **Action:** every session that rebuilds a Go binary (8, 12, 13, 14) must plan an explicit,
  PID-specific restart. Apply DB migrations *before* the API restart (`db.New` prepares every
  statement at boot and fails fast on a drifted schema).

### F-002 — Keep the nightly `cron-vibesynth.sh` Sigil line until Session 12 _(carried)_
- **Found:** Session 3 · **Status:** Watch (Session 12)
- The S3 crontab rewrite had dropped the nightly Sigil generation line; it was restored. Do not
  drop it before Session 12, which converts that nightly run into reconciliation/backfill-only.

### F-003 — Rating engine has sub-display percentile tie-break non-determinism _(carried)_
- **Found:** deferred-finalize work (pre-S6) · **Status:** Open
- On messy/incomplete seasons, `recompute_season` vs per-fixture finalize can differ by ~74 rows
  in the *percentile layer* due to tie-break ordering — but **0 rows differ on the displayed
  `rating_composite_score`**. On a clean, complete season it is byte-identical and fully
  deterministic. So equivalence checks must be run only on a STABLE, COMPLETE season.
- **Action:** candidate fix — add a deterministic tiebreaker to the rank `ORDER BY`. Verify in
  Session 16's engine-equivalence tests.

### F-004 — `REFRESH MATERIALIZED VIEW CONCURRENTLY` is safe inside the recompute txn
- **Found:** Session 6 · **Status:** Ops note
- `recompute_season()` runs `REFRESH MATERIALIZED VIEW CONCURRENTLY` and is called inside an
  explicit `with conn.transaction()` in the seeder — this works (contrary to the common
  assumption that CONCURRENTLY refresh can't run in a transaction block). The S6 durable-drain
  wraps recompute + snapshot + marker-delete in one transaction for atomicity on this basis.

### F-005 — Python seeder appears to run from source (editable install)
- **Found:** Session 6 · **Status:** Open (verify)
- S6's seeder code change (`cli.py`, `upsert.py`) is treated as live the moment it's committed,
  on the assumption the seeder is an editable install (`pip install -e .`). If a host instead has
  a built (non-editable) install, the new code won't activate until reinstall. The S6 migration
  (101) is applied either way, so there's no schema/code inconsistency risk — only the question
  of *when* the new behavior turns on.
- **Action:** confirm the install mode on archbox (and any seeding host); document it in Session 17.

### F-006 — `migrate.sh` is bulk; a parallel session's unapplied migration would be swept up
- **Found:** Session 6/7 deploy · **Status:** Ops note
- `sql/migrate.sh` applies **every** file in `sql/migrations/` not yet in `schema_migrations`, in
  lexical order. A second session left `099_team_rosters.sql` untracked **and** deliberately
  unapplied; running the bulk runner would have applied it too. 101 and 102 were therefore applied
  **per-file** (replicating the runner's `INSERT … ON CONFLICT` recording), leaving 099 alone.
- **Action:** when a sibling migration is intentionally pending, apply your own per-file — not via
  `migrate.sh`. Migration-number **gaps are fine** here (e.g. 099 unapplied while 100–102 applied):
  these migrations are independent and idempotent. Coordinate 099 with the other session before any
  bulk run.

### F-007 — `pipeline_work` is entity-keyed; scrub stays article-keyed
- **Found:** Session 7 · **Status:** Folded into Session 8
- The durable work queue (`pipeline_work`, migration 102) is keyed by entity and covers the
  per-entity *derive* stages (transfers, narratives, vibe, momentum, sigil). **Scrub is
  article-keyed** and already has a durable queue: `news_article_entities.scrubbed_at IS NULL`
  (+ its partial index). Don't try to model scrub in `pipeline_work`.
- **Action:** Session 8 wires producers (enqueue at link-insert→scrubbed / vetted / transfer /
  rating / vibe / momentum / sigil changes) and the consumer that drains the queue, replacing the
  in-process `runStart` watermark in `cmd/pipeline`.

### F-008 — No DB-backed Go test harness exists yet
- **Found:** Session 7 · **Status:** Folded into Session 16
- There is no `TEST_DATABASE_URL` wiring or test-DB fixture in the Go suite, so `go test ./...`
  runs entirely offline. `go/internal/work/work_test.go` is already written as integration tests
  **gated on `TEST_DATABASE_URL`** (skip when unset) — ready to light up the moment Session 16
  stands up a migrated test database + CI. (They pass today against an ad-hoc ephemeral PG.)
- **Action:** Session 16 — provision the test DB, set `TEST_DATABASE_URL` in CI, and the
  work-queue concurrency tests (already authored) plus the prepared-statement-registration check
  come along for free.
