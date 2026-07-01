# First GPT Audit — Session 8: Make compile → scrub → derive → reveal an ordered pipeline

**Worked:** 2026-06-22 (archbox)

**Plan:** `planning_docs/FIRST-GPT-AUDIT.md`, Session 8

**Depends on:** Session 7 (the durable `pipeline_work` substrate + `go/internal/work`
primitives — wired here for the first time). **Designed with:** Session 9 (real-time
trigger semantics) and Session 12 (event-driven Sigil convergence lifecycle), both
implemented separately.

**Product authority:** wiki `Product Narrative`

## Goal

Turn `cmd/pipeline` from an in-process-watermark chain into an **ordered, durable,
crash-recoverable** pipeline where the database (`pipeline_work`) is the cross-stage
handoff. After one successful run, every accepted new input has reached its derived
products — and a re-run with no fresh input does no local model work.

The pre-S8 pipeline RSS-swept, then derived against a `runStart` watermark while
scrubbing happened **asynchronously** in a separate maintenance ticker — so fresh
links usually weren't vetted in time to influence the same run, and a crash lost the
in-memory touch-set.

## Scope decision (S8 ↔ S12 boundary)

The audit's S8 target flow ends in `momentum → sigil`, but in the live code **momentum
is not a generation** (it's read-derived: peer-cohort precompute + per-event composite
slope; `rating_history` is still write-only) and **Sigil already reads its three
pillars live with an input-hash `SkipUnchanged` gate**. The full event-driven Sigil
convergence lifecycle is explicitly Session 12.

Confirmed with the owner: **S8 = the ordered durable news-rail backbone + a terminal
`sigil` stage on the existing generator.** Momentum stays read-derived; Sigil's
convergence/season/follower lifecycle stays S12. Recorded as findings F-010/F-011.

## What changed

### Target flow (now)

```
requeue stale → RSS sweep → scrub(fresh batch) → enqueue vetted entities
  → drain transfers → narratives → vibe → sigil          (in declared order)
```

- **RSS persist returns the fresh batch.** `thirdparty.persistArticles` now tracks the
  article IDs that gained a NEW link (`RowsAffected > 0` on the primary/secondary link
  inserts) and `GetEntityNews` surfaces them (3rd return). Its only caller is
  `corpus.Sweep` (the live `/news` routes are retired), which aggregates them into an
  `article_id → sport` map. This replaces the `runStart` window — no more starvation
  when a re-seen URL lands no new link rows.
- **Scrub-in-run.** `cmd/pipeline` scrubs exactly that fresh batch synchronously
  (`NewsScrubber.ScrubArticle`, persisting `vetted` + `scrubbed_at`) **before** any
  derivation, so fresh content is vetted in the same run.
- **Producers.** As soon as an article is scrubbed, its **vetted** entities are enqueued
  into `pipeline_work` — `narratives` + `vibe` for every entity, plus `transfers` for
  teams — keyed by a corpus fingerprint (`CorpusVersion`: vetted-link count + latest
  scrub epoch within `NewsLookback`) as `input_version`. Per-article (not batch-at-end)
  so a crash mid-batch preserves work already scrubbed.
- **Ordered consumer.** Each stage is drained from the queue in declared order
  (`Claim → run → Complete/Fail`, dead-lettering after `maxAttempts`). Transfers ground
  the narratives; vibe reads the fresh narratives + heat; a **completed vibe enqueues
  its sigil** (before completing the vibe row, so a crash re-runs vibe rather than
  dropping the sigil). Sigil drains via the existing generator with `SkipUnchanged`.
- **`runStart` is no longer a correctness boundary** in `cmd/pipeline` — it's gone from
  the derive path entirely (the ad-hoc `cmd/sentiment` sibling still uses the watermark).
- **Maintenance scrub ticker demoted** to backlog/repair only (comment-level): the daily
  run scrubs its own fresh batch; the ticker mops up the older tail, real-time inserts,
  and failed-in-run links.

### Crash recovery & idempotency

- `RequeueStale` (30-min lease) runs at startup to recover a crashed prior run's
  in-flight rows before anything new is claimed.
- Re-run with no fresh input ⇒ sweep returns an empty affected set ⇒ nothing scrubbed
  or enqueued ⇒ **no local model work** (the audit's "re-run unchanged input is skipped").
- Kill after scrub ⇒ restart drains the still-pending `pipeline_work` rows ⇒ derivation
  resumes from the database, not from memory.

## Files changed

- `go/internal/thirdparty/news.go` — `persistArticles` returns affected article IDs;
  `GetEntityNews` surfaces them (3rd return).
- `go/internal/corpus/corpus.go` — `Sweep` returns the `affected` map (+ keeps
  `runStart`); new `AffectedVettedEntities` and `CorpusVersion` helpers.
- `go/cmd/pipeline/main.go` — rewritten: requeue-stale → sweep → scrub-in-run →
  per-article enqueue → ordered queue drains (transfers/narratives/vibe/sigil);
  generic `drainStage`; vibe→sigil handoff; dropped the per-stage time-debounce flags.
- `go/cmd/sentiment/main.go` — updated the `Sweep` call to the new arity (unchanged
  behavior; still the watermark path).
- `go/internal/maintenance/maintenance.go` — scrub-ticker doc demoted to backlog/repair.
- `planning_docs/FIRST-GPT-AUDIT-FINDINGS.md` — F-007 resolved; F-009..F-013 added.

No migration: `pipeline_work` already exists (S7, migration 102).

## Verification

- `gofmt -l`, `go build ./...`, `go vet ./...` — clean. `go test ./...` — all pass (the
  `TEST_DATABASE_URL`-gated `internal/work` integration tests skip CI-less, as before).
- **Read-only validation against the live prod schema:** `pipeline_work` confirmed empty
  (S7 substrate still inert); `AffectedVettedEntities` and `CorpusVersion` queries
  execute and return the expected shapes (e.g. a FOOTBALL team fingerprinting to
  `89 vetted links : <epoch>` — a new vetted link advances both halves and reopens work).
- **Bounded end-to-end smokes** (real RSS + real local model, against prod):
  - NBA (`-scrub-limit 2`): sweep `ok=30 fail=0 fresh_articles=274` — the **new
    affected-IDs return works** (274 freshly-linked articles surfaced); `scrub-limit`
    capped 274→2.
  - NFL (`-scrub-limit 1`): sweep `fresh_articles=303`, capped 303→1.
  - In BOTH, the scrub local model calls hit the 180s Ollama timeout (cold/contended GPU,
    see F-014) and the pipeline was **correctly fail-closed**: `scrubbed=0 →
    entities_enqueued=0 →` every drain `ok=0 fail=0 →` clean `EXIT=0`. A failed scrub
    produced **no** derivation and **no** queue rows — the fail-closed invariant holds.
  - Ollama capacity was diagnosed (F-014): `local-model:tag` (8B) is partially CPU-offloaded
    on the 8GB GPU; warm latency for a small generation is ~7.5s, but the 1200-token
    scrub under contention with the API's own local model workers exceeds 180s.
  - **Happy-path confirmation** (raised `OLLAMA_TIMEOUT_SECONDS=600` to clear the
    environmental blocker, `-scrub-limit 1 -limit 1 -min-articles 5`): the full chain
    ran end-to-end — `scrub: done scrubbed=1 failed=0 entities_enqueued=2`, then every
    stage drained real work **in declared order**: `transfers ok=1`, `narratives ok=1`,
    `vibe ok=1`, `sigil ok=1`. The **vibe→sigil handoff fired** (the sigil enqueued at
    vibe completion drained successfully). A brand-new RSS input thus reached scrub →
    transfers → narratives → vibe → sigil in **one run** — the audit's "Done when."
  - The smoke's two vetted entities were both teams (an NFL article co-mentions two
    teams); with `-limit 1` each stage drained 1 and left the 2nd entity's
    `narratives/transfers/vibe` rows **pending** in `pipeline_work`. That is `-limit`
    behaving as designed — and it doubles as a live demonstration of the **durable
    resume** property: those pending rows survive process exit and are claimed by the
    next run's drain (verified via `pipeline_work_status`: 3 pending rows, 0 attempts).

## Deploy

`cmd/pipeline` is a **cron** binary (`scripts/hosting/cron-pipeline.sh` execs
`go/bin/pipeline` fresh each night at 00:00) — deploying = rebuilding `go/bin/pipeline`
(done), **no** systemctl restart. The shared-package edits (corpus/news/maintenance)
also compile into `scoracle-api`; an API restart is **not** required for S8 correctness
(the API doesn't touch `pipeline_work` until Session 9, and `GetEntityNews` has no API
caller), but the next `scoracle-api` rebuild should include them (F-013).

## Session-9 / Session-12 handoff

- **S9** closes the residual single-article scrub→enqueue window by triggering enqueue
  on the `vetted=TRUE` transition / article-scrub-complete, drains pending work on
  startup even if a NOTIFY was missed, and adds bounded concurrency. It also makes the
  real-time LISTEN/NOTIFY workers enqueue durable work instead of representing
  completed eligibility directly.
- **S12** owns the Sigil convergence lifecycle: season-scoped hash/previous-score/
  debounce, decoupling generation from follower/FCM early-returns, the real `DryRun`
  field, momentum-as-versioned-input (F-011), the stats-rail `rating → sigil` producer
  (F-010), and converting the nightly run into reconciliation/backfill-only.
- **S13** adds the per-job advisory lock (overlap prevention) + `pipeline_runs` record
  (F-012).
