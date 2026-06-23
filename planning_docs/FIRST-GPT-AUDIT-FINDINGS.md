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
- **Found:** Session 7 · **Status:** Resolved (Session 8, `937df44`)
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

### F-009 — Transfers are now FRESH-NEWS-SCOPED, not all-teams-nightly
- **Found:** Session 8 · **Status:** Ops note
- Pre-S8 the nightly pipeline ran transfer analysis across **every** team each run. S8 made it
  event-driven: only teams that gained a **fresh vetted link** this run are enqueued for
  `transfers`. A team with no new corpus gets **no new transfer generation** — this is intended
  (transfer heat decay is read-time: the `/transfers` read filters to heat>0 within 14d, so a
  stale rumor ages out of the served set without needing re-generation). Net effect: far fewer
  Gemma calls, and "no fresh news ⇒ no work" holds for transfers too.
- **Action:** if a *coverage* refresh of all teams is ever wanted (e.g. before launch), add it as a
  bounded **reconciliation** job (mirror the Session 12 reconciliation pattern), not by reverting to
  unconditional all-teams generation.

### F-010 — Sigil terminal stage is wired minimally in S8; convergence lifecycle is still S12
- **Found:** Session 8 · **Status:** Watch (Session 12)
- S8 wires Sigil as the terminal queue stage: a completed `vibe` item enqueues a `sigil` item
  (before the vibe row is completed, so a crash re-runs vibe rather than dropping the sigil), and the
  drain calls the **existing** `SigilGenerator.Generate(..., SkipUnchanged: true)` — which already
  reads its 3 pillars live and skips the Gemma call on an unchanged input hash. S8 deliberately did
  **not** rebuild the convergence lifecycle. Still owned by **Session 12**: season-scoping the hash /
  previous-score / debounce, moving generation out of follower/FCM early-returns, the real `DryRun`
  field, and converting the nightly run into reconciliation/backfill-only.
- **Action (also S12):** S8 only wires the **news-rail** producer (vibe→sigil). The **stats-rail**
  producer — *Rating change ⇒ enqueue sigil* (and the Momentum input, see F-011) — is NOT wired by
  S8. S12 (or a stats-rail/finalize hook) must add the `rating`/`momentum`→`sigil` enqueue so a stat
  change alone reconverges the Sigil.

### F-011 — Momentum is still read-derived; "append a Momentum generation" is blocked → S12
- **Found:** Session 8 · **Status:** Watch (Session 12)
- The audit S8 work list says "append a Momentum generation when its input version changes," but in
  the live code **Momentum is not a generation** — it is computed at read time (peer-cohort
  precompute + per-event composite slope inside `SigilGenerator.loadMomentumPillar`), and
  `rating_history` is still **write-only** (per its own comment, too shallow to be the trajectory
  source yet). So there is no momentum row to enqueue or version. S8 leaves momentum read-derived and
  Sigil reads it live.
- **Action:** when `rating_history` has multi-point depth (or a dedicated momentum generation lands),
  Session 12 can make Momentum a versioned input feeding the Sigil convergence hash. Until then the
  Rating+Vibe+Momentum "three versioned inputs" model is partial by necessity.

### F-012 — Pipeline overlap is not yet guarded (advisory lock is Session 13)
- **Found:** Session 8 · **Status:** Watch (Session 13)
- The S8 pipeline has **no advisory lock**, so two concurrent `cmd/pipeline` runs could both claim/
  process work. The blast radius is bounded: `Claim` uses `FOR UPDATE SKIP LOCKED` (two runs get
  disjoint rows) and `Complete`/`Fail` are status-guarded. The real risk is `RequeueStale` at
  startup stealing a slow-but-alive prior run's in-flight rows — mitigated by a **30-minute** stale
  lease (longer than any single item's budget, incl. `perTeamTimeout`=10m for transfers), but not
  eliminated. Separately, the in-API maintenance scrub ticker (30m) and the nightly pipeline can both
  scrub the same article (idempotent, just wasteful).
- **Action:** Session 13 adds the per-job PostgreSQL advisory lock (and the `pipeline_runs` record);
  a shared Gemma concurrency governor is Session 14. Until then, rely on the single nightly cron slot
  + the 30m lease.
- **Update (Session 9):** there are now **two** drainers of `pipeline_work` — the nightly `cmd/pipeline`
  cron AND the always-on in-API `derive.StartWorker`. They overlap by design every night. Cross-claim is
  still safe (`FOR UPDATE SKIP LOCKED` → disjoint rows; both share `derive.StaleLease`=30m so neither
  steals the other's live lease), but this makes S13's advisory lock more relevant, not less. The S9
  deploy also demonstrated lease-recovery for real: a release-time restart flap (F-016) orphaned two
  `running` rows that `RequeueStale` would have recovered.

### F-013 — `GetEntityNews` signature changed; only the sweep calls it, but the API recompiles it
- **Found:** Session 8 · **Status:** Ops note
- `thirdparty.NewsService.GetEntityNews` now returns a 3rd value (affected article IDs). Its **only**
  caller is `corpus.Sweep` (the live `/news` RSS routes are retired), so behavior is unchanged
  elsewhere — but the symbol is still compiled into `scoracle-api`. The running API keeps its old
  binary until rebuilt+restarted; an API restart is **not required for S8 correctness** (S8's queue is
  driven by `cmd/pipeline`, a cron binary; the API doesn't touch `pipeline_work` until Session 9). The
  pipeline "deploy" is simply rebuilding `go/bin/pipeline` (cron execs it fresh each night — no
  systemctl restart). Just ensure the **next** `scoracle-api` rebuild includes these shared-package
  changes (corpus/news/maintenance); `go build ./...` is clean.

### F-014 — Ollama cold-start can blow the 180s timeout (capacity → Session 14)
- **Found:** Session 8 verification · **Status:** Watch (Session 14)
- During the S8 smoke, the first two scrub Gemma calls after idle each hit the
  `OLLAMA_TIMEOUT_SECONDS=180` client timeout and failed (`Client.Timeout exceeded while awaiting
  headers`). Once `gemma4:e4b` was warm (resident in VRAM) the SAME generation dropped to **~7.5s**.
  Root cause is capacity, not the pipeline: `gemma4:e4b` is an **8B** model **partially
  CPU-offloaded** — only ~3.6GB of its ~10GB sits in the **8GB** GPU (which already shows ~6.5GB
  used) — so the cold load + first inference is slow enough to exceed 180s under any contention.
- **Impact on S8:** the pipeline behaves **correctly** — a timed-out scrub is fail-closed (nothing
  vetted ⇒ nothing enqueued ⇒ no derivation), so cold runs simply *under-derive* (the maintenance
  backlog re-scrubs later) rather than publish bad data. But a cold nightly run could leave much of
  the batch un-derived.
- **Action (Session 14):** confirm the production Ollama timeout from measurement (operation-specific
  — narratives may need longer than scrub); separate worker readiness from the one-time API boot
  ping / add a model warm-up; add a shared GPU concurrency governor. Consider that the 8B model does
  not fully fit the 8GB GPU — quantization/offload tuning or a smaller model may be the real fix.

### F-015 — Live schema drift: the migration ledger != the live schema (088 half-applied; 093–095 unreflected)
- **Found:** Session 9 · **Status:** Partially resolved (`cc23b68` / migration 103); remainder → Session 15/17
- Verified DIRECTLY against the prod DB (not the migration files): `088_rename_vibe_to_sentiment` is
  RECORDED in `schema_migrations`, but its table rename did **not** stick — the table is still
  `vibe_scores` (no `sentiment_scores` exists), which is why all live Go still queries `vibe_scores`.
  Yet 088's trigger/function half DID land, so `news_article_entities` carried **two** AFTER INSERT
  triggers (`trg_vibe_trigger_on_news_link`→`notify_vibe_trigger` AND `sentiment_trigger`→
  `notify_sentiment_trigger`), BOTH firing `pg_notify('vibe_trigger', …)` — every 4→5 crossing
  double-fired (the 30m debounce masked it). `093/094/095` (sigil convergence rename) are likewise not
  reflected (live functions/tables keep pre-rename names). **A version being in `schema_migrations` does
  NOT mean its effects are live — read the live schema, never the files.**
- **Resolved by S9:** migration 103 drops BOTH triggers + both notify functions and installs the single
  `enqueue_derive_on_vetted` trigger, killing the double-fire.
- **Action:** Session 15 (migration reconciliation) must square the ledger with reality — either finish
  the `vibe_scores`→`sentiment_scores` rename (touches `db.go` prepared statements + many call sites) or
  revert 088's recorded-but-partial state, and apply/retract 093–095. Session 17 docs must use the ACTUAL
  live names. Launch-gate item ("deployed schema, migrations, and docs describe the same system").

### F-016 — `scoracle-api.path` rebuild-watcher flaps the API across a release (now costs Gemma work)
- **Found:** Session 9 deploy · **Status:** Resolved (S9 — `release.sh` masks the watcher during placement)
- NOT a regression: `scoracle-api.path` (watches `go/bin/`, restarts the API on rebuild) was DELIBERATELY
  fixed to the correct consolidated path in **Session 2** and has been active since. The
  `backend-api-restart-mechanics` memory calling it "inert" was simply **stale** (now corrected). The
  unit even documents the multi-restart as expected/harmless. What's NEW is the **cost**: `release.sh`
  places 4 binaries → 4 directory-change events → ~4 restarts (plus its own explicit restart) before
  settling; pre-S9 that was harmless (listeners just reconnected), but S9's in-API derive worker means
  each spurious restart now **cancels an in-flight Gemma drain** (orphaned `running` rows, recovered by
  `RequeueStale`; wasted GPU on the contended 8GB card, F-014).
- **Resolved:** `release.sh` now `systemctl --user stop scoracle-api.path` before placing the binaries and
  re-arms it on exit (cleanup trap), so only its single authoritative restart fires. The watcher is kept
  for ad-hoc `go build` outside `release.sh` (its actual value); the `backend-api-restart-mechanics`
  memory is updated to reflect it is active.
- **Residual (not fixed):** the watch is on the whole `go/bin/` directory (intentional — `go build`
  renames temp→final so a file watch would miss it), so a standalone `go build -o bin/pipeline` outside
  `release.sh` still restarts the API. Low-frequency; narrowing it reliably is hard. Left as-is.

### F-017 — composite_shift → Sigil still runs Gemma directly off the percentile NOTIFY (not durable)
- **Found:** Session 9 · **Status:** Watch (Session 12)
- S9 routed the NEWS-rail real-time triggers (narratives/vibe/transfers) through the durable
  `pipeline_work` queue, but deliberately left the STATS-rail real-time path untouched: the percentile
  listener (`internal/listener/listener.go`) still calls `SigilGenerator.Generate` DIRECTLY on a
  `composite_shift` (≥10 percentile delta), in-process, off the transient `percentile_changed` NOTIFY,
  with a 24h time-debounce — the same "transient NOTIFY drives Gemma, lost on restart" pattern S9 removed
  from the news rail. It's Sigil convergence, which Session 12 owns.
- **Action:** Session 12 should enqueue a durable `sigil` `pipeline_work` item on a Rating/composite
  change (stats-rail producer; pairs with F-010) instead of generating inline, so the in-API derive
  worker drains it like every other stage.

### F-018 — An API restart mid-drain strands the derive worker's leased batch for up to staleLease (30m)
- **Found:** Session 9 deploy · **Status:** Watch (Session 13/14)
- The in-API `derive` worker's `DrainAll` claims a batch (up to `claimBatch`=10) as `running`, then
  processes serially. A graceful shutdown (SIGTERM from ANY restart — release, manual, or the
  `scoracle-api.path` trigger) cancels the drain ctx mid-item: the in-flight Gemma call errors with
  "context canceled" AND `work.Complete`/`Fail` then also run on the cancelled ctx, so they no-op
  ("mark-failed failed: context canceled") — the leased rows stay `running` (orphaned). The next start
  runs `RequeueStale(StaleLease=30m)`, but the just-orphaned rows are <30m old so they are NOT recovered
  until ~30m later. Net: a restart while the worker is mid-drain delays that batch by up to 30m.
  Correctness is safe (work is never lost); observed live during the S9 deploy (orphans manually
  requeued for promptness). Note this is independent of F-016's flap — it happens on a *single* clean
  restart too.
- **Action:** clean fix is for the worker to settle its claimed items on graceful shutdown using a FRESH
  (non-cancelled) context — run `work.Complete`/`Fail`, or requeue-to-`pending`, on `context.Background()`
  with a short timeout so a shutdown returns rows to claimable immediately instead of stranding them as
  `running`. Pairs with S13 (advisory lock / overlap) and S14 (Gemma lifecycle). Until then: prefer
  releasing when `pipeline_work` is quiet, or manually requeue stale `running` rows after a mid-drain
  restart.
