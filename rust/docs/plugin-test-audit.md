# Plugin architecture final test audit

This is the milestone 7 audit of the landed plugin architecture. Decisions are by coherent
contract group: a test is valuable when it detects a consequential failure that another retained
group does not already detect. Test count is recorded only to make the change reproducible.

## Retained contract map

| Contract | Retained evidence | Decision and reason |
|---|---|---|
| Open plugin registration and composition | `studio::plugin::tests`, `application::fleet::tests`, `application::plugins::tests`, and the unrelated-task worker rehearsal | Keep the negative boot checks (duplicate identity/task, empty task, route/grant mismatch), real fleet registration, configuration parsing, and a database-backed unrelated plugin. Together they detect closed enums or kernel edits, invalid production manifests, and configuration regressions. |
| Scoped capabilities | `application::models::tests` and `application::tools::tests` | Keep. These cross the production capability constructors and prove undeclared routes/domains are absent before reach, declared operations pass the gate, URL classes cannot be spoofed, and per-run budgets bind. |
| Queue claims and scheduling | `application::queue::work` and `application::queue::worker` unit/integration groups | Keep. They separately cover exact-claim requirements, retry timing, notification/disjoint claims, revision and reclaim fencing, dependency eligibility, shared/global admission, starvation, timeout, independent outbox progress, and open-task execution. |
| Durable fan-out and crash recovery | `application::queue::outbox::postgres_recovery_tests` plus the worker recovery rehearsals | Keep. Transaction rollback, dispatch-before-ack replay, injected dispatch failure/backoff, identity fan-out, actual child-process death, polling without notification, and dispatch while a handler is blocked are distinct failure windows. |
| Claim-fenced product publication | Ignored adapter suites for Analyst, Editor, Fixture Boxscore, Graph, Influencer, Insider, Investigator, Journalist, Oracle, and Scout | Keep. These are the only tests that exercise real transactions for current claims, stale revisions, same-revision reclaim, rollback, required events, markers/debounce, partial progress, and final completion. Overlap in fencing scenarios is intentional because each adapter has a different multi-write transaction. |
| Cognition product contracts | Cognition and adapter unit groups under each `plugins/<name>/` package | Keep by product family. They cover parsers, abstention/no-material distinctions, citation and identity grounding, deterministic scores/directions, evidence selection, input hashes, provenance, and production regressions. Similar-looking cases encode different evidence states or previously observed failures rather than a shared pass-through abstraction. |
| Shared session and publishing form | `studio::session::surface_tests` and `plugins::support::{form,guards}` | Keep. Session tests distinguish bounded publishing correction from structured-task correction and exhaustion; shared form/guard tests protect served prose normalization and global invariants used by multiple plugins. |
| Evidence preparation | `evidence::{fetch,memories,news,story_parts,trajectory}` | Keep. These cover hostile/malformed HTML, source throttling, missing-vs-zero semantics, correction invalidation, packet attribution/contradiction/budgets, storyline selection, and trajectory vocabulary. The ignored packet lease test uniquely covers cross-host takeover. |
| Routing and provider behavior | `runtime::{config,route,providers}` | Keep route identity compatibility, candidate routing, model/backend reuse, per-host concurrency, context windows, provider request translation, and incomplete-output refusal. One historical same-host sharing case was consolidated into the general distinct-model backend-sharing test. |
| Ledger provenance | `runtime::ledger::postgres_tests` and adapter ledger assertions | Keep. The runtime test proves persisted plugin identity/output contract provenance; adapter tests prove product-specific evidence and parser outcomes. |
| Evaluation rubrics and active fixtures | `evaluation::{tasks,judge}`, `bin/eval` tests, `fixtures/contracts`, and every `fixtures/quality/<task>` case | Keep the selected corpus. Offline tests validate current versions, authored review criteria, bidirectional rejection/acceptance axes, rubric failure behavior, and capture/replay boundaries. Live fixture execution remains an explicit model-quality gate rather than part of service-free Cargo tests. |
| Supported operator CLIs | `bin/eval`, `bin/factsweep`, and `bin/statcommentary` tests | Keep parsing and durable-enumeration compatibility tests. They protect public invocation shapes and the nightly hash/staleness selection without requiring a live model. |
| Low-level utilities | `util`, fetch cleaning, and numerical helper tests | Keep only boundary/Unicode/normalization and known production-regression cases. These functions sit below several plugins and failures would corrupt parsing, hashing, or evidence rather than merely change internal layout. |

## Consolidated, rewritten, and deleted

- Deleted manifest tests that repeated descriptive `consumes`, `produces`, context-provider,
  contract-version, and internal-product declarations. Those fields had no runtime consumer, so
  the declarations themselves were removed rather than preserved solely for their tests.
- Deleted unused `ProductKind` and `ProviderId` types, the unused manifest contract version, and
  unused registry query/convenience APIs. Prompt/output versions remain at the persistence and
  ledger boundaries that consume them.
- Deleted the unenforced `WorldRead` and `Commit` grant labels. Database preparation remains an
  adapter-owned typed API and publication remains claim-fenced; only inference and web grants
  remain because their concrete capability constructors enforce them.
- Consolidated fleet checks into durable task identifiers, production resource policy, scoped
  acquisition grants, and real full/partial registration. Removed exact roster duplication and
  constructor-field tests already guaranteed by the implementation.
- Consolidated duplicate registry resolution and evaluation task-registry tests into their
  broader exhaustive cases. Removed the historical same-host route split test already covered by
  backend reuse, and removed the exact Editor fixture/property count snapshot in favor of semantic
  fixture coverage that permits adding useful cases.
- Rewrote the transfer fixture-axis test: it previously asserted a condition only inside an `if`
  guarded by the identical condition. It now requires every current transfer fixture to carry a
  stage or confidence axis. A deliberate temporary removal of both axes from one fixture made the
  retained test fail at that fixture; the fixture was restored before validation.

No active fixture was deleted. Both files under `fixtures/contracts/` are included by deterministic
tests, and every quality directory is loaded by the current-contract fixture audit and `eval`.

## Known gaps

- Offline Cargo tests validate fixture structure and rubrics but cannot establish current model
  quality. Release/model changes still require `eval --task <task> --fixtures` on the intended
  backend and human review of the authored criteria.
- Factsweep and historical Rating non-queue wrappers do not have a full fake-model plus PostgreSQL
  end-to-end test. Their component preparation, inference scoping, publication, provenance, and CLI
  boundaries are covered, but one wrapper-level failure could still require an operator rehearsal.
- Network success behavior for real third-party sources is intentionally outside deterministic
  tests. Broker grants, URL classification, budgets, parsing, cache/lease behavior, and failure
  handling are covered without depending on the public internet.

These gaps do not weaken the durable worker acceptance gate, but they should guide future tests
when a concrete regression or a stable local provider harness justifies the maintenance cost.
