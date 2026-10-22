# Production recovery and simplification — October 22, 2026

Status: implementation plan only. No implementation, commit, push, deployment, restart, production repair, query cancellation, or credential change is authorized by creation of this document.

## Outcome

Keep the system simple:

```text
Go collects covered-team RSS headlines -> Postgres stores source data
Rust decision plugin -> local d1 -> durable character obligations
Rust character plugins -> local Qwen3.5 -> qualified products in Postgres
Go serves saved products
```

Repair executable contracts first. Remove work that has no consumer. Use the existing queue and edition records to finish each edition safely. Preserve the last qualified content while new work runs. No new platform, intelligence layer, model tier, article reader, or model tool calling.

## Confirmed user decisions

- Postgres stores collected data and enriched products; Rust is the AI harness; Go serves prepared products. Serving requests do not start inference or acquisition.
- Exactly two local models: the decision plugin calls d1; all six character plugins call Qwen3.5. Plugins send focused JSON content/instructions and receive JSON outputs. Models make no tool calls.
- News evidence consists of headlines, with publisher, timestamp, source identity and attribution. Do not introduce article-body ingestion or browser-using characters.
- Scout primarily expresses the target's measured statistical profile. Its only news input is availability headlines. Statistics remain measurements; a headline does not rewrite a numeric statistic or canonical identity.
- d1 scores exactly four needed dimensions: subject relevance, emotion, movement, availability. Remove clickbait from active scoring.
- Fetch headlines for the covered teams throughout the day. Stop new fetching around 22:00 America/Detroit, drain already queued d1 work until midnight, then freeze inputs and begin characters at 00:00.
- Fetching remains paused until character work and qualified publication finish, then starts an immediate catch-up sweep. The existing six-hour character deadline is not the target.
- Preserve Analyst's synthesis of Scout and Influencer, and Oracle's synthesis of all five character products. Their dependencies do not require extra models.

Use America/Detroit, including DST, everywhere. For collection day D, fetching stops at D 22:00, the edition starts at D+1 00:00, and its reporting date is D.

## Proposed safe defaults — not confirmed user choices

1. **d1 still pending at midnight:** keep fetching paused and the last qualified content visible; retain every captured decision obligation and start characters late only after that finite backlog settles. Record the delayed start. A strict midnight start cannot also promise a complete drain when d1 is behind. Never silently drop decisions, mark failed work drained, or move it out of view. Obtain agreement on this exception before production cutover.
2. **Publication:** expose an edition atomically after qualification. Candidate rows remain hidden. Preserve earlier qualified card versions for legitimate no-new-evidence outcomes, with their original dates; never represent old evidence as today's work.
3. **Exhausted failures:** stop retrying that exact input automatically, retain a visible blocker, and preserve the current published edition. A software or model failure is not a legitimate no-evidence result. Repair/retry or explicit operator disposition is needed before qualifying a blocked edition.
4. **Overrun into another midnight:** finish the same edition without overlapping or resetting it. Record the missed boundary; after completion, catch up fetching and use the next normal collection boundary. Do not build an automatic stack of overdue editions.
5. **Scope and history:** do not expand entity coverage or discover new entities. Covered-team feeds currently resolve known teams, players and persons; preserve existing supported output scope until explicitly changed. Keep only useful bounded, dated headline history. Removing history or player products is a separate product decision.

## Verified audit baseline

Snapshot: October 22, approximately 10:32–10:38 Detroit. These are observations, not facts to assume in a later implementation session.

| Area | Verified state |
|---|---|
| Mac backend checkout | Clean at f176a7e3387f0c4758092b8556930cdb41ee0711, branch codex/transfer-subject-validation |
| Mac worker | Separate older checkout, with untracked release/launcher files; running launcher uses releases/identity-availability-20261021-0b3eb01a |
| Archbox | Clean detached source checkout at 0b3eb01a; migrations 318 and 319 installed; cognition and coverage active |
| Serving API | Reports commit 31d4ba335e5a; database health passes. Checkout SHA does not identify every running binary |
| d1 | LiquidAI/d1-3B-GGUF via llama.cpp /v1/systemone; SHA-256 16aff27ea2eefdc32b9897f43854a5d3170c1dc8dccb9c756905af30a4e22402; four configured slots |
| Characters | All six routes use Qwen3.5-4B-Q4_K_M; four healthy 10,240-token slots, reasoning off |
| RSS | Latest sweep: 206/206 teams, 583 requests, zero RSS errors, about nine minutes |
| Storage | Archbox root about 4.7 GiB free; separate data disk about 1.6 TiB free; Mac about 31 GiB free |

October 22 edition: froze 00:00:22, ended 06:00:22 partial/character_deadline; 19,434 assignments, including 13,415 completed, 5,838 expired and 181 failed. Completed assignments include no-content outcomes. There were 3,007 publication receipts: Rating 2,031; Momentum 418; Sigil 338; Insider 148; Narratives 72; Vibe zero. The reopened October 21 edition also ended partial.

## Concrete defects and gaps

### 1. Vibe's publisher is broken independently of model quality

[rust/src/plugins/influencer/publish.rs](../../rust/src/plugins/influencer/publish.rs) inserts trigger_type='dayic' at line 55 and repeats it in ledger metadata. The live vibe_scores constraint permits milestone/manual/periodic/news_spike. Live logs repeatedly report this exact constraint error; all 91 current failed Vibe rows had it. The latest audited checkout still contains the typo.

Use the existing periodic trigger value consistently. Do not broaden the database constraint to accept a typo. A successful model result followed by a failed insert currently causes another model call on retry because attempt reuse selects only published/abstained outcomes. Reuse an already validated response for the exact same evidence/model/contract instead.

### 2. Availability scoring is not wired to Scout

[classification-criteria-v9-headlines.json](../../rust/fixtures/classifier/classification-criteria-v9-headlines.json) asks five questions. Subject routes Journalist/admission, emotion routes Influencer, movement routes Insider. Availability has no active routing consumer; clickbait has no active routing consumer and is stripped from writing input.

Adding a scout route alone is insufficient. [scout/mod.rs](../../rust/src/plugins/scout/mod.rs), around line 290, completes classifier-spectrum-v1 work as retired_article without expressing the profile or reading availability. [scout/prompt.rs](../../rust/src/plugins/scout/prompt.rs) renders selected statistical measurements only. Wire the new source contract through delivery, frozen preparation, input hash, validation, publication and acknowledgement.

### 3. Completion and visibility differ from the requested cycle

[309_continuous_coverage.sql](../../sql/migrations/309_continuous_coverage.sql) defines coverage/draining/characters, with a 15-minute drain and six-hour character limit. [318_coverage_freeze_set_based.sql](../../sql/migrations/318_coverage_freeze_set_based.sql) expires unfinished work and reopens fetching at that deadline. A failed daily upstream assignment correctly blocks dependents. Removing only the time limit would leave permanent dependency failures unresolved.

Products become visible as they are inserted. [go/internal/db/db.go](../../go/internal/db/db.go) selects latest reporting dates; its Vibe leaderboard uses the sport-wide maximum date around line 434. The first new card can narrow the board to a partly populated day. There is no qualified-edition visibility barrier.

### 4. Replay and unconditional fanout waste work

[classifier_replay_admission](../../sql/migrations/315_headline_entity_pipeline.sql) materializes retained-corpus revisions before applying a batch limit. [classifier/adapter.rs](../../rust/src/plugins/classifier/adapter.rs), around line 270, runs this as a recurring scheduled operation without the controller's server-side timeout.

At the snapshot, PID 66552 had run it for more than 36 minutes, locally connected with an empty application name; it was the largest observed PostgreSQL CPU consumer. Morning PID 65602 was absent. No query was canceled. Ownership was not conclusively established.

The freeze creates all six assignments for each active team/player. October 22 included 2,921 Vibe and 3,044 Transfers assignments with no spectra. Rating's empty spectra are not themselves a defect: its primary input is statistics. Normal RSS persistence already queues current headline decisions; broad archive replay does not belong in the ordinary daily path.

### 5. Parser success is not factual qualification

The [October 21 qualification receipt](../progress_docs/2026-10-21-model-qualification-and-availability.md) records release-blocking false accepts in the 44-case assessment. Analyst/Oracle parsers do not receive source evidence; Insider's checked findings do not fully bind its served body. These are distinct from transport, capacity and publication problems.

[tools/fresh.rs](../../rust/src/tools/fresh.rs), validate_named_facts, requires every detected name to co-occur with every number across a narrative. Two correctly attributed sentences about different subjects and different scores can fail. All 285 current failed Narratives rows were guard rejections; the audit did not determine how many were false rejects. Preserve grounding while fixing this cross-product check; do not remove guards merely to increase throughput.

## Ordered implementation phases

Implement bounded phases with focused tests and a reviewable diff. Do not deploy between incompatible schema/runtime halves. Use new forward migrations; do not edit already applied migration history.

### Phase 1 — restore the successful publication path

- Fix Vibe's dayic typo in publisher and ledger.
- Add a database regression through the actual claim-aware successful Vibe publisher, including the migrated trigger constraints, source settlement and downstream completion event.
- Retain/reuse validated output when publication fails, keyed by exact request/evidence/model/contract. Keep generation retry separate from transaction retry; do not create another job system.

Owners: plugins/influencer/{mod.rs,publish.rs,tests.rs}; harness/queue/publication.rs only if a shared fix is necessary.

Gate: one eligible Vibe input publishes exactly once; a publication retry does not call Qwen again; stale claims publish nothing.

### Phase 2 — four questions, real Scout availability, bounded daytime work

- Version decision criteria to subject/emotion/movement/availability; remove clickbait. Keep current subject/emotion/movement thresholds until focused evidence justifies changing them. Calibrate the availability threshold against named injury, return, denial and irrelevant-headline cases; do not invent it.
- Route availability plus subject relevance to Scout, for supported team/player targets only. Reuse the existing spectrum delivery mechanism, not the retired packet/Editor rail.
- Freeze a small availability headline section alongside the measured profile. Include its source IDs, timestamps, contract and hash in the Scout request/retry provenance. No transfer, mood or general news enters Scout.
- Replace the retired_article shortcut for the new availability contract; coalesce repeated injury headlines into one subject/edition assignment; validate source currency and settle the used/abstained/held deliveries transactionally.
- Availability qualifies wording about participation only. It must not fabricate diagnosis, duration, recovery, statistical changes or canonical metadata. With no usable profile, retain explicit missingness; do not invent a replacement statistical card.
- Remove recurring whole-archive admission replay from production. Keep ingest-time enqueue. Recovery reconciliation must inspect a bounded set of changed/missing revisions with a persisted cursor and server-side time/lock limits. Do not automatically rescore old receipts when the criteria version changes.

Owners: plugins/classifier/{decision.rs,adapter.rs,delivery.rs,manifest.rs}; fixtures/classifier criteria; plugins/scout/{mod.rs,prompt.rs,snapshot.rs,publish.rs}; forward SQL migration for dispatch/reconciliation.

Gate: exactly four answers; availability alone reaches Scout; clickbait is absent; ordinary ingestion cannot trigger a corpus-wide revision scan; decisions and source revisions are not lost or published twice.

### Phase 3 — one durable daily lifecycle and qualified visibility

Reuse the current Postgres edition tables and pipeline_work:

| Phase | Required behavior |
|---|---|
| Collect | Hourly covered-team sweeps while open; d1 continuously settles current revisions |
| Drain, D 22:00 | Stop starting RSS requests/sweeps; fence persistence against the same boundary; capture the finite admitted decision backlog, including running work and delayed retries |
| Wait for midnight | Continue d1 for captured obligations; do not start characters early merely because the queue becomes empty |
| Characters, D+1 00:00 | If drained, freeze the edition and start characters; fetching remains paused. Apply the proposed late-drain exception explicitly if agreed |
| Qualify/publish | All required work resolved, no unresolved blockers or handoffs; atomically expose qualified products and refresh edition-aware serving/cache identity |
| Reopen | Immediate catch-up sweep, then normal coverage until the next collection boundary |

- One persisted phase and IANA calendar rule coordinate both hosts. Keep repeatable transitions and the existing shared/exclusive publication fencing.
- Drain checks must include backoff and exhausted obligations, not just rows available now. Stop broad history reconciliation from injecting unbounded new work during drain.
- Remove six-hour forced expiry, automatic partial resume, and missed-window branches tied to that deadline. A timeout belongs to a job, not to global edition completion.
- Persist meaningful outcomes: candidate product, justified no-material/unchanged result, retryable error, blocked error. A deleted work row or best-effort ledger entry alone is not evidence of useful publication.
- Keep exact claim tokens, input revisions, durable outbox and disjoint SKIP LOCKED claims. Inference happens outside publication transactions. Bound retry work; keep model timeout < handler timeout < lease, or renew a lease explicitly when justified.
- Bind candidate products to an edition. Add the minimum published-edition state needed; Go must read qualified versions, not merely max(reporting_date). Old failed candidates must never leak when a later edition is published.
- Carry forward only earlier qualified products where the explicit no-new-evidence policy permits it, with original dates. Account for no-evidence markers, withdrawal and historical-date queries.
- Include edition identity in cache keys/ETags and responses where needed. Each response uses one publication boundary.
- On restart, resume the persisted phase and remaining assignments. On another midnight, keep the active edition; no overlapping run or silent attempt reset.

Owners: forward migrations replacing policies from 309/310/311/315/318; harness/queue/{coverage.rs,coverage_tests.rs,work.rs,outbox.rs}; go/cmd/pipeline/{monitor.go,monitor_test.go}; go/internal/{corpus/corpus.go,thirdparty/news.go,db/db.go,cache/cache.go}; relevant handlers.

Gate: late work continues beyond six hours; qualified content survives failure/restart; fetch resumes only after successful qualification/publication; 22:00/midnight/DST behavior is reproducible.

### Phase 4 — remove unnecessary work without changing character roles

- Schedule evidence-eligible base assignments. Resolve empty news obligations without inference. Schedule Scout for changed usable measured inputs or availability; preserve a dated unchanged outcome when no new expression is needed.
- Preserve Analyst's Scout/Influencer dependency and Oracle's five-product dependency. Use compact typed products plus source references and qualifications; do not feed increasingly verbose free-prose histories or invent a new fact ontology.
- Consolidate duplicated dependency definitions in SQL and Rust manifests while preserving database-enforced same-edition readiness. Failed peers must not masquerade as completed/no-evidence peers.
- Collapse production per-role model/candidate routing into one shared Qwen client with one aggregate four-slot budget. Keep plugin identity, instructions and output schema distinct. d1 remains its separate local decision client.
- Replace the simple headline-history rank subprocess with the same bounded sort in Rust, using parity tests. This removes the character worker's helper boot dependency without deleting useful dated history or unrelated DuckDB analytics.
- Remove inactive Investigator/fixture acquisition seats, web capability setup and unused model-route knobs from the production roster after checking current consumers. Do not broaden acquisition.
- Archive obsolete Harvester/Editor queue obligations only in a separately approved maintenance action. Preserve source/product history and harvester_query_provenance, which current headline admission still consumes.
- Reconcile old launchers/service descriptions and READMEs with the two-model topology. Do not remove other personal model services until their consumers are checked.

Owners: harness/{registration.rs,fleet.rs,route.rs,config.rs}; plugin manifests/preparation; classifier/memories.rs; tools/memories.rs; go/internal/analytics/duckdb/memory.go; hosting configuration and documentation.

Gate: fewer eligible calls and runtime dependencies, unchanged product roles and factual meaning; no second queue, DAG engine or serving-time inference.

### Phase 5 — qualify Qwen's simplified production contracts and measure capacity

Keep requests small and JSON-based. Preserve source IDs/currency, exact sport/type/ID, dates, denial and uncertainty. Bound output length. Validate supported names, numbers and quotes against the cited headline, not pooled unrelated reports. Canonical metadata identifies the subject; it is not news evidence. Generated peer prose is not new independent corroboration.

Use the existing production factories, parsers and retry paths for qualification. Repair overly broad guards with positive as well as negative fixtures. Keep a held-out human-reviewed factual set; JSON validity and name/number presence cannot prove semantic correctness. No extra judge model or model replacement is required by this plan.

Record: admitted decisions, queue age and retry/backoff; d1 drain duration; eligible assignments; calls and repeated calls per exact input; actual products versus no-material/unchanged/blocked outcomes; input/output tokens; slot-seconds; preparation/database time; publication time; edition completion and fetch-resume time. Measure against the real covered population before changing concurrency or hardware. Four healthy idle slots after a forced deadline do not establish insufficient capacity.

Gate: zero observed critical subject/factual errors in the agreed qualification set; justified abstentions remain available; no unexplained positive-case loss; measured d1 work fits the two-hour drain on representative days and character work makes sustained progress.

## Essential acceptance matrix

Run mutation-based contracts only on a fresh disposable PostgreSQL 18 database, using scripts/check.sh and focused additions. Never aim fixture tests at production.

| Case | Expected result |
|---|---|
| Successful Vibe and transaction retry | One product and handoff; no duplicate inference for saved valid output |
| Four-question routing | Subject, emotion, movement, availability only; irrelevant and denied/ambiguous availability handled correctly |
| Scout boundary | Measured profile retained; only availability news included; revised evidence changes hash; no statistical/identity mutation from headlines |
| 21:59/22:00/in-flight RSS | No new request after closure; persistence boundary fenced; admitted work retained |
| Empty queue at 22:05 | Characters wait until local midnight |
| Midnight with d1 retries/backoff | No false drain; agreed late-drain policy applied, no silent loss |
| Repeated ticks/two hosts | One edition; disjoint claims; stale execution cannot publish |
| Crash at each transition/after inference/after dispatch | Durable progress resumes; no lost handoff or duplicate product |
| Failed/absent peer | Distinguish error from legitimate no-material; no stale peer presented as current |
| Six-hour and next-midnight overrun | Same edition continues; no forced resume, overlap or attempt reset |
| Publication/cache/withdrawal/history | Candidate invisible; atomic qualified switch; original dates retained; old rejected candidates never leak |
| DST March/November, source revisions, duplicate headlines | Correct America/Detroit calendar; immutable input identity; correct source fencing/deduplication |
| Factual adversarial and held-out cases | Preserve negation, subject and numbers; detect unsupported chronology/causality; allow correctly attributed multi-subject sentences |

Do not call a green software suite model-quality approval. The audit inspected tests but did not execute state-writing suites.

## Rollout, rollback and definition of done

Implementation may prepare and test local changes. Commits, pushes and production actions require their own authorization; deployment requires separate approval. Never cancel an unknown query or run a watchdog/alert script as incidental verification.

Before a proposed release: verify a clean candidate and its exact test evidence; schema/runtime compatibility on both hosts; actual API/worker/helper/model identities; current backup and root-disk headroom. The snapshot had different API and cognition commits. Do not overwrite the separate worker checkout's untracked releases/launcher.

First exercise saved inputs in a disposable environment, then prepare a hidden candidate edition on the intended transport under separately approved capacity limits. Require several successful natural cycles before claiming recovery; proposed gate is three, including a restart rehearsal. No known critical factual defects, unaccounted obligations, duplicate publications or candidate leakage.

Rollback should switch serving to the previous qualified edition, with coherent cache identity, and use schema-compatible archived binaries if needed. Do not restore withdrawn contaminated cards or incorrect frozen packets. Reconcile active claims before any runtime rollback. Do not solve rollback by destructively restoring the entire database.

Done means: two models; four useful decision questions; Scout expresses measured statistics with availability-only news; one durable edition; 22:00 pause and midnight start under the agreed exception policy; bounded retries/replay; qualified atomic visibility; preserved prior good content; immediate fetch catch-up after completion; measured workload fits normal operation.

## Implementation-session launch prompt

```text
Read /Users/scotty/scoracle/scoracle-backend/docs/planning_docs/PLAN-production-recovery-2026-10-22.md first, then the repository README, run_docs/DEVELOPMENT.md, rust/README.md, SQL/hosting instructions and applicable AGENTS.md/.agents/skills. Honor my chosen Codex model and Ponytail settings.

Verify the current branch, tracked/untracked changes, source and deployed versions before relying on the audit snapshot. Preserve unrelated work, especially the separate scoracle-worker releases and launcher. Implement this plan in ordered, bounded phases with focused regression tests and the required disposable-database checks. Keep Postgres -> Rust enrichment -> Postgres product -> Go serving; d1 plus Qwen3.5 only; four questions; headline news; Scout statistics plus availability only; JSON in/out; no model tools. Clearly surface proposed product/exception choices before making them production policy.

This authorizes local implementation and tests, not commits, pushes, production writes, deployments, restarts, query cancellation, credentials, spending or alerts. Prepare concrete reviewable changes. Deployment requires separate approval.
```
