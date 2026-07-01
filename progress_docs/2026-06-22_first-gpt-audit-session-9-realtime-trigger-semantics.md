# First GPT Audit — Session 9: Repair real-time news trigger semantics

**Worked:** 2026-06-22 (archbox) · **Deployed:** live, commit `cc23b68`

**Plan:** `planning_docs/FIRST-GPT-AUDIT.md`, Session 9 · **Designed with:** Sessions 7 (durable
`pipeline_work`) + 8 (ordered pipeline). **Product authority:** wiki `Product Narrative`.

## Goal

Make the real-time news path (between nightly `cmd/pipeline` runs) obey the audit invariant:
**notifications improve latency but are never required for correctness.** Pre-S9, a trigger fired
`pg_notify('vibe_trigger'/'transfer_trigger', …)` on a raw link INSERT (a "5 articles in 60 min"
volume spike, *before* scrub), and two in-API listeners ran local model directly off that transient
NOTIFY, holding the affected-entity set in process memory (lost on restart; a missed NOTIFY never
recovered; in-process governors that don't work across replicas).

## What changed

The trigger now **enqueues durable work** on the `vetted=TRUE` transition; NOTIFY is only a
wake-up; an in-API worker **drains the durable `pipeline_work` queue** on wake / startup / a
safety-net timeout. One drain implementation backs both the nightly cron and the real-time worker.

- **`sql/migrations/103_enqueue_derive_on_vetted.sql`** — drops BOTH live AFTER INSERT volume
  triggers + their notify functions (live drift, see F-015) and installs `enqueue_derive_on_vetted`,
  an `AFTER UPDATE OF vetted … WHEN (NEW.vetted IS TRUE AND OLD.vetted IS DISTINCT FROM NEW.vetted)`
  trigger. It computes a corpus fingerprint (`count : max(scrubbed_at) epoch` over the entity's
  vetted, <72h links — byte-identical to `corpus.CorpusVersion`), gates on `count=0` (no fresh
  corpus → enqueue nothing; also bounds the bulk auto-vet), enqueues `narratives` + `vibe`
  (+ `transfers` for teams) into `pipeline_work` with the same ON CONFLICT reopen semantics as
  `work.Enqueue`, and fires `pg_notify('pipeline_work_ready', '')` (constant payload → per-txn
  de-dup). A partial covering index `idx_nae_vetted_lookup` keeps the per-row count cheap under the
  20000-row maintenance bulk auto-vet.
- **`go/internal/derive/derive.go`** — `Drainer`: the S8 drain (`drainStage` + the 4 stage runners
  + `nameOf` + `vibeVersion` + queue constants) relocated out of `cmd/pipeline` so nightly +
  real-time share ONE implementation. `DrainAll` runs transfers → narratives → vibe → sigil in
  order; the vibe→sigil enqueue stays inside `drainVibe`.
- **`go/internal/derive/worker.go`** — the in-API drain worker: a dedicated `LISTEN
  pipeline_work_ready` conn, one goroutine looping `RequeueStale(StaleLease) → DrainAll →
  WaitForNotification(timeout)`. Single goroutine ⇒ ≤1 `DrainAll` in flight (bounded GPU); per-entity
  in-flight + cross-replica safety come free from the queue (`FOR UPDATE SKIP LOCKED`, `running`
  lease). pgx v5 leaves the LISTEN conn usable after a context-deadline interrupt, so the per-call
  timeout loop is safe. Started only when Ollama is reachable at boot.
- **`go/internal/ml/news_scrub.go`** — `applyVerdicts` batched into ONE `UPDATE` per article (over
  `unnest`'d arrays) instead of per-link autocommit: atomic, and one trigger transaction per article
  so the constant NOTIFY de-dups to one wake-up.
- **`go/cmd/pipeline/main.go`** — drops the Go enqueue (`scrubAndEnqueue`→`scrubFresh`; the trigger
  is the sole enqueuer, closing S8's residual scrub→enqueue window) and drives `derive.Drainer`.
- **`go/cmd/api/main.go`** — swaps `StartNewsVolume` + `StartTransfer` for `derive.StartWorker`;
  keeps the percentile listener (FCM + `composite_shift`→sigil; Session 12 owns that — F-017).
- **`go/internal/config/config.go`** — `DERIVE_WORKER_ENABLED` (default true),
  `DERIVE_DRAIN_INTERVAL_SECONDS` (default 30).
- **deletes** `go/internal/listener/news_volume_worker.go` + `transfer_worker.go` (~460 lines).

Net: **612 insertions / 778 deletions** across 10 tracked files — a smaller running system.

## Verification

- **Pre-deploy:** `gofmt`/`vet`/`build`/`test` green. Migration dry-run (rolled back). **Trigger
  behavior proven in a rolled-back transaction** — caught + fixed a real `array_append` plpgsql bug
  before deploy (a `text[] || 'literal'` ambiguity). All three S9 bullets confirmed at the trigger
  level: fresh team transition → narratives/vibe/transfers (n=1 each); rejected (`vetted=FALSE`) →
  0 rows; stale (>72h) → 0 rows (count gate); player burst (2 links, one UPDATE) → 1 row/stage, no
  transfers (PK-dedup ⇒ ≤1 in-flight), `input_version` matching `CorpusVersion`'s `count:epoch`.
- **Live (post-deploy, `cc23b68`):** API healthy (`/health/db` 200, serving the built commit), old
  listeners gone, `Derive drain worker connected`. Observed ALL FOUR stages drain to completion
  (`transfers/narratives/vibe/sigil ok=1`); `pipeline_work` emptied to 0; fresh output rows landed
  for team/30 NFL (`news_summaries`, `vibe_scores`, `transfer_rumors`).

## Deploy notes / gotchas

- Applied migration 103 **per-file** + recorded `103_enqueue_derive_on_vetted` in
  `schema_migrations` (NOT `migrate.sh` — the sibling-owned `099_team_rosters.sql` stays unapplied),
  then `scripts/hosting/release.sh` (builds all 4 binaries @ `cc23b68`, reinstalls units, restarts
  the API, verifies health). Migration before restart, per F-001.
- **F-015 (drift):** the live DB had `088` recorded but the table was still `vibe_scores` and TWO
  AFTER INSERT triggers both fired `vibe_trigger`; 093–095 unreflected. 103 drops both triggers and
  fixes the double-fire; the `vibe_scores`→`sentiment_scores` reconciliation is Session 15/17.
- **F-016 (deploy flap):** `scoracle-api.path` is now ACTIVE (no longer the "inert" of the memory) —
  `install.sh` re-renders it correct — so it raced `release.sh`'s explicit restart into ~4 rapid
  restarts before settling (NRestarts stayed 0). That cancelled an in-flight `DrainAll`, orphaning 2
  `running` rows; `RequeueStale` (30m) would have recovered them (graceful degradation, as designed)
  — manually requeued here to verify the drain in-session.

## Handoff

- **S12** owns the stats-rail real-time Sigil: route `composite_shift`→sigil through `pipeline_work`
  instead of the inline `SigilGenerator.Generate` the percentile listener still calls (F-017); plus
  the Sigil convergence lifecycle (F-010/F-011).
- **S13** advisory lock is now more relevant: two drainers (nightly cron + in-API worker) share the
  queue (safe via SKIP LOCKED + shared 30m lease, but F-012 updated).
- **S15** migration reconciliation must square the ledger with the live schema (F-015).
- Decide the path-watcher question (F-016) and update the `backend-api-restart-mechanics` memory.
