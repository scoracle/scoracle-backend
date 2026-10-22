# Production recovery — implementation and cutover receipt

Initial local verification: phases 1–4 are implemented. Phase 5 has factual regression repairs, saved-output replay and capacity instrumentation; its model-quality and representative-capacity acceptance gates remain open. **This does not establish factual/model or representative-capacity qualification.** At the initial local checkpoint, no commit, push, deployment, production write, production service restart or query cancellation had been performed; the subsequent authorized cutover is recorded separately below.

## Production cutover authorized October 22

The user subsequently authorized putting the recovery approach on main and deploying it, accepting a live production trial while factual qualification and representative capacity remain unproven. The described late-drain, blocker, carry-forward and nonoverlapping-overrun behavior is the selected trial policy; availability 90 remains a provisional calibration rather than human gold.

The initial transition preserves the already-served legacy surface while the publication pointer is `NULL` (responses identify it as `legacy`). It never labels those rows qualified, restores withdrawn rows or exposes a new candidate. The first complete qualified edition switches away from this compatibility surface. This replaces the earlier requirement to manufacture a qualified legacy baseline. A disposable regression covers legacy visibility before the first pointer, candidate exclusion and removal of legacy fallback after qualification.

A fresh production database backup and archived binaries/configuration precede cutover. With both workers stopped, a one-time retry of old failed character inputs is permitted after the software repairs; past partial editions and their outcomes remain historical. Retired queue seats retain their records but have no registered production consumer. The normal new controller never resets exhausted work. Enrollment begins at the real cutover time, then uses the next natural 22:00/midnight boundary.

The final cutover regression also exited 0 against fresh `classifier_test_coverage_cutover`, including the legacy fallback visibility regression, all Rust/SQL checks, prepared statements and Go race tests. Log: `/tmp/scoracle-cutover-20261022/check.log`. The fresh production backup completed at 14:22 Detroit (7.7 GiB, readable archive table of contents and SHA-256 recorded privately).

Deployment verification will be appended after the running API, workers, schema and collection have been checked. Do not use the older raw-product API as a rollback against new hidden candidates: pause workers and retain qualified serving, or switch its pointer to a previous qualified edition with a revision increment.

## Initial baseline and preserved work

At the initial checkpoint, the original backend checkout was on `main` at `387c5fe9312e28a89282483146f0e6de0eb7ff94`, with the user's untracked recovery plan preserved. Implementation is in the attached `production-recovery` worktree at `f176a7e3387f0c4758092b8556930cdb41ee0711`, the plan's audited baseline containing migrations 318 and 319. Changes were uncommitted. The separate worker checkout, releases and launcher were unchanged. Its qualification archive was read and extracted privately under the disposable test directory; raw production inputs were not added to this repository.

At the initial checkpoint, Archbox, the serving API and running model/binary identities had not been reverified for deployment. The baseline is a source revision, not evidence of current deployed identity.

## Confirmed decisions and proposed policy

Confirmed: Postgres owns durable data/products; Go collects covered-team headlines and serves saved products; Rust owns enrichment; d1 answers four questions; all six characters share Qwen3.5; no model tools or article reader. Scout expresses measured statistics with availability-only reporting. Collection closes at 22:00 America/Detroit, characters wait until midnight, and fetching remains paused through completion. Analyst and Oracle retain their existing roles and dependencies.

The implementation exercises the plan's originally **proposed defaults**, selected for the authorized production trial:

- Late d1 drain delays characters and keeps collection paused, retaining every captured obligation.
- Publication switches atomically. Explicit no-new-evidence/unchanged outcomes may carry earlier qualified cards with their original dates. An abstention/withdrawal does not silently resurrect a card.
- Exhausted exact-input failures remain visible blockers until explicit repair/disposition.
- Overruns finish the active edition, record missed boundaries and do not stack or reset editions.
- Existing supported entity scope and bounded useful headline history remain intact.

The Scout availability threshold of 90 is also provisional. It is measured against the small synthetic calibration below, not a confirmed product choice or independent human gold set.

## Ordered implementation

### Phase 1: publication retry

Vibe uses the existing `periodic` trigger. Its generated response can be reused after a failed publication only for the exact request/evidence/model/contract. Reuse still runs validation, source-currency and claim fencing. The actual publisher regression injects a failed insert, reclaims work and proves one model call, one product, one completion event and no stale publication. A crash before the response is saved can still repeat inference.

### Phase 2: decisions, Scout and bounded reconciliation

Migration 320 and criteria v10 activate subject, emotion, movement and availability; clickbait is absent. Existing subject/emotion/movement thresholds remain 55/15/80. Archived five-question receipts remain readable without being automatically rescored. Only admitted team/player availability reaches Scout through the new `classifier-availability-v1` source contract.

Scout freezes source IDs, dates, contract and availability provenance with its measured profile and input hash. It deduplicates repeated publisher headlines and bounds the section to eight complete sources / 4 KB of retained source records. Unusable or oversized inputs are held explicitly. Availability wording must be a separate verbatim attributed headline; numeric measurements remain independently checked against the statistical evidence. Without a usable profile, Scout retains missingness and settles availability without inventing a statistical card. Claim-fenced publication revalidates and transactionally settles used, abstained, redundant and held deliveries.

Recurring archive replay is removed. Explicit recovery reconciliation uses the existing enqueue path, a persisted `(article_id,sport)` cursor, at most 1,000 inspected revisions, and 15-second statement / one-second lock limits. It is fenced by the collection boundary. Normal ingestion still queues current revisions.

Calibration used 24 serial requests to the existing local d1 endpoint: two 12-case trials covering injury, return, denial, uncertainty, rest, suspension, wrong identity, historical injury, transfer, result, tactics and emotion. The second four-question instruction and threshold 90 separate the six intended availability positives from the six negatives, with wrong identity excluded by subject relevance. The selected run took about 8.1 seconds; its slowest call was about 3.31 seconds. These assistant-authored labels and this tiny run establish neither population accuracy nor two-hour drain capacity. The checked-in [calibration receipt](../../rust/fixtures/classifier/availability-calibration-receipt-2026-10-22.json) records the pinned d1 identity, scores and request hashes.

### Phase 3: durable lifecycle and qualified serving

Migration 321 replaces deadline expiry with persisted drain membership and explicit assignment resolutions. The 22:00 transition captures running, backed-off and exhausted classifier work. An empty queue at 22:05 does not start characters early. After midnight, every captured obligation must be settled before freeze. Six-hour and subsequent-midnight overruns preserve the same edition and attempts. Deleting a work row alone cannot establish character completion.

Candidates are edition-bound. The published-edition pointer changes in the completion transaction only after required assignments and handoffs resolve. Qualified views exclude unfinished and rejected editions, including old rejected candidates after a later publication. All Go product readers, including profile Vibe series, use those views. Each response reads one repeatable-read boundary; cache keys, ETags and the edition response header include its revision. Withdrawals invalidate that revision. Historical queries retain qualified original dates.

RSS checks the same collection predicate before each request, applies the next Detroit cutoff as its request deadline, and fences persistence with the existing transition lock. Collection reopen triggers immediate catch-up. No production schedule was changed during local implementation.

**Initial cutover:** legacy rows remain the compatibility serving surface only until the first qualified pointer, without being declared qualified. Old partial editions remain partial. Active legacy assignment outcomes require explicit reconciliation; the verified October 22 cutover starts from coverage with no active edition.

### Phase 4: less runtime work

Empty news assignments resolve without inference; unchanged Scout inputs keep their dated outcome. SQL owns Analyst/Oracle dependency definitions, including same-edition readiness and failed-peer barriers. Production ignores per-role model/candidate overrides and uses one shared Qwen client. A database claim lock enforces four aggregate running character jobs across hosts, alongside the existing client semaphore. Startup verifies model timeout < handler timeout < lease.

Headline-history ranking is the same bounded Rust sort, with DuckDB parity coverage for lenses, ties, window boundaries and limits. The character startup helper dependency is removed. Investigator and fixture acquisition are absent from the production roster but retained for explicit offline callers. No old queue obligations or source history were deleted. Launcher/service documentation and schema preflight match the candidate; the historical service filename remains compatible.

### Phase 5: qualification and measurement

Named numerical claims are checked per sentence/clause rather than crossed against every name in an entire narrative. Positive multi-subject/decimal cases and negative switched-subject cases are covered. Analyst and Oracle validate against their prepared peer evidence. Insider binds its served prose to headline evidence and rejects unsupported chronology. Scout rejects invented sample limits and observed counts derived from policy minima. These are conservative lexical/structured checks, **not semantic entailment or human factual approval**.

The read-only `qualify_saved` example reassembled/reparsed the archived 44-case release assessment through current factories and production guards, asserting that factual evidence and identity were unchanged. It made zero model calls:

| Character | Mechanically accepted | Rejected | No call |
|---|---:|---:|---:|
| Analyst | 4 | 1 | 1 |
| Journalist | 8 | 2 | 1 |
| Oracle | 1 | 3 | 0 |
| Scout | 6 | 5 | 0 |
| Insider | 2 | 5 | 0 |
| Influencer | 5 | 0 | 0 |
| Total | 26 | 16 | 2 |

The five newly rejected saved replies correspond to previously identified factual failures (shared decline, basketball attribution, unsupported chronology, incompatible comparisons/sample limits, and unknown sample counts). A rejection does not by itself prove that the guard diagnosed the semantic error correctly. The replay contains targeted incident cases and previously seen holdouts; it is not a new independent qualification set, a current-prompt model run, or a full bounded-rewrite trial. The [machine-readable receipt](../../rust/fixtures/quality/recovery-saved-replay-2026-10-22.json) contains case outcomes, not private source packets.

The shared model client records every production character call's prompt/request hashes, input/output tokens and occupied-slot time, including replies later rejected; failed calls have separate events. Worker spans identify exact claims/revisions, execution time and publication transaction time. The [read-only capacity report](../../sql/queries/recovery_capacity.sql) summarizes ready wait/state age, retries, captured drain, assignment outcomes, actual product rows, handoffs and recorded generations. Queue timestamps describe current state/ready wait rather than lifetime age across revisions. Preparation/database/parsing time is a residual of execution minus model/publication time, not an independently instrumented database profiler. Best-effort generation ledgers must be reconciled with call logs.

## Verification

Testing uses fresh disposable PostgreSQL 18.6 databases under `/tmp/scoracle-recovery.rIWb3T`, bound to localhost port 55432. No fixture test targets production. The two existing live/private tests are excluded from the ordinary database suite.

Final `scripts/check.sh` exited 0 against the fresh `classifier_test_coverage_recovery_final` database: baseline checksum/restore, migrations through 321 and a zero-pending-migration rerun, SQL measurement/freeze/qualified-visibility contracts, formatting, Go vet/build, 366 Rust library tests plus 17 ordinary binary tests, 67 serial integration/helper checks, all prepared statements and all Go race tests. The serial checks include cross-host, cross-role four-slot budget enforcement, Rust/DuckDB rank parity, source fencing, Scout missingness/settlement, Vibe retry, DST, persistent overruns, qualified carry/history and withdrawal. Log: `/tmp/scoracle-recovery.rIWb3T/final-check.log`.

The release/install simulation passed with fake external commands in a temporary tree; it never restarted real services. The read-only capacity SQL executes successfully on the disposable schema. Saved replay completed with 44 cases and zero calls. Release schema preflight also passed against the final disposable database. `git diff --check` passes for backend and wiki; the original backend still has only the untouched untracked plan. The wiki has a small candidate-status update and handoff, preserving its unrelated frontend note. The session-owned disposable server was stopped after verification.

## Qualification still required after the authorized live cutover

1. Trial the selected lifecycle/carry-forward policies and legacy compatibility transition; do not describe legacy cards as qualified.
2. Run the simplified contracts through the actual Qwen transport and bounded retry path against an agreed, independently human-reviewed held-out set. Require zero observed critical subject/factual errors and explain positive-case losses. Existing parser passes do not satisfy this gate.
3. Under separately approved capacity limits, exercise a hidden representative candidate edition. Measure the real covered population, two-hour d1 drain, character progress, repeated calls, preparation/publication time and fetch resume. No representative capacity result is claimed here.
4. Verify both hosts and matching schema/consumers during cutover. A rollback rehearsal to a previous qualified pointer remains unavailable until such a pointer exists; preserve withdrawals.
5. Deployment is now authorized. Observe natural successful cycles, including a restart rehearsal, before claiming sustained production recovery. Three cycles remain a proposed acceptance period, not a completed result.

The smaller implementation reuses the existing queue, edition records, receipts, model router and SQL projections. No new scheduler, judge model, fact ontology or acquisition system was added.
