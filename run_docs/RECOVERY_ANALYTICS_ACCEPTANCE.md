# Recovery and cohort acceptance

**September 20 update:** the API now maintains all supported cohort scopes with DuckDB on startup and every five minutes. The manual-only canary and scheduling gates below describe the earlier rollout. See the [SQL contract](../sql/README.md) and [completed cutover](sql-duckdb-cutover-2026-09-20.md) for current ownership and deployment evidence. Recovery and parity requirements still apply.

This procedure separates isolated recovery proof, live read-only observation, an approved live recovery canary, and analytical producer cutover. Passing one is not evidence for the next. Production actions below are preparation, not authorization.

## Deployed checkpoint — September 19, 2026, 23:45 EDT

User-authorized Archbox transition completed on runtime build `ebac7942cebd`. All six production binaries were staged from that commit; installed hashes and both running process images matched. The old workers stopped before migrations 256–261 committed atomically. Both services and all nine stages started successfully, and the exact original cron schedule and rebuild watchers were restored. Local and public smoke tests each passed all 18 checks. A fresh 2.7 GiB backup restored successfully into a separate PostgreSQL 18 cluster, where the same migration batch and API statement registration passed before the live transition. The isolated cluster is stopped.

A one-item normal NFL Rating run completed Rating → Momentum → Sigil with observed distinct claim tokens, three products/provenance records, and no remaining work or outbox backlog. The observer did not catch the short-lived outbox rows; it observed the downstream claims and final empty outbox, rather than recording a complete event-by-event outbox history. The original 1,218 terminal failure rows retained their exact pre-transition digest. No live fault injection or historical repair was performed.

Migration 262 was applied separately. Fresh NBA/2025 team and player exports passed parity and published 30/253 rows, with publication times of 5.05/13.18 ms and process RSS of 73.1/74.8 MiB. Full process times were 182/69 ms. Every serving field matched retained output exactly; fresh-process rebuilds passed. These are individual canary observations, not proof of sustained performance or a 5% contention budget. No automatic cohort schedule or broad provider switch was installed.

The active deployment evidence is in `/mnt/data/backup/scoracle/releases/studio-20260920` on Archbox. The backup also has a matching-checksum copy at `/home/sheneveld/scoracle-offdisk-backup/studio-pretransition-20260920.dump`. Preserve these and the prior binaries through at least two complete acquisition cycles. **Do not use the historical runbook's generic old-binary rollback:** stop new consumers and roll forward with compatible fenced code; pre-256 workers cannot safely consume this state.

Remaining gates are overnight acquisition/derivation results, sustained resource/freshness observation, and a separately scoped live recovery fault rehearsal. Detailed handoff: [production transition](../../scoracle-wiki/progress_docs/scoracle-backend/2026-09-19_studio-production-transition.md).

## Implemented boundary

`analytics-snapshot` reuses `go/internal/analytics/duckdb`'s existing cohort formula and `analytics_entity_context` publication contract. It exports one complete sport/entity-type/season population and its immediately preceding season in a repeatable-read, read-only transaction. It retains NULL inputs, closes the transaction before compute, and gives identical rows to a PostgreSQL reference and a private DuckDB instance. No live attachment is used on this path.

The manifest includes scope, formula `cohort-context-v1`, fixed `as_of`, actual `captured_at`, PostgreSQL MVCC snapshot ID, SHA-256 input hash, and all replayable inputs. `as_of` is an observation/scenario label, **not historical time travel**: season ratings have no versioned historical source permitting reconstruction at an arbitrary timestamp. The two engines use the same label and rows. Source hashes detect changes, including deletions and NULL transitions; they are content revisions, not monotonic sequence numbers.

Membership, input ratings, deltas, prior-season selection, league boundaries, NULL/zero distinctions, counts and output order must match exactly. Only unrounded percentile/interpolated quantile fields permit absolute error <= `1e-9`, fixed before comparison. This formula has no persisted rounding. Missing current ratings are omitted; missing prior ratings retain a current row without a delta; peers include self; prior seasons never bridge leagues. Equal deltas share `percent_rank`; singleton rank is zero. No reporting-week calculation is being migrated.

Boundaries: at most 100,000 input rows; one PostgreSQL connection; 15-second SQL statement timeout during export/publication; 2-second publication lock acquisition timeout; 2-minute CLI deadline; private in-memory DuckDB with 256 MB buffer budget, two threads and 256 MB spill limit. Buffer limits do not cap total RSS. The process-wide resource cost must be measured on the destination host before scheduling. Reports are mode 0600 and cannot overwrite an existing file.

## Commands

Run from `go/`, with credentials already supplied through the environment:

```sh
go run ./cmd/analytics-snapshot -sport NBA -entity-type team -season 2025 \
  -as-of 2026-09-20T02:40:00Z -output /tmp/nba-team-cohort.json

go run ./cmd/analytics-snapshot -replay /tmp/nba-team-cohort.json \
  -output /tmp/nba-team-cohort-rebuilt.json
```

Shadow is the default and sets PostgreSQL `default_transaction_read_only=on`. It cannot publish, clear dirty work, enqueue cognition or call a model. A replay recreates a fresh DuckDB engine and reruns both calculations over retained inputs. The report contains inputs and both outputs; stdout records final timing and publication outcome. The report is flushed before an optional publication, so process loss after commit leaves replay evidence. Its `published:false` describes the pre-publication artifact; the durable receipt is authoritative.

`-publish` explicitly enables replacement through migration 262. It validates complete result membership, acquires a per-scope advisory lock and a short SHARE lock on the source stats table, re-reads/hash-checks both source seasons, then atomically replaces the whole cohort and `analytics_cohort_publication` receipt. The table lock closes the validation/commit race, including insert/delete phantoms; it can briefly block **other sports' writers too**. It never spans analytical computation. Production acceptance must measure this cost; this conservative bounded mechanism is not permission to broaden scopes indefinitely.

Same batch replay does not rewrite products or receipt. Older `as_of` cannot replace newer publication; equal-time differing inputs/results are rejected. An in-flight source correction rejects the old result, requiring fresh export. Deletion replaces the complete population, including empty populations. The cohort table itself is the serving projection: a late projection/receipt error rolls the entire transaction back. There is no separate refresh to acknowledge prematurely. No Momentum dirty marker is touched. Refresh after subsequent source changes remains the responsibility of the one scheduled/manual owner; this change does not add automatic invalidation or schedule itself.

## Rehearsal

Only against the designated disposable database, schema plus migrations through 262 (including the migration-255 cohort table):

```sh
# from rust/
TEST_DATABASE_URL=postgresql://scotty@127.0.0.1:55439/editor_test \
  cargo test --lib -- --ignored --test-threads=1
# from go/
TEST_DATABASE_URL=postgresql://scotty@127.0.0.1:55439/editor_test \
  go test ./internal/analytics/snapshot -v
```

Synthetic fixtures only. Rust tests launch child OS processes which exit without destructors before publication commit, after commit/before dispatch, and after dispatch/before event acknowledgement. Other cases cover exact revision/reclaim fencing, committed Insider partial progress after reconnect and supersession, durable outbox backoff, and an actual worker safety tick without any LISTEN connection. Fake creation/terminal handlers ensure no real model service is called. Existing Oracle readiness, worker concurrency/retry semantics and Graph `g5` remain untouched.

The cohort test proves MVCC consistency across a concurrent source update; exact parity including ties/zero/NULL/league moves/singletons; destroyed/recreated DuckDB state; idempotent receipt; correction during compute; old-result rejection; late receipt failure after projection writes with full rollback and retry; complete deletion; and unchanged unrelated dirty work. It uses synthetic NBA seasons 2197–2198, not copied production data. Never point these tests at production.

## Momentum projection recovery (local readiness fix)

The Go maintenance owner now publishes `refresh_momentum_scores`, one concurrent refresh of `latest_momentum_scores_per_entity`, and exact dirty-marker deletion in one transaction. It holds the existing SQL producer advisory lock for the whole drain; overlapping drains skip without acknowledging work. A projection failure, acknowledgement failure, or cancelled connection rolls back scores and projection together and leaves the dirty marker for retry. A newer `last_marked_at` survives publication. The five-minute in-process throttle advances only after commit.

This closes the previously documented acknowledgement-before-projection gap; the change is deployed in `ebac794`. It changes no formula, producer owner, model routing, or DuckDB selection and requires no migration. The transaction has a two-minute context deadline, a 60-second per-statement timeout and a two-second lock timeout. All selected sports commit or roll back together. Measure the longer transaction and advisory-lock hold under the next acquisition cycles; the new live binary includes this fix, but a populated full-load Momentum refresh has not yet been measured.

`REFRESH MATERIALIZED VIEW CONCURRENTLY` is legal inside a transaction and retains concurrent reader access. Migration 227's historical comment claiming otherwise is incorrect; its removal of the blocking write-trigger remains valid. See [PostgreSQL refresh documentation](https://www.postgresql.org/docs/17/sql-refreshmaterializedview.html). Synthetic tests exercise the actual concurrent-refresh statement, preserve the prior projection through late failure, recover after cancellation, serialize competing drains, and preserve a source update arriving during publication:

```sh
# from go/; this test creates and drops a separate synthetic database.
# The disposable database role must have CREATEDB.
TEST_DATABASE_URL=postgresql://scotty@127.0.0.1:55439/editor_test \
  go test ./internal/maintenance -count=1 -v
```

## Live inspection and historical repair

Read running API build identity, Rust startup stamp and executable hashes; repository HEAD alone is insufficient. Read actual service environment (allowlist only), stage/route startup logs, migration ledger and table/trigger definitions, crontab/timers, aggregate queue state, outbox availability, analytical dirty markers and cohort freshness. Do not dump credentials or assume runbook model names are current. Obtain all SQL observations in read-only transactions with statement deadlines.

Historical failed rows predate fencing. A product's presence does not prove it belongs to the failed attempt; absence does not prove a model call is safe to repeat. For a separately approved repair set, correlate exact subject/revision/time, product and required provenance, pair progress, evidence effects, current source fingerprint and follow-up existence. Classify complete/no repair, partial/targeted required-effect repair, stale/superseded, and unknown/manual review. Never bulk-reset attempts, replay settled identities, or enqueue every historical failure. Diagnostic ledger alone is not transactional proof.

## Reviewable canaries and rollback

Two independent transitions; do not change provider selection, analytical ownership and invalidation ownership together.

1. **Studio recovery transition:** refresh live evidence and inventory every status-only queue writer/consumer, including cron and manually running binaries. Archive exact executable hashes, unit configuration and a recoverable database backup. Pause producer launches and rebuild path watchers; drain/stop all old queue consumers before applying 256–261. Verify zero old running processes/leases, migrations and trigger definitions, then start only the reviewed claim-aware binaries. SQL packet/weekly fanout remains its current owner. Do not reopen terminal failures. First observe naturally arriving bounded work, record exact claim/product/outbox lineage, then obtain a separately explicit fault-injection approval if a live kill/restart is needed. Local crash tests are not a live recovery pass.
2. **Cohort publication canary:** choose NBA/2025/team first (30 observed outputs), then NBA/2025/player (253 observed outputs). Re-export before publishing; retained shadow inputs are not assumed current. The single owner is one explicit `analytics-snapshot` invocation per scope. Disable any old manual/job writer for these scopes before enabling `-publish`; no cohort cron was found during inspection, but reconcile the prior manually produced rows with the operator first. Apply only reviewed 262 as its own authorized migration. Keep default `ANALYTICS_ENGINE=postgres`, SQL rating/Momentum producers, invalidation and all model routes unchanged. Run exact bounded commands above with a fresh `as_of`, new artifact path and `-publish` only after approval. Do not install an automatic schedule in this first canary.

Proposed acceptance budgets, to agree before live writes: zero semantic mismatches/stale publications; RSS <=128 MiB for the measured NBA scopes; spill <=16 MiB; export <=1 second; total batch <=2 seconds; publication and source-writer blocking <=250 ms; no cognition calls or dirty acknowledgements; API latency and cognition throughput within 5% of a same-host baseline over two full acquisition cycles. Measure database CPU/I/O, lock waits, publication cost and host contention; LAN shadow timings alone cannot pass this gate. Abort on any loss/duplication of required effects, missing lineage, new revision cleared, parity mismatch, budget breach or freshness regression.

Cohort rollback: stop the new owner, retain last committed projection/receipt and pending work, and leave 262 installed. A failed transaction already preserves the previous projection, as rehearsed. Do not blindly restore an old snapshot over corrected sources. An old producer may resume only after the new owner is stopped and a bounded fresh reference computation has been approved; two writers must never overlap. Retain prior binary/artifacts for at least two complete acquisition cycles before retiring the old entry point.

Studio rollback: stop the new consumer and retain queue/outbox rows. **Do not restart the pre-256 status-only worker alongside token-aware work.** The previous production binary cannot dispatch new outbox obligations. Reconcile/verify these with compatible code before any downgrade, or remain stopped and roll forward. A mixed-version downgrade and live restart have not yet been rehearsed; that gate remains open, rather than being implied by additive DDL compatibility.

## Evidence limits

Local tests, live read-only shadow parity, live recovery and analytical cutover must each be recorded separately in the wiki progress log. No deployment, production migration, public publication, historical repair, extra model call or producer transition is implied by running the shadow tool. A new formula/evidence-quality study and learned judgment remain deferred.

References: [PostgreSQL repeatable-read snapshots](https://www.postgresql.org/docs/17/transaction-iso.html), [DuckDB resource limits](https://www.duckdb.org/docs/current/operations_manual/limits). The CLI measures export, reference, private-engine load/compute and optional publication separately; use an OS resource meter for RSS, CPU and I/O.
