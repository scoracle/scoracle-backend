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
- **Found:** Session 3 · **Status:** Resolved (Session 12, `cron-vibesynth.sh` + `crontab.example`)
- The S3 crontab rewrite had dropped the nightly Sigil generation line; it was restored. Do not
  drop it before Session 12, which converts that nightly run into reconciliation/backfill-only.
- **Resolved (Session 12):** `-mode nightly` is now bounded RECONCILIATION — it enumerates
  current-season rated entities whose Sigil is missing/stale and ENQUEUES `pipeline_work(sigil)`
  for the derive worker to drain (no inline synthesis, no Ollama). The cron line is KEPT (now
  `-limit 500`, `-throttle-ms` dropped); the script + `crontab.example` comments were rewritten to
  describe the reconciliation/backstop role.

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
- **Found:** Session 8 · **Status:** Resolved (Session 12)
- **Resolved (Session 12):** the STATS-rail producer now exists — the percentile listener enqueues a
  durable `pipeline_work(sigil)` item on a composite shift (F-017), independent of followers. Combined
  with S8's news-rail vibe→sigil producer and the nightly reconciliation backstop (F-002), every input
  change reconverges the Sigil through the one drain path. Season-scoping, the real `DryRun`, and
  follower/FCM decoupling all landed (see the S12 progress doc).
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
- **Found:** Session 8 · **Status:** Partially addressed (Session 12); a dedicated Momentum generation remains future
- **Update (Session 12):** the Sigil convergence input hash already combines all three pillars
  (narrative titles + rating divined_peak/notability + momentum latest_sentiment/composite/vibe_prompt),
  and S12 makes the rating + composite-momentum pillars SEASON-EXACT, so "trigger convergence when any of
  the three changes, debounce by that hash" is satisfied without a separate Momentum row. Momentum is
  still read-derived (no `momentum_generations` table; `rating_history` still too shallow), so a *versioned
  Momentum generation* feeding the hash remains future work. The vibe/news component is NOT season-scoped
  (see F-029), which is the real residual gap in the three-versioned-inputs model.
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
- **Found:** Session 8 · **Status:** Resolved (Session 13, `c35e1ba` — `internal/jobrun`)
- **Resolved (Session 13):** `internal/jobrun.Guard` takes a per-job session advisory lock
  (`pg_try_advisory_lock(hashtext('scoracle.job.'+job))`) on a dedicated connection held for the run's
  life. `cmd/pipeline` ("pipeline"), `cmd/statcommentary` ("statcommentary"), and `cmd/vibesynth`
  backfill+nightly/reconcile (shared "vibesynth") each Guard at start: a second run (or a manual run
  racing the cron) finds the lock held, records a `skipped` `pipeline_runs` row, and exits 0. Verified
  cross-session on a throwaway PG: same job → `f` (excluded), different job → `t` (isolated). NOTE this
  guards JOB-vs-JOB only; the in-API derive worker deliberately does NOT take the lock — it is meant to
  drain alongside the cron, and `FOR UPDATE SKIP LOCKED` already keeps their claimed rows disjoint.
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
- **Found:** Session 8 verification · **Status:** Resolved (Session 14, `cf4f26069df6`)
- **Resolved (Session 14):** worker readiness is decoupled from the boot ping (generators built
  unconditionally; `derive.DrainAll` reachability-pre-gates and DEFERS when Ollama is down — no
  claims, no burned retries — so pending work drains on recovery with no API restart). Operation-
  specific timeouts replace the flat 600s stopgap: `OLLAMA_TIMEOUT_SECONDS` (300, long/narratives +
  HTTP backstop) vs `OLLAMA_SHORT_TIMEOUT_SECONDS` (120, scrub/vibe/sigil/transfer). `keep_alive=30m`
  keeps gemma4:e4b resident so the true cold load (the 180s trigger) is rare — measured warm
  `load_ms ≈ 350` post-deploy. A shared GPU governor (`OLLAMA_MAX_CONCURRENT`, default 1) serializes
  all in-process Gemma. The 8B-on-8GB partial-offload reality is unchanged but is now *tolerated*
  rather than fatal. See the S14 progress doc + F-035 (explicit cross-process governor).
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
- **Found:** Session 9 · **Status:** Resolved (Session 12, `internal/listener/listener.go`)
- **Resolved (Session 12):** `handlePercentileChange` now ENQUEUES a durable `pipeline_work(sigil)` item
  on a ≥10 composite delta (input_version `composite:<season>:<pctile>`) instead of calling
  `SigilGenerator.Generate` inline off the transient NOTIFY. The enqueue runs BEFORE the follower
  early-return, so zero-follower entities still reconverge (simplification A). `RecentlySynthesized` (the
  inline 24h time-debounce) was deleted — the queue + the generator's input-hash gate handle dedup. The
  listener no longer takes a `*ml.SigilGenerator` (the API still builds one for the derive worker).
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
- **Found:** Session 9 deploy · **Status:** Resolved (Session 13, `c35e1ba` — `internal/derive` + `cmd/api`)
- **Resolved (Session 13):** the drain now settles its leased rows on a context DETACHED from the drain
  context, so a graceful shutdown no longer no-ops the bookkeeping. `drainStage` runs Complete/Fail (and
  the vibe→sigil enqueue) on a fresh `context.Background()` with a short timeout; on shutdown it hands the
  leased-but-unprocessed batch back to `pending` via the new `work.Requeue` (single row, status-guarded,
  no attempt burned) so the rows are immediately reclaimable instead of stranded `running` for 30m. A
  successful generation still Completes even mid-shutdown; a shutdown-cancelled run requeues instead of
  burning a retry. `cmd/api` waits (bounded 8s) on the worker goroutine's done channel before the pool
  closes, so the settle actually lands. Locked by `work_test.go` (`TestRequeue*`). Note the deploy that
  shipped this fix still ran under the OLD (pre-fix) binary's shutdown, so it stranded 1 `running` row
  (transfers NBA team 14), requeued by timestamp post-deploy — the LAST time that toil is needed.
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

### F-019 — NewsNarrator treats an empty `{"narratives": []}` as a parse FAILURE (now dead-letters)
- **Found:** Session 9 deploy · **Status:** RESOLVED (Session 11)
- Surfaced live by S9's durable queue: two thin-corpus NFL players (`player/1447`, `player/39`) failed
  the `narratives` stage with `parse narratives failed (raw="{\"narratives\": []}")`. Gemma legitimately
  returned an EMPTY narratives array (nothing worth narrating for a thin/short corpus), but the narrator's
  parser treats empty as a hard error. PRE-EXISTING narrator behavior (S9 didn't touch `NewsNarrator`) —
  before S9 the in-API news-volume worker just logged-and-dropped it; now the durable queue retries it
  `maxAttempts`=5× and then **dead-letters** it. That's the queue working as intended ("what work is
  failing?"), but the underlying semantics are wrong: an empty result should be a SUCCESSFUL no-data
  outcome (a narratives marker), not a failure.
- **Action:** make `NewsNarrator.Generate` return a successful empty/`SkippedNoCorpus`-style result (or a
  marker row) on `{"narratives": []}` instead of erroring, so thin-corpus entities Complete cleanly rather
  than dead-lettering. Fits Session 11 (append-only marker semantics) / narrator robustness.
- **Update (Session 10):** still **Open** — S10 is transfers-only and did NOT touch `NewsNarrator`. The
  TRANSFER fail-closed path applies the same failure-as-retryable philosophy correctly (a model failure →
  `is_rumor=NULL`, retried, never served), but the NARRATIVES generator still hard-errors on an empty array.
  Post-S10 deploy these 3 `parse narratives failed (raw="{\"narratives\": []}")` rows (`player/33934357`,
  `player/1447`, `player/39`, all NFL) are the **only** dead-letters in `pipeline_work`. Cleanly isolates the
  remaining work for **Session 11**.
- **Resolved (Session 11):** `parseNarratives` now reports whether the response was *parseable as a
  narratives document*, not whether it carried narratives. A cleanly-closed array — including the empty
  `{"narratives": []}` — returns `ok=true` with zero narratives, so `Generate` falls through to the existing
  no-corpus marker path (`groundNarratives` empty → NULL-narrative `news_summaries` row) and the queue item
  **Completes** instead of failing. A genuinely malformed/truncated response (no `"narratives"` key, no `[`,
  or EOF before any object closed AND nothing salvaged) still returns `ok=false` → error → retry, so
  `generation_failed` never masquerades as no-data. Locked by `news_narratives_test.go`
  (`TestParseNarrativesEmptyArrayIsNoData`). By deploy time the dead-letter set had rotated/grown to **11
  failed `narratives` rows, all `{"narratives": []}`** (the bug kept producing them under the old binary);
  all were requeued post-deploy and Completed as markers (see progress doc).

### F-020 — Transfer fail-closed does NOT rewrite historical fail-OPEN rows (append-only); a handful were still served
- **Found:** Session 10 · **Status:** Resolved-by-re-vet (`1486b7b` + migration 104); launch-gate check
- S10 stops NEW fail-open rows, but ~1383 PRE-EXISTING `is_rumor=TRUE, model_version IS NULL` rows remained:
  **1308** from the now-dropped Phase-1 `seed_transfer_rumors` heat-only seeder (`prompt_version='heat-v1'`)
  + **75** from the old Go provisional fallback (`t1`/`t2`, no model). The migration deliberately does NOT
  mutate them — the product invariant is append-only ("a marker must never overwrite/invalidate historical
  derivations"). Of the 1383, only **6** were actually SERVED (latest-per-pair, heat>0, within the 14-day
  read window); the rest had already aged out. Rather than an in-place UPDATE, S10 **enqueued the 3 teams**
  behind those 6 pairs (NBA team 1, NFL teams 1 & 3) into `pipeline_work(transfers)` so the now-fail-closed
  derive worker re-vets them and APPENDS a real verdict that supersedes the fail-open TRUE — on-philosophy.
- **Action:** launch gate — assert no SERVED rumor lacks a Gemma `model_version`
  (`is_rumor IS TRUE AND model_version IS NULL` among latest-per-pair, heat>0, <14d should be 0). Any future
  stragglers self-heal: they age out of the 14-day window or are superseded on the pair's next re-vet.

### F-021 — Transfer retry is TEAM-grained: one unknown pair re-runs the whole team's Gemma vet
- **Found:** Session 10 · **Status:** Watch (optimization; not launch-blocking)
- `pipeline_work` keys transfers by **team**, so S10's fail-closed retry (drainTransfers returns an error when
  `res.Unknown>0` ⇒ `work.Fail` ⇒ backoff re-enqueue) re-runs `GenerateForTeam` for the ENTIRE team — every
  candidate pair is re-vetted by Gemma, even the ones that already resolved TRUE/FALSE. Correct and bounded
  (`maxAttempts`=5 × `failBackoff`=30m, then dead-letter; append-only so duplicate TRUE/FALSE rows are
  harmless — latest-per-pair wins), but wasteful on the contended 8GB GPU (F-014) when a single pair keeps
  failing. Chosen deliberately over inventing a finer-grained per-pair queue (the audit says reuse the
  existing transfers stage, don't add a mechanism).
- **Action:** optional optimization — have `GenerateForTeam` skip pairs that already have a FRESH successful
  verdict (input-hash / recency debounce, simplification B) so a retry only re-vets the still-unknown pairs.
  Pairs with Session 14 (Gemma capacity) and simplification B (input-hash over time-debounce).

### F-022 — Column-DROP migrations: release the NEW binary FIRST, then migrate (reverse of the usual order)
- **Found:** Session 10 · **Status:** Ops note
- The standard rule (F-001) is "apply the migration BEFORE the API restart" because `db.New` prepares
  statements at boot. That assumes the migration only ADDS capability the new binary needs. When a migration
  DROPS a column/function-param that the OLD binary still writes/reads (here: `input_tweet_ids` and
  `compute_transfer_heat`'s 4th OUT param), the order **inverts**: the OLD binary breaks on the new schema,
  but the NEW binary is written to tolerate BOTH (it omits the dropped column from INSERTs — default fills it
  — and SELECTs a named OUT-param subset that works with or without the dropped param). S10 shipped no new/
  changed prepared statements, so `db.New` boots cleanly against either schema. Sequence used: commit →
  `release.sh` (new binary live, API restarted) → apply migration 104 → verify. Zero broken window, no API
  stop needed.
- **Action:** for any future column/param **drop**, make the new binary backward-compatible with the
  pre-drop schema, deploy it first, then drop. Reserve "migrate-then-restart" for ADDITIVE migrations.

### F-023 — Sigil generation-side pillar/debounce loaders do NOT apply the canonical latest-generation rule
- **Found:** Session 11 · **Status:** Resolved (Session 12, `ml/sigil.go` + `ml/rating.go`)
- **Resolved (Session 12):** `loadRatingPillar`, `lastSynthesisHash`, `lastScore` (sigil.go) and
  `lastCommentaryHash`, `ReStampPeakKeys` (rating.go) all dropped the `body/score IS NOT NULL` pre-filter
  and now take the entity('s season')'s LATEST generation regardless of nullability: a marker suppresses
  the rating pillar (returns nil), and a marker's NULL input_hash → "" so the debounce never wrongly skips
  against a superseded real generation. These also became SEASON-scoped (sigil.go's took a new `season`
  arg; rating.go's were already season-scoped).
- Session 11 fixed the *serving reads* to honor markers (latest generation regardless of nullability →
  marker clears current). The *generation-side* loaders that feed Sigil convergence and the regen-debounce
  were left as-is because they belong to the Sigil/convergence lifecycle (Session 12), but they are
  inconsistent with the new rule: `ml/sigil.go loadRatingPillar` and `lastSynthesisHash`/`lastScore`, and
  `ml/rating.go lastCommentaryHash`/`ReStampPeakKeys` all select the latest **non-marker** row
  (`... WHERE body/score IS NOT NULL ORDER BY generated_at DESC LIMIT 1`) rather than "latest generation,
  skip if marker." Consequence: if an entity's latest commentary is a no-stats marker but an older real
  commentary exists, `loadRatingPillar` still feeds the OLD body into a new Sigil, and the rating-rail
  debounce hash compares against the OLD generation — so a marker doesn't fully propagate into convergence.
  Low impact today (`stat_summaries` has **0** markers live; the rating commentary generator rarely hits its
  no-stats path), which is why it was deferred rather than fixed here. (`loadNarrativePillar` already uses the
  correct unfiltered-max pattern, so the narrative pillar is fine.)
- **Action:** Session 12 — when reworking convergence inputs, apply the same canonical rule to the Sigil
  pillar loaders and the rating/sigil debounce queries so a marker suppresses the corresponding pillar and
  the debounce keys off the true latest generation. Pairs with the season-scoping work in S12.

### F-024 — Explicit marker-reason column deliberately deferred (markers stay NULL-body rows)
- **Found:** Session 11 · **Status:** Deferred (optional; revisit at S12/simplification D)
- The audit suggested an optional `marker_reason` (`no_corpus` / `no_stats` / `no_pillars`) so a marker's
  cause is legible, with the hard rule that `generation_failed` must NOT masquerade as no-data. Session 11
  did NOT add the column: the failure/no-data distinction is already encoded in **control flow** — a real
  failure returns an `error` (the queue retries / dead-letters), while a no-data outcome writes a NULL-body
  marker row and Completes — so correctness does not need a column, and the per-product reason is implicit
  in which table the marker lives in (`news_summaries` = no-corpus, `stat_summaries` = no-stats,
  `sigil_synthesis` = no-pillars). Avoiding a schema change kept S11 read-path-only (just `release.sh`, no
  migration), which was preferable with the parallel Sonnet session sharing the tree and the F-015 schema-drift
  risk.
- **Action:** if observability later wants the reason surfaced (e.g. an operator dashboard distinguishing
  "thin corpus" from "model error"), add `marker_reason` as an ADDITIVE column under simplification D
  (standardize generation tables) — set it on every marker-writing path; never write `generation_failed` as a
  marker (keep that an error/retry). Additive → migrate-before-restart (F-001), not the F-022 reverse order.

### F-025 — Validate prepared statements with a throwaway `db.New` boot BEFORE restarting prod
- **Found:** Session 11 · **Status:** Technique (use for any API-touching session — S12 next)
- F-001/F-015: `db.New` prepares EVERY statement at boot (validating columns + functions against the live
  schema), so a SQL/column error in an edited read makes the restart boot **degraded** instead of failing
  the edit at compile time — `go build` and `go vet` do NOT catch a bad column reference inside a prepared-
  statement string. S11 edited 6 reads in `db.go`; to catch a degraded-boot risk *before* touching prod, a
  throwaway `cmd/validate-stmts` called `db.New(ctx, cfg)` against the live DB — this runs the EXACT boot
  path (`AfterConnect` → `registerPreparedStatements` → `Ping`) and returns an error on the first bad
  statement, but starts **no** worker / listener / drainer (unlike running the full API binary, which would
  spin up a second derive worker that races the live one for `pipeline_work` — F-018). Printed `OK`; removed
  after. Faster + safer than booting a spare-port API, and authoritative (same code as boot).
- **Action:** for any session that adds/edits a prepared statement, run this throwaway `db.New` check
  against the live (or a prod-cloned) schema before `release.sh`. Worth promoting to a kept
  `cmd/validate-stmts` (or a `go test` that prepares every statement against a migrated test DB) under
  Session 16 (CI) so the check is permanent rather than re-created per session.
  - **Used again (Session 12):** the same throwaway validated `entity_vibes` + `sigil_leaderboard` (both
    gained a `$4 season` param + a `sports.current_season` subselect) → `OK`; removed after.

### F-026 — Sigil season semantics: HISTORICAL-supported (Scott's call); `/sigil` + crown board take `?season`
- **Found:** Session 12 · **Status:** Resolved (Session 12) — product-contract decision
- The audit asked to decide current-season-only vs. historical. Scott chose **historical supported**:
  `/sigil/{...}` and `/leaderboard/sigil` accept an optional `?season=N`; with no param they serve the
  **live view** = the current season PLUS legacy NULL-season rows (the pre-S12 event-driven default), so an
  older season's crown can never become current (the bug: NBA player/4's most-recently-*generated* row was a
  `season=2024` row scoring 35, which the old "latest generated_at" read served as the live crown). An
  explicit `?season=N` returns that season exactly (no NULL, no 72h freshness window). Every generation now
  STAMPS a concrete season (`SigilGenerator.resolveSeason`: nil ⇒ `sports.current_season`), so real-time/
  manual convergence targets the current season and only an explicit-season backfill writes historical rows.
  Both reads now also emit a `season` field. Debounce hash / previous-score / latest-gen are all season-scoped.
- **Action:** the iOS/web clients can add a season selector to the Sigil card/board; until then the default
  (no param) live view is unchanged in shape (additive `season` field).

### F-027 — `/sigil`'s 72h freshness window is a residual timing-assumption (kept for now)
- **Found:** Session 12 · **Status:** Open (follow-up; deferred deliberately)
- `entity_vibes` (per-entity `/sigil`) still gates the current crown on `generated_at > NOW() - 72h` (it
  predates marker semantics). The audit's guiding principle is "explicit, durable state over timing
  assumptions," and markers now clear stale crowns explicitly — so the time window is redundant in principle.
  S12 KEPT it (only on the live view; an explicit `?season` ignores it) to avoid a serving-behavior change
  during a live deploy AND because it's the current safety net for stale legacy NULL-season rows. Side effect:
  a steady current-season entity whose inputs don't change for 3+ days drops off the per-entity card while
  still ranking on the crown board (which has no freshness window) — a pre-existing inconsistency, not a
  regression.
- **Action:** once reconciliation has stamped every current-season entity (so NULL-season rows are gone,
  F-028) replace the 72h window with pure marker-based clearing, making per-entity `/sigil` and the crown
  board agree on "current" without a time gate.

### F-028 — Legacy NULL-season Sigil rows are served as "current" via a transition allowance
- **Found:** Session 12 · **Status:** Ops note / launch-gate
- 715 pre-S12 `sigil_synthesis` rows have `season IS NULL` (the event-driven paths never stamped a season).
  The live view deliberately includes `season IS NULL` so deploy does NOT drop coverage — those crowns keep
  serving. Reconciliation enqueues every current-season entity lacking a season-STAMPED row (NULL-only
  entities count as missing), so over time each NULL crown is superseded by a season-stamped one (newer
  `generated_at` ⇒ wins the latest-gen pick). Do NOT remove the `season IS NULL` allowance from the live
  reads until coverage is fully season-stamped, or those entities lose their crown.

### F-029 — Historical Sigils reuse CURRENT news/vibe pillars (news/vibe are not season-scoped)
- **Found:** Session 12 · **Status:** Open (limitation of "historical supported")
- The Rating pillar (`stat_summaries.season`) and the composite-Momentum pillar (`event_*.season`) ARE
  season-exact, but the narrative pillar (`news_summaries`) and the sentiment half of Momentum
  (`vibe_scores`) have NO season column — news is "now." So a backfilled historical-season Sigil grounds its
  news/vibe component on the LATEST news, not that season's. Historical differentiation is therefore mostly
  the Rating + composite-Momentum pillars. Acceptable (you can't reconstruct a past season's news sentiment),
  but worth knowing before leaning on historical Sigils as faithful season snapshots.
- **Action:** if faithful historical news is ever needed, season-stamp `news_summaries`/`vibe_scores` (large
  change) — otherwise document `/sigil?season=<past>` as "current narrative over that season's stats."

### F-030 — Current-season Sigil coverage gap (NFL/FOOTBALL have ZERO season-stamped rows) → launch-gate
- **Found:** Session 12 · **Status:** Watch (launch gate) — reconcile before launch
- Under strict season stamping, NBA 2025 needs only ~9 (a prior backfill passed `-season`, so 278 rows are
  2025-stamped), but **NFL (1072) and FOOTBALL (2147) current-season entities have NO `season=2025`-stamped
  Sigil** — their crowns are all legacy NULL-season rows (they still serve via F-028, but aren't season-
  stamped). Reconciliation (`vibesynth -mode nightly`) will enqueue all of them; at GPU throughput (F-014)
  that drains over several nights, plus the always-on derive worker + event-driven convergence. Separately,
  reconciliation can re-enqueue a "timestamp-stale but content-unchanged" entity each night (a new input row
  whose content doesn't move the Sigil hash) — the drain then SkipsUnchanged cheaply (no Gemma, no new row),
  so it's wasteful-but-bounded and non-converging for those few.
- **Action:** before launch, run a larger reconcile/backfill to season-stamp current-season NFL/FOOTBALL
  (e.g. `vibesynth -mode backfill -sport NFL` once, GPU-bound), then assert current-season coverage.

### F-031 — Parallel session claimed migration 105 (`105_vibe_scores_shadow.sql`); next free = 106
- **Found:** Session 12 · **Status:** Ops note
- The plan/memory assumed "next free migration = 105," but the parallel Sonnet (Rust scrubber / vibe-parity)
  session had already created `sql/migrations/105_vibe_scores_shadow.sql` (untracked) in the shared tree.
  S12 needed NO migration (code-only, like S11), so there was no collision — but the next session must use
  **106** and coordinate with the parallel work (which also left tracked edits to `rust/*` + `.gitignore`
  uncommitted in the shared tree, and new untracked `rust/src/{lib,vibe}.rs`, `rust/src/bin/`,
  `go/internal/ml/vibe_parity_test.go`). `git fetch` + inspect before any bulk migrate or commit (F-006).
- **Update (Session 13):** S13 took **106** (`106_pipeline_runs.sql`, applied per-file). At S13 start the
  parallel work was already committed (`2b4f401`, on origin/main) — `105` is tracked and the tree was clean
  except S13's own files; only `099_team_rosters.sql` remained untracked (left alone). **Next free = 107.**

### F-032 — Two pre-fix narratives dead-letters persisted past the S11 fix; surfaced by the new report
- **Found:** Session 13 · **Status:** Resolved (requeued 2026-06-24; `cmd/work dead-letters` → 0). The two
  rows (FOOTBALL `team/6898`, `team/3513`) were requeued (`failed`→`pending`, attempts reset) and are
  draining behind the existing narratives backlog under the fixed binary; the dead-letter state is cleared,
  so the nightly pipeline no longer exits 1 on them.
- The new `go run ./cmd/work dead-letters` immediately surfaced **2** dead-lettered `narratives` rows —
  FOOTBALL `team/6898` and `team/3513`, both `attempts=5`, `last_error=parse narratives failed
  (raw="{\"narratives\": []}")`, dead-lettered ~19:09 ET 2026-06-23. This is the exact F-019 empty-array
  class, but the CURRENTLY DEPLOYED `parseNarratives` returns `ok=true` for a clean `{"narratives": []}`
  (→ marker, verified by reading the code), so these are **pre-fix stragglers**: they dead-lettered under a
  pre-S11 binary and were NOT in the S11/S12 requeue sweeps (parked far-future ⇒ never retried ⇒ never
  self-cleared). Requeuing them (`status='failed'`→`'pending'`, `attempts=0`) will let the fixed worker
  reprocess → empty array → marker → Complete. The targeted requeue UPDATE was **denied by the deploy-mode
  write guard** (it mutates prod records beyond the deploy itself), so it was left for an explicit operator
  action.
- **Consequence (intended):** until cleared, every nightly `cmd/pipeline` run ends `exit 1` /
  `status=failed` in `pipeline_runs` (the "dead-lettered work remains" rule, F-033) — the new machinery
  correctly nagging about stuck work, not a regression.
- **Action:** requeue the two rows once
  (`UPDATE pipeline_work SET status='pending', attempts=0, available_at=NOW(), last_error=NULL
  WHERE stage='narratives' AND status='failed' AND available_at > NOW() + INTERVAL '50 years';`), confirm
  they Complete as markers, and the pipeline returns to green. Consider a `cmd/work requeue-dead-letters`
  subcommand (Session 16/17) so this is a blessed operator command rather than ad-hoc SQL.

### F-033 — Pipeline exit-1 keys off GLOBAL dead-letter state, not just the current run
- **Found:** Session 13 · **Status:** Ops note (deliberate design)
- `cmd/pipeline` exits `1` whenever `work.DeadLetters` is non-empty at end of run — i.e. it reflects the
  whole queue's dead-letter state, not only failures THIS run produced. This is deliberate (the audit lists
  "work remains failed after retries" as an exit-non-zero condition) and gives cron a daily nag until an
  operator clears the stuck work. Side effect: a pre-existing dead-letter (e.g. F-032) makes an otherwise
  clean run report `failed`. Exit-code map: `0` success/overlap-skip · `3` partial (retryable item failures
  this run) · `1` whole-stage failure OR any dead-letters remain. `statcommentary`/`vibesynth` exit codes
  are run-scoped only (no dead-letter gate) — only the pipeline owns the queue-health signal.
- **Action:** none required. If the conflation ever becomes noisy, split "this run failed" from "queue has
  dead-letters" into distinct exit codes, or scope the dead-letter check to stages the run touched.
- **Update (Session 14):** a new exit condition — `cmd/pipeline` now reports `partial` (`exit 3`) when the
  derive drain DEFERS because Ollama was unreachable (`res.Deferred`), distinct from `WholeStageFailure`.
  Raw ingestion still happened (sweep); the work is pending, not failed. Note `res.Deferred` is checked
  FIRST so an outage reads as retryable-partial, not a hard failure.

### F-034 — Simplification A (move the derive worker out of the API) deliberately deferred
- **Found:** Session 14 · **Status:** Deferred (scope with Scott)
- The audit's simplification A proposes splitting background derivation into its own worker process so API
  restarts don't govern ML availability. Session 14 did NOT do this — F-014's readiness decoupling already
  removes the main motivation: an API restart no longer DISABLES ML until the next restart (the worker
  defers-and-recovers on its own), and F-018 already settles the leased batch on a restart. What remains
  true of the in-API model: an API restart still CANCELS an in-flight Gemma drain (F-018 requeues it, so no
  loss, but the wasted GPU time on the contended card is real), and the API process owns more concerns than
  pure serving. Those are smaller wins than the "ML disabled until restart" bug S14 fixed.
- **Action:** if/when the API restart cadence (releases, `scoracle-api.path` ad-hoc rebuilds) makes the
  cancel-in-flight cost annoying, OR the blast-radius argument wins, lift the derive worker + maintenance
  Gemma tickers into a dedicated `cmd/worker` binary (its own systemd unit, NOT restarted by `release.sh`'s
  API restart). Scope with Scott first — it changes the deploy topology (a new unit, new cron/`install.sh`
  wiring) and the `scoracle-api.path` story.

### F-035 — Explicit cross-process Gemma governor (`OLLAMA_NUM_PARALLEL=1`) is NOT set on the ollama service
- **Found:** Session 14 · **Status:** Ops note (recommended follow-up)
- The S14 `OLLAMA_MAX_CONCURRENT` semaphore is **process-wide** — it bounds the API's own goroutines
  (derive worker + maintenance scrub), but it can NOT coordinate across the separate `cmd/pipeline` cron
  process and the API. Cross-process, the only thing serializing GPU work is Ollama's own server-side
  scheduling. The ollama systemd service currently sets NEITHER `OLLAMA_NUM_PARALLEL` nor
  `OLLAMA_MAX_LOADED_MODELS` (verified: no drop-in). For a 10GB model that barely fits the 8GB GPU, the
  authoritative cross-process governor is `OLLAMA_NUM_PARALLEL=1` + `OLLAMA_MAX_LOADED_MODELS=1` — this
  guarantees Ollama never tries to run two gemma4:e4b requests (or load a second copy) at once, which would
  thrash/OOM the card. Today the box happens to serialize anyway (observed), but it is not pinned.
- **Action:** add an ollama systemd drop-in (root):
  `Environment=OLLAMA_NUM_PARALLEL=1` and `Environment=OLLAMA_MAX_LOADED_MODELS=1`, then
  `systemctl daemon-reload && systemctl restart ollama`. Do it during a quiet `pipeline_work` window (the
  restart drops the resident model; the API derive worker will defer-and-recover via F-014). Low risk,
  makes the cross-process bound explicit rather than incidental.

### F-036 — Gemma per-call metrics are LOG-only (no durable run-record metric)
- **Found:** Session 14 · **Status:** Deferred (optional observability)
- S14 added per-call Gemma timing as a structured slog line (`op`, `wall_ms`, `eval_count`, outcome) —
  `journalctl --user -u scoracle-api | grep 'gemma call'` gives an operator real model latency. It is NOT
  hung off `pipeline_runs` (the audit's literal ask) because the in-API derive worker — which makes most of
  the real-time Gemma calls — does NOT own a `pipeline_runs` row (only the cron jobs do), and a durable
  per-call metric would need a new table or `pipeline_runs` columns (a migration). Logs were the
  zero-migration, parallel-session-safe choice, consistent with S13's "avoid a complex observability stack."
- **Action:** if a durable/aggregated Gemma latency surface is wanted later (a dashboard distinguishing slow
  vs timed-out vs unavailable over time), add a `gemma_calls` metrics table (or timing columns on a
  per-stage run record) under Session 16/17 — additive migration, set on every `Generate`. Pairs with
  moving the worker out of the API (F-034), which would give it a natural run-record owner.

### F-037 — Transfers per-pair Gemma call is bounded by the team budget, not the short timeout
- **Found:** Session 14 · **Status:** Watch (optimization; pairs with F-021)
- The new operation-specific short timeout (`OLLAMA_SHORT_TIMEOUT_SECONDS`, 120s) is applied per-stage by
  the drainer for vibe/sigil and per-article for scrub. Transfers stay TEAM-scoped (`perTeamTimeout`=10m for
  the whole team's many pair calls); an individual pair's Gemma call is bounded only by the HTTP client
  backstop (= `OLLAMA_TIMEOUT_SECONDS`, 300s), not the 120s short budget — because the pair call uses the
  team context directly inside `transfer.go analyzePair`. So one wedged pair could run up to 300s within the
  team's 10m. Acceptable (pairs are short, NumPredict 1200; ~2 such wedges fit the team budget) and avoids
  threading a per-pair timeout through `GenerateForTeam`, but it is the one stage where the short budget
  isn't enforced per-call.
- **Action:** when F-021 (per-pair transfer retry / skip-already-vetted) is implemented, wrap each pair's
  Gemma call in a `GemmaShortTimeout` context at the same time, so a single slow pair fails fast like the
  other short ops.
