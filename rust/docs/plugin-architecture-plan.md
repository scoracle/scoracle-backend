# Plugin architecture execution plan

Status: execution started, 2026-09-22.

## Target

The harness is the durable space in which cognition runs. Plugins supply the work:
preparation, tools, inference policy, prompts, parsers, validation, products, and
domain reactions. Adding an independent capability must not require teaching the
execution kernel about a character, sport, product table, or downstream plugin.

Plugins are statically linked Rust packages/modules. Dynamic loading, a general
workflow language, and a context-plan interpreter are not prerequisites.

The harness owns registration, invocation, deadlines/cancellation, resource limits,
bounded retries, claim fencing, atomic publication coordination, and durable event
delivery. Its storage and inference backends implement narrow host interfaces.
The composition root selects plugins and binds concrete adapters.

Each plugin owns its typed preparation and cognition, correction policy, product
writer, domain events/reactions, and fixtures. Shared equipment is reusable plugin
support or an injected provider. Pure cognition receives prepared context and
explicit capabilities. A plugin's Postgres adapter may use SQL; it must not own
claim acknowledgement or bypass the host's publication boundary.

This revises the README ownership boundary deliberately: publication coordination
becomes a host responsibility through a storage adapter, while domain SQL remains
with plugins. No Postgres-specific transaction code belongs in the inference core.

## Desired layout and dependency direction

```
studio/                 generic cognition/session and plugin contracts
plugins/<name>/         preparation, cognition, adapters, manifest, tests
plugins/support/        shared publishing form and domain evidence helpers
application/            composition root and concrete durable host adapters
application/queue/      generic durable scheduling and event delivery
runtime/                configured inference, transport, storage implementations
evaluation/             invokes the same plugin preparation/cognition as production
bin/                    thin invocation entry points
```

File counts and names within a plugin follow its needs. Infrastructure must not
import concrete plugins, except in the explicit composition root. Existing
database stage strings, route keys, prompt revisions, and output-schema versions
remain stable through mechanical changes. These identities have different meanings.

## Invariants

- Validate the exact lease token and input revision before every claim-bound effect.
- Product effects, required provenance, and required follow-up intent commit atomically.
- Support both progress commits and final completion. Preserve Insider checkpoints.
- Preserve no-material, unchanged, terminal, abstention, and superseded outcomes.
- Optional diagnostic telemetry remains best-effort; it is not a durability receipt.
- Run model/network preparation outside publication transactions.
- Preserve database-level dependency eligibility across multiple worker processes.
- Keep domain fan-out and subject mapping explicit. Product family names alone do
  not describe completion barriers, target entities, revisions, or debounce rules.
- Scoped handles constrain ordinary application access; they are not a sandbox for
  arbitrary native Rust plugins.

## Milestones

### 1. Make the executable contract honest

- Replace the unsupported task array with one typed durable stage per registration.
  Remove the open-ended TaskKind string and its panic-producing conversion.
- Use typed routing roles instead of a second role spelling. Retain multiple roles
  where a plugin may perform different inference operations.
- Validate inference declarations at registration; correct Editor/Investigator grants.
- Document the remaining descriptive metadata and unenforced grants accurately.

The existing Stage enum is a transitional storage vocabulary, not the final
extensibility mechanism. Milestone 4 removes that central enumeration requirement.

Gate: current fleet constructs; duplicate ownership and inconsistent inference
declarations fail clearly; offline library/binary tests pass; no SQL, prompt, route,
resource-limit, or product behavior changes.

### 2. Colocate plugins without changing behavior

- Move studio/<plugin> and application/<plugin> into one owning plugin module,
  retaining separate cognition and adapter submodules where useful.
- Move manifests next to their owners; keep the roster in the composition root.
- Move publishing form/guards into plugin support. Convert plugin-specific Studio
  extension methods into plugin functions.
- Remove dead alternative publication APIs when migrating their helper/test callers.
- Preserve shared preparation paths used by production, evaluation, and CLI tools.
- Keep mechanical moves separate from lifecycle changes in reviewable patches.

Gate: compile all targets, run offline tests, compare prompt and SQL literals and
route selection before/after. The inference kernel imports no concrete plugin.

### 3. Centralize durable publication mechanics

- Introduce small host operations for claim-fenced progress and final publication.
  The host obtains the transaction and claim lock; plugin adapters write domain
  effects; the host coordinates obligations, completion, and commit receipts.
- Derive durable outcomes from successful host operations. Do not require every
  claim to produce a product or every plugin to use a single transaction.
- Migrate a simple publisher first, then the other voices, acquisition, Editor,
  and Insider. Preserve each existing atomicity boundary deliberately.
- Persist the applied-identity-to-rating obligation atomically with identity changes
  if immediate propagation is required; retain documented recovery behavior.
- Add plugin identity to diagnostic provenance through a compatible schema migration;
  retain task, operation, prompt, and output-contract identity.

Gate: isolated migrated Postgres tests for stale/reclaimed claims, rollback, progress
followed by failure, no-product completion, and crash/replay. Compare row effects,
hashes, and required follow-ups. Frozen model outputs cannot prove transaction safety.

### 4. Remove domain policy from the durable worker

- Register task keys and scheduling policy without editing a closed character enum.
  Preserve stored stage strings and inventory database constraints/producers first.
- Move Desk storyline maintenance, sport/week sealing, and packet compilation to
  plugin-owned scheduled operations; the host supplies generic cadence and shutdown.
- Extract domain eligibility and ordering from work::claim while preserving atomic
  database-level gates. Do not replace them with process-local scheduling order.
- Move direct downstream knowledge into explicit event reactions owned by plugins.
  Events include subject, revision, and effect/completion identity. Transactional
  enqueues remain valid where they satisfy the same atomic intent contract.
- Shadow-compare each migrated edge, including debounce and terminal paths, before
  switching dispatch. Retain existing barriers until parity is demonstrated.

Gate: an unrelated test plugin can register and execute without kernel edits;
multi-worker tests preserve eligibility, fairness, and durable fan-out; no newsroom
or character names remain in generic scheduling code.

### 5. Narrow execution capabilities and separate cognition policy

- Inject resolved inference handles and run deadlines instead of Arc<Models>.
- Move configurable route ownership out of the character-specific Role enumeration
  when introducing plugin-owned registration. Milestone 1's typed roles remove
  duplicate spellings but are a transitional dependency, like Stage.
- Supply plugin-owned typed preparation functions with scoped read/provider handles.
  Extract common concrete loaders only when they remove real duplication.
- Route Editor fetching through the shared provider boundary while preserving its
  retrieval semantics. Enforce declarations at the actual capability construction site.
- Let plugins supply correction instructions; the session kernel owns only bounded
  retry mechanics. Preserve existing publishing corrections during migration and
  validate structured-task corrections separately on fixtures.
- Avoid a provider registry or context-plan interpreter unless demonstrated runtime
  requirements cannot be met cleanly by ordinary typed functions.

Gate: cognition cannot obtain the raw pool/global router through its API; capability
refusals are exercised through real invocation paths; prompts, hashes, provenance,
and quality fixtures retain expected behavior.

### 6. Complete coverage and simplify

- Make factsweep and inline historical rating generation plugin invocations, with
  appropriate non-queue run contexts rather than fabricated live claims.
- Remove superseded APIs and metadata without consumers. Update tests as required
  by those removals; defer the comprehensive test audit and pruning to milestone 7.
- Update README/source map and operational guidance to describe the landed system.
- Verify that adding a plugin requires its package and composition registration,
  with schema changes only for genuinely new persisted domain data.

## Execution and validation record

- Baseline: `cargo test --lib --bins --offline`: 562 passed, 57 ignored.
- Ignored tests require isolated migrated Postgres. They have not been run and are
  mandatory before accepting lifecycle or scheduling changes.
- Preserve the pre-existing Insider person/player filtering change and unrelated
  working-tree edits. Do not commit, deploy, or mutate a live database as part of
  this initial implementation slice.
- Milestone 1: complete. One typed task per registration; typed role declarations;
  inference grant/role consistency checked at boot; Editor and Investigator grants
  corrected; registry resolution uses its validated task index; comments distinguish
  live guarantees from planned boundaries. Removed the false compound-task test and
  redundant role-spelling tests; added invalid-declaration and fleet-registration cases.
- Validation: `cargo test --lib --bins --offline`: 562 passed, 0 failed, 57 ignored.
  `cargo check --all-targets --offline`, `cargo fmt --all -- --check`, and
  `git diff --check` passed. No SQL, prompts, route selections, or resource limits changed.
- Milestone 2: complete. All ten plugins now live under `src/plugins/<name>/`.
  Each owns its manifest and adapter; the nine inference plugins also own a cognition
  module. The roster lives in `application/fleet.rs` and dependency binding stays in
  `application/plugins.rs`. Shared form, guards, and resource constants live in
  `plugins/support/`. Production, evaluation, binary, example, and test imports follow
  the new owners; fixture includes and subprocess test filters were updated.
- Editor, Graph, and Investigator operations are plugin functions accepting Studio.
  Uncalled product markers use a read-only model-name accessor. The unused Publisher
  trait, Outcome enum, and Analyst/Influencer publication wrappers were removed;
  Analyst tests now exercise the production creation function. The obsolete wrapper
  publication-failure test was removed; real adapter durability tests remain.
- Milestone 2 validation: `cargo check --all-targets --offline` passed without warnings;
  `cargo test --lib --bins --offline`: 561 passed, 0 failed, 57 ignored. Formatting and
  diff whitespace checks passed. A comparison against the pre-move working-tree
  snapshot checked 9,323 string literals, including 235 raw literals: differences
  were limited to test paths and test scaffolding. SQL and prompt text, explicit
  inference route selections, and all ten manifest policies are unchanged.
- Deliberately retained boundaries: session correction still depends on shared form
  support until milestone 5; Boxscore's missing-fixture path uses a narrow bridge to
  the same Investigator transaction until milestone 3. Studio imports no concrete
  plugin. Domain policy still exists in the worker until milestone 4. These are source
  ownership changes, not a claim that capability or lifecycle isolation is finished.
- Follow-up for scoped inference: verify actual routes against each declaration,
  including Insider identity adjudication's existing EmotionalNews route.
- Milestone 3: started with the Graph publisher. The durable host now owns a small
  claim-fenced publication transaction that validates the exact lease and input
  revision, lends a borrowed transaction for plugin-owned domain SQL, and issues
  progress or final commit receipts. Graph is the first migrated final publisher;
  unchanged/no-product completion uses the same boundary. Its SQL, preparation,
  diagnostics, and product behavior are unchanged.
- Milestone 3 continued through the five voice publishers: Analyst, Influencer,
  Journalist, Oracle, and Scout now use the same host-owned final publication
  boundary. Their plugin adapters retain domain SQL while required outbox intent,
  product or marker writes, and exact completion share the host transaction.
- Milestone 3 database validation: a disposable local Postgres 17 cluster was built
  from the checksummed baseline and was already current through all migrations. The
  five migrated Graph cases and 24 affected voice publication cases passed serially.
  The complete ignored Rust database suite then passed: 57 passed, 0 failed. This
  covers stale and reclaimed claims, rollback, no-product completion, required
  follow-ups, crash/replay, progress survival, scheduling, and durable dispatch.
- Milestone 3: complete. Fixture Boxscore, Investigator, and Editor final publication
  now use the host boundary; Boxscore no longer reaches through Investigator to complete
  a missing fixture. Insider pair rows, identity applications, and score wrappers use
  claim-fenced progress commits, while its team barrier uses final completion. Raw claim
  lock/completion helpers are confined to the durable queue implementation.
- Applied transfer identities now persist their player/old-team/new-team Rating intent
  atomically via `267_transfer_identity_rating_outbox`; dispatch remains idempotent and
  retryable. `268_cognition_ledger_plugin_identity` adds compatible nullable plugin
  provenance, backfills known historical stages, and new diagnostic writes take identity
  directly from the owning manifest.
- Milestone 3 final validation: the disposable database applied migrations 267 and 268
  cleanly. The isolated Rust database suite passed serially with 59 tests, including new
  rollback/dispatch coverage for applied-identity Rating fan-out and a real ledger insert
  retaining plugin identity. Full offline, formatting, compile, and whitespace checks
  passed. SQL and prompt literals in plugin domain behavior were not changed.
- Milestone 4: started. The host now runs plugin-owned scheduled operations through a
  generic cadence/shutdown loop. Editor owns storyline dormancy, sport/week sealing,
  packet compilation, velocity adaptation, and exact-title deduplication; packet config
  is bound in the composition root rather than passed into the worker. Voice registration
  order moved to the composition root, and Oracle owns its five-pillar completion barrier.
  The moved SQL and stored stage strings are unchanged.
- Milestone 4 slice validation: `cargo check --all-targets --offline` passed without
  warnings. `cargo test --lib --bins --offline` passed with 562 tests and 59 ignored;
  formatting and diff whitespace checks passed. The ignored database suite was not rerun
  because this slice mechanically relocates its existing barrier and maintenance SQL.
- Milestone 4 continued: durable tasks now use the open `TaskKey`; no enum variant is
  required for registration. Each first-party plugin manifest owns its stable `TASK` string. Schema
  inventory confirmed `pipeline_work.stage` has no allow-list constraint; its primary key,
  claim indexes, SQL producers, and stored strings remain unchanged. The existing
  `entity_type` allow-list is a separate persisted-subject constraint.
- Claim ordering and database eligibility are now manifest-owned `ClaimPolicy`. The host
  accepts only bounded ordering variants and binds upstream task/event blocker arrays into
  one generic `FOR UPDATE SKIP LOCKED` query. Momentum and Sigil retain their exact
  cross-worker dependency and pending-outbox barriers without being named in queue SQL.
- Milestone 4 task/claim validation: `cargo test --lib --bins --offline` passed with
  564 tests and 59 ignored; compile, formatting, and whitespace checks passed. A fresh
  checksummed local database was migrated through 268, then the complete ignored database
  suite passed serially: 60 passed, 0 failed. This includes the existing multi-worker
  eligibility/fairness cases and a new unrelated-plugin test that registers an arbitrary
  task string, claims it through the production worker, executes it, and completes it under
  the exact claim without kernel changes.
- Milestone 4 event-reaction slice: the durable outbox now dispatches a registered chain
  of plugin-owned reactions rather than switching on domain event kinds. Analyst owns
  Momentum enqueue decisions, Oracle owns its completion barrier reaction, and Scout owns
  applied-identity Rating intent. The composition root installs the complete statically
  linked reaction set on every host independently of its enabled task handlers, so split
  worker fleets retain cross-process fan-out and no host can acknowledge a partial chain.
  The host continues to own row locking, bounded retry/backoff, deletion receipts, and the
  generic cadence/shutdown loop. Stored event kinds, stage strings, SQL, debounce behavior,
  and terminal barriers are unchanged.
- Milestone 4 event-reaction validation: `cargo test --lib --bins --offline` passed with
  565 tests and 60 ignored. `cargo check --all-targets --offline` passed without warnings.
  The complete ignored database suite then passed serially against the disposable migrated
  PostgreSQL cluster: 60 passed, 0 failed, including dispatch failure/backoff, crash/replay,
  idempotent fan-out, claim fencing, cross-worker eligibility, and unrelated task execution.
- Milestone 4 event ownership completed: the generic outbox now exposes one claim-bound
  event insert operation and contains no product/event names. Influencer, Analyst, Scout,
  Journalist, and Insider own their stable event keys and producer wrappers; Oracle and
  the other reactions reference those plugin-owned identities explicitly. The production
  SQL statement and all persisted strings remain byte-for-byte unchanged. The full offline
  suite again passed with 565 tests and 60 ignored, and the complete isolated PostgreSQL
  suite again passed serially: 60 passed, 0 failed.
- Milestone 4: complete. The transitional `Stage` source alias and all ten kernel-owned
  first-party task constants are removed; generic scheduling accepts only open `TaskKey`
  values while plugin manifests remain the sole owners of stored task spellings. Generic
  scheduler comments and registry fixtures no longer depend on newsroom identities. The
  composition test now enumerates all seven durable event mappings, including reaction
  order for the two multi-effect edges, and the database suite exercises their row effects,
  debounce/terminal paths, failure backoff, crash replay, and multi-worker gates.
- Milestone 4 final validation: `cargo test --lib --bins --offline` passed with 565 tests
  and 60 ignored; `cargo check --all-targets --offline`, formatting, and diff whitespace
  checks passed without warnings. The complete ignored PostgreSQL suite passed serially:
  60 passed, 0 failed. No migration, stored identifier, SQL behavior, prompt, route, product,
  or resource-limit change was introduced.
- Next: milestone 5, injecting scoped inference/preparation capabilities and moving
  correction policy out of the session kernel while preserving current routes and outputs.

## 7. Final test audit — deferred until all architecture work is complete

Start only after milestones 1–6 and their acceptance checks are complete. Until then,
run the checks needed for each change and maintain affected tests; do not undertake
general test cleanup. The current test count is neither a quality measure nor a
target to preserve. No arbitrary reduction quota or replacement coverage quota.

Audit the landed implementation, including ignored database tests, fixtures, helpers,
and subprocess rehearsals. For each test or coherent group, identify the concrete
failure it detects, why that failure matters, and whether another test already detects
it. Record a keep, consolidate, rewrite, or delete decision with a short reason.
Do not infer value or authorship from a test's name or the suite's size alone.

### Keep when the test proves a distinct, consequential contract

- Durable behavior: exact-claim fencing, reclaimed leases, atomic rollback, crash
  recovery, idempotent replay, required follow-ups, partial progress, and final completion.
- Scheduling and resource behavior: cross-worker eligibility, starvation prevention,
  deadlines, cancellation, and actual concurrency limits.
- Capability boundaries exercised through production invocation paths: undeclared
  access is refused and permitted operations receive the intended scoped capability.
- Product correctness: meaningful parser/validation failures, abstention and missing
  evidence semantics, provenance, deterministic calculations, and input identity where
  a change should trigger or suppress work.
- Representative production regressions and quality fixtures that protect groundedness
  or useful output. Test domain promises without freezing incidental prompt wording.
- Compatibility at actual external boundaries: persisted identifiers, schema contracts,
  configured routes, and supported CLI behavior.

These are candidates, not blanket exemptions. Require each retained case to add
evidence that a simpler or broader test does not already provide.

### Consolidate or remove when the test adds no independent protection

- Assertions that repeat constants, manifest spellings, field assignments, or the
  implementation's own calculations without an independent expected result.
- Tests of properties already guaranteed by Rust's types or removed abstractions.
- Duplicate cases across layers that exercise the same code and failure mode.
- Snapshots of internal layout, formatting, or prompt wording with no product contract.
- Trivial pass-through mocks, artificial helper behavior, and tests whose only effect
  is to make harmless refactoring expensive.
- Historical fixtures for unsupported behavior; overlapping examples that do not
  represent distinct edge cases. Consolidate useful cases where it improves clarity,
  not merely to lower the reported function count.

### Completion gate

Produce a concise map from retained contracts to their tests, document important gaps,
and remove redundant tests and orphaned scaffolding. For questionable retained tests,
use a small deliberate fault or equivalent check to confirm they detect the claimed
failure; avoid a blanket new mutation-testing framework. Run the retained offline and
isolated-database suites, reporting what actually ran. An ignored test is not evidence
of a working guarantee. Finish with fewer maintenance obligations and clear protection
for the durable harness and plugin contracts; test count and coverage percentages
alone cannot establish completion.
