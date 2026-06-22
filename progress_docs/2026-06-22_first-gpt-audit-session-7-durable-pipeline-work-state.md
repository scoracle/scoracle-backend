# First GPT Audit — Session 7: Introduce durable news-pipeline work state

**Worked:** 2026-06-22 (archbox)

**Plan:** `planning_docs/FIRST-GPT-AUDIT.md`, Session 7

**Designed with (implemented separately):** Sessions 8 (ordered compile→scrub→derive→reveal
pipeline) and 9 (real-time trigger semantics). Session 8 **depends on** this one.

**Product authority:** wiki `Product Narrative`

## Goal

Give the database a durable answer to: *"What backend derivation work is pending, running, or
failed?"* — replacing the news pipeline's in-process `runStart` watermark and best-effort
`LISTEN/NOTIFY`, neither of which survives a crash or guarantees that changed inputs are
reprocessed.

## Scope boundary (why this session is the substrate, not the rewiring)

The audit says Sessions 7–9 should be **designed together but implemented separately**, and
Session 8 depends on 7. So Session 7 delivers the **durable state model + its access
primitives + an operator view + tests**. It deliberately does **not** rewire the live pipeline
to produce/consume the queue — that is Session 8 (producers + ordered consumer) and Session 9
(trigger-enqueues-work semantics). Keeping the producers/consumer out of Session 7 also means
the table stays **empty and inert in production** until its consumer exists, so we are not
accumulating undrained work or adding write-load to the in-flight scrub coverage build.

The exact producer call-sites and the consumer are enumerated below as the Session-8 handoff.

## Design

### `pipeline_work` (migration `102`)

One **generic** table (the audit's "prefer one generic table over several specialized queues"),
keyed `(stage, entity_type, entity_id, sport)`:

```
stage, entity_type, entity_id, sport,
status ∈ {pending, running, failed},   -- minimal, as specified
attempts, available_at, updated_at, last_error, input_version
```

- **Entity-keyed, derive stages only.** Scrub is *article*-keyed and already has a durable queue
  (`news_article_entities.scrubbed_at IS NULL` + its partial index), so it is **not** modeled
  here. `pipeline_work` covers the per-entity derive stages: `transfers`, `narratives`, `vibe`,
  `momentum`, `sigil`.
- **Only outstanding work is stored.** Completed rows are **deleted**, so the table is small and
  `pipeline_work_status` (a `GROUP BY (stage, status)` view) is a direct operator dashboard.
- **`input_version`** lets a changed input reopen completed/failed work instead of relying on
  elapsed-time debounce ("generated within N hours" ≠ "generated from current inputs").
- Two partial indexes: claim path `(stage, available_at) WHERE status IN ('pending','failed')`;
  stale-recovery path `(updated_at) WHERE status='running'`.

### `go/internal/work` — the primitives

- `Enqueue(ctx, q, Item)` — idempotent upsert. `Querier` is the pgx subset shared by `*pgxpool.Pool`
  and `pgx.Tx`, so a producer can enqueue **inside the transaction that wrote the input** (atomic).
  Conflict policy: a same-version pending/running row is left untouched (dedupe, no lease-yank); a
  **changed `input_version` or a `failed` row is reopened** to pending.
- `Claim(ctx, pool, stage, limit)` — leases ready rows (`pending`/`failed` past their backoff) with
  `FOR UPDATE SKIP LOCKED` in a CTE `UPDATE … RETURNING`, marking them `running`. Concurrent
  claimers get disjoint rows.
- `Complete(ctx, q, Item)` — deletes the row, **but only while still `running`**. If a newer input
  reopened it to `pending` mid-flight, Complete is a no-op and the reopened work survives.
- `Fail(ctx, q, Item, cause, backoff, maxAttempts)` — `→ failed`, bump `attempts`, record
  `last_error`, schedule `available_at` backoff; at `maxAttempts` it parks far in the future as a
  visible **dead-letter** rather than retrying forever.
- `RequeueStale(ctx, q, lease)` — flips `running` rows older than the lease back to `pending`
  (recover a crashed worker).
- `Counts(ctx, q)` — the `pipeline_work_status` view.

### `go/cmd/work` — operator CLI

- `work status` — pending/running/failed by stage (the "DB can answer" tool).
- `work requeue-stale [lease]` — recover abandoned leases (default 15m).

## Maps to the audit's verification

All four are exercised by `internal/work/work_test.go` against a real Postgres (see Verification):

- *Insert duplicate work → one row* — `TestEnqueueDedups`.
- *Crash after claiming → stale work retryable* — `TestRequeueStaleRecoversClaim`.
- *Two workers claim distinct rows* — `TestClaimIsExclusiveAndDrains` (SKIP LOCKED → disjoint;
  fully-leased stage yields nothing further).
- *Changed input reopens previously completed work* — `TestChangedInputReopens` (complete→delete→
  re-enqueue new version → claimable) + `TestEnqueueReopensInTableOnNewVersion` (reopen in place).
- Plus `TestFailBacksOffThenDeadLetters` for the dead-letter contract.

## Verification

- `go build ./...`, `go vet ./...` — clean. `go test ./...` — all pass (work DB tests **skip**
  without `TEST_DATABASE_URL`, keeping CI-less `go test ./...` green).
- **Real Postgres run:** spun an ephemeral throwaway PG 18 cluster (isolated port/socket, torn
  down after), applied migrations `101`+`102`, ran `go test ./internal/work/... -v` against it →
  **6/6 PASS**. Confirms the claim/reopen/requeue/dead-letter SQL, not just compilation.

## Session-8 handoff — producers + consumer (NOT done here)

Enqueue at (each inside the txn that wrote the input, using `work.Enqueue`):

- article/entity link inserted *and* its article scrubbed → entity ready for `transfers`/`narratives`
  (in `thirdparty/news.go` persist + `ml/news_scrub.go` `applyVerdicts` vetted-transition);
- transfer verdict changes → `narratives`/`vibe`;
- Rating or Vibe generation changes → `momentum`;
- trajectory changes → `momentum`; Rating/Vibe/Momentum change → `sigil`.

Consumer: a stage drainer (`Claim` → run Gemma → `Complete`/`Fail`) replacing the `runStart`
handoff in `cmd/pipeline`, ordered per the target flow; `RequeueStale` on a maintenance tick.
Session 9 then converts the real-time `LISTEN/NOTIFY` triggers to enqueue durable work rather
than representing completed eligibility directly.

## Files changed

- `sql/migrations/102_pipeline_work.sql` (new)
- `go/internal/work/work.go` (new)
- `go/internal/work/work_test.go` (new — `TEST_DATABASE_URL`-gated)
- `go/cmd/work/main.go` (new)
- `progress_docs/2026-06-22_first-gpt-audit-session-7-durable-pipeline-work-state.md` (this doc)
