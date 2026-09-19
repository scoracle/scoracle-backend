# Scoracle Studio and Rust application

**Studio is Scoracle's in-house harness.** [`src/studio/`](src/studio/) gives LLMs a place to create from prepared material. Postgres stores the evolving world; DuckDB studies it; Studio equips characters to express what the evidence supports.

This crate also contains the production application: routing, acquisition helpers, Postgres adapters, durable queue handling, and operator tools. Those concerns live outside Studio's core. They can share a binary without sharing ownership.

Read the [backend README](../README.md), [product narrative](../../scoracle-wiki/PRODUCT_NARRATIVE.md), [data-flow map](../../scoracle-wiki/DATA_FLOW.md), and [development rules](../run_docs/DEVELOPMENT.md) before changing a boundary. The [runbook](../run_docs/RUNBOOK.md) owns operational procedures.

## What is implemented

The Analyst is the first migrated character, based on the user’s committed `ea20950` runtime/composition/evidence reorganization. `src/studio/analyst/` owns its prepared domain types, `momentum-s27` prompt, JSON parser, deterministic product fields, and creation flow. The new memory material and fingerprint are retained. `src/application/analyst.rs` owns production retrieval, pillar-to-domain adaptation, model selection, queue invalidation, claim-aware publication, and the best-effort diagnostic ledger. Eval and fixture callers use Studio's prompt/parser contracts plus the explicit pillar adapter; the old Analyst junction is removed.

The Influencer is also migrated. `src/studio/influencer/` owns its `v32` brief, prompt builder, parser, prepared assignment, marker/product creation, and injected publication. `src/application/influencer.rs` owns retrieval, memory rendering/fingerprints, early debounce, and claim-aware Postgres publication. `src/application/outbox.rs` durably retries the post-publication Momentum offer and Oracle barrier. Its former junction directory and composition brief are removed; production, standalone generation, eval, and fixture tools import the authoritative owners directly.

Scout is fully migrated. `src/studio/scout/` owns its brief, prepared `Subject`/`Assignment`, measurement shaping, prompt, parser/guards, no-stats and unchanged outcomes, deterministic product assembly, and model session. It imports no concrete database, queue, application, or model-host adapter. `src/application/scout.rs` owns the durable entity request, PostgreSQL evidence and memory preparation, model selection, debounce, triggers, standalone/queue persistence, exact-claim completion, and ledger diagnostics. Production, batch, eval, fixture, and inspection callers use those owners; the Scout junction and duplicate composition brief are removed.

Journalist is fully migrated. `src/studio/journalist/` owns its brief, prepared `Subject`/`Assignment`, prompt, tolerant parser, citation grounding, deterministic impact/source metadata, and called or uncalled edition assembly. It imports no concrete database, queue, application, or junction adapter. `src/application/journalist.rs` owns packet/PostgreSQL retrieval, memory preparation, model selection, debounce, queue claims, storyline progression, atomic multi-row/marker publication, and diagnostics. Production, eval, fixture, Editor, and Insider callers use the new owners; the Journalist junction and duplicate composition brief are removed.

Oracle is fully migrated. `src/studio/oracle/` owns its brief, typed five-card `Assignment`, spread readiness, deterministic input identity, divergence/convergence and omen calculations, prompt, parser/guards, explicit empty marker, and model session. `src/application/oracle.rs` owns PostgreSQL pillar retrieval, the existing barrier policy, memory preparation, routing, debounce, queue ownership, claim-aware `sigil_synthesis` publication, and diagnostics. Oracle has no downstream obligation, so publication and exact completion share one short transaction without a new outbox kind. Production, evaluation, fixture, Analyst, Insider, worker, and outbox callers use the new owners; the Oracle junction and duplicate composition brief are removed.

Shared model and generation contracts now live in Studio. `runtime/harness.rs` re-exports them and delegates extraction for the four unmigrated seats. That database-bearing context will retire as their adapters move; Studio is not a wrapper around a permanent older harness.

The current worker, queue, SQL analytics, and product tables continue to operate. Migration 256 adds a unique claim token and captured running revision to `pipeline_work`; `runtime/work.rs` requires both on complete, fail, defer, and release. Migrations 257–260 add the narrow `application_outbox` for Influencer, Analyst, Scout, and Journalist. Oracle needs no schema change or downstream event: after inference its short transaction locks the exact claim, writes the crown or marker when needed, and deletes that claim. Unchanged Oracle inputs complete without a product. Stale executions write nothing. The outbox retries the earlier seats' database-only reconciliation, while optional cognition-ledger writes remain best-effort after product commit. Other handlers still need this publication boundary. The incorporated baseline already includes the Go DuckDB analytics boundary and cohort memory context; it is preserved.

## One assignment, end to end

```text
application: retrieve evidence + select model + choose publisher
  -> Analyst Assignment (form, mood, movement, sourced memory, window)
  -> Studio: construct prompt -> model -> parser/guards -> Generation<MomentumSummary>
  -> injected Publisher: application persists product and provenance
  -> Outcome::Published(receipt), or NoMaterial without a call
```

- `model::Inference` is the replaceable model interface; concrete Ollama/OpenAI-compatible clients and routing remain outside Studio.
- `Studio::extract` parses only the visible answer and captures the successful request, responding model, prompt, and telemetry. It preserves the existing limit of three attempts for surface violations or incomplete output. Ordinary transport/parser failures propagate without retry; the application owns durable backoff.
- `Parser<T>` returns a validated value, explicit abstention, or an error. Each character defines what abstention means; never invent a successful product from missing evidence.
- `Generation<T>` carries the product, provenance, and optional call diagnostics. It also represents deterministic products that did not need a call.
- `Publisher<T>` is an injected publication capability. The adapter owns storage, idempotency, and durability. Its receipt is application-defined.
- `influencer::create` returns either a called score or an uncalled NULL marker; `influencer::run` publishes either product. It never returns Analyst’s `NoMaterial`. The application skips unchanged material before rendering/calling, offers Momentum after skips and successful publication, and propagates failures. A prior real score permits one closing quiet read; the latest-row/prior-memory disagreement can bypass debounce.
- `analyst::create` can produce a validated result without publishing it, useful for evaluation. `analyst::run` completes creation and publication.

No database pool, queue item, router, or sibling character product is required by the Analyst core. Prepared `Form`, `Mood`, and `Snapshot` values carry only its evidence. Today's Postgres adapter maps older Oracle types to those values and supplies the existing sourced-memory rendering and fingerprint. Later analytical results can enter through the same preparation boundary.

Add capabilities only for actual assignments. A character that needs retrieval during creation should receive a narrow typed interface, with its implementation in the application. Do not add a generic tool registry, agent framework, or database handle merely for future flexibility.

## Character expression

[`src/studio/form.rs`](src/studio/form.rs) owns shared form and parser-compatible output contracts. `composition::form` and `composition::guards` are compatibility exports. Migrated character briefs live beside their Studio creation code; Analyst retains a temporary composition re-export, while Influencer, Scout, Journalist, and Oracle have no duplicate brief. Other briefs remain in `composition/characters/` until their vertical migrations. Memory loading and rendering remain in `composition/memories`.

Form is the canvas, character is the brush, and memories are the paint. The model
creates the reading. All six writer paths now load shared memories before their
material hash gate. Source changes can refresh a reading; prior generated prose
cannot trigger itself. `composition::compose_card` supports standalone probes;
the live input adapters combine the same memory rendering with their current evidence.
See [the memories design and real packages](fixtures/memories/README.md).

Use this question when adding or reviewing any prompt, input instruction, guard, or
evaluation rule:

> Does this rule protect the evidence or help the character express it—or does it choose the wording for them?

Keep guardrails for evidence, identity, attribution, uncertainty, and the shared output
form. Let the model choose the expression.

| Character | What it reads and expresses |
|---|---|
| Scout | The entity right now: current metrics, their supported trajectory, injuries, and personnel changes. |
| Analyst | A synthesis of Rating and Vibe trajectories. |
| Journalist | Factual reporting on the developing stories around the entity. |
| Influencer | Emotional charge and its trajectory. The stories supply evidence for the feeling. |
| Insider | An expert reading of transfer news, informed by the available history and emerging source-reliability evidence. |
| Oracle | A mystic snapshot of the entity, using the other five cards as evidence for its claims. |

Scout and Influencer own their respective trajectories, interpreting current evidence
through their supplied memories. Analyst provides a concise synthesis of those two
readings. The nuance belongs with the pillars that understand the evidence.

Journalist and Influencer have distinct subjects. Reporting tone alone is not evidence
of how a crowd feels. Memories provide continuity, not fresh measurements or proof.
The Editor, Investigator, and Graph junctions extract and verify evidence for this work.
The Insider's extraction contracts live in `insider/verification.rs`.

Current input limits: Scout receives per-skill season comparisons and an overall recent
performance trend, not recent slopes for every skill. Vibe receives current story
material, its prior read and score, and relational memory. Source-reliability history
is used by transfer verification; the Insider's wrap receives the resulting board
and prior wraps. Oracle currently receives that transfer board rather than the
Insider's finished wrap. These limits must remain distinct from the intended character
scopes; a prompt cannot supply missing evidence.

Automated checks cover mechanical contracts; review live outputs for grounded claims
and character expression. Keyword bans cannot establish whether an interpretation
follows the evidence.

## Source map

| Path | Responsibility |
|---|---|
| `src/studio/mod.rs`, `src/studio/session.rs` | Model session, bounded correction, injected publication, and outcome. |
| `src/studio/model.rs` | Model interface, call options/results, provider-independent incomplete-output signal. |
| `src/studio/generation.rs` | Typed products, parser interface, provenance, call diagnostics. |
| `src/studio/analyst/`, `src/studio/influencer/`, `src/studio/scout/` | Character creation and service-free tests. |
| `src/application/analyst.rs`, `influencer.rs`, `scout.rs`, `journalist.rs`, `oracle.rs` | Concrete preparation, routing, lifecycle policy, work coordination, and claim-aware publication. |
| `src/studio/form.rs`, `src/studio/guards.rs` | Shared character form, output contracts, and mechanical guards. |
| `src/composition/` | Existing sourced-memory packages and character briefs awaiting migration. |
| `src/junctions/` | Four remaining model-facing seats and transitional application adapters. |
| `src/runtime/route.rs`, `src/runtime/providers/` | Role selection, host concurrency, and model transports. |
| `src/main.rs`, `src/runtime/worker.rs`, `src/runtime/work.rs` | Service composition, dispatch, fenced `pipeline_work` claims, and acknowledgement lifecycle. |
| `src/runtime/harness.rs`, `src/evidence/corpus.rs`, `src/runtime/ledger.rs` | Legacy application context, data retrieval, publication diagnostics. |
| `src/runtime/util.rs` | Pure value helpers shared during migration. |
| `src/evaluation/tasks.rs`, `src/bin/eval.rs`, `fixtures/` | Existing character evaluation system and frozen material. |

`Stage::Rating` is the current queued Scout stage (`rating`); `peak` is older terminology. `statcommentary` nightly mode enqueues current-season rating work; historical backfill can still generate inline. Check `src/runtime/work.rs` and `src/main.rs` for the actual stage roster and registration. Current registration order helps prioritize dependencies but is not proof that the inputs are fresh; revision-aware scheduling is part of the approved migration.

## Building a character assignment

1. Define its material and output using domain values. Keep source provenance and missingness explicit.
2. Prepare evidence in the application. Use bounded reads or versioned study results; keep measurement distinct from prior model interpretation.
3. Keep character prompt, input rendering, parser, and deterministic product assembly together in Studio.
4. Inject the selected model and any necessary capability. Keep retries, leases, credentials, and storage in adapters.
5. Validate before publication. Optional bad titles may degrade to absent according to the character contract; invalid truth must not become a valid row.
6. Verify empty, partial, failure, and successful cases. Compare the existing contract before changing evidence richness or prompt meaning.
7. Update implementation status here, in the backend README, and in the wiki data-flow map.

Preserve public products and prompt/hash behavior while extracting a character. A later richer-study change should explicitly version changed evidence semantics and evaluate grounded output quality. Do not preserve legacy internals merely because they are old.

## Verification

From this directory:

```bash
cargo test --lib
cargo test --lib studio::analyst
cargo check --all-targets
cargo build --bin scoracle-cognition --bin statcommentary
```

These mechanical checks do not establish live model quality or database durability. Use the existing eval binary and fixtures for model/prompt changes. `eval --task momentum --fixtures --live-system` selects the current system prompt rather than the system text frozen in a fixture; it makes model calls and needs a configured backend. Measure a baseline before declaring a regression.

Claim/publication-fencing integration tests use the production SQL and are ignored unless explicitly run with an isolated, migrated `TEST_DATABASE_URL` (`cargo test --lib postgres_ -- --ignored --test-threads=1`). Ordinary library tests compile them and exercise the no-database contracts. Apply migrations 256–260, then drain/stop every older status-only worker before starting the token-aware worker. Migration 257 must exist before the new worker starts because every tick drains its outbox; migrations 258–260 must exist before the claim-aware Analyst, Scout, and Journalist record their completion kinds. Migration 256 is additive and its trigger keeps older statements schema-compatible, but ownership begins only after older workers exit because they do not present claim tokens.

The model route is independent of the character. `COGNITION_ROUTE_<ROLE>` selects the backend/model; per-host governors bound concurrency. `VOICE_NUM_CTX` resolves the shared voice window (default 4096); Analyst reserves 700 output tokens in either window, independent of the 1,200-character body and 140-character hook ceilings. Influencer keeps 700 tokens for windows up to 4096 and 800 for larger windows; eval retains its existing 800-token reservation with `num_ctx=0`. Keep options consistent with the resident model's resource budget.

## Runtime and operations

The existing executable remains `scoracle-cognition`; the existing service remains `scoracle-cognition.service`. No `studio` process needs to be launched separately. Environment parsing lives in `src/runtime/config.rs`; use `DATABASE_PRIVATE_URL` or `DATABASE_URL` plus the configured model routes. Do not infer the active host/model from a dated README measurement.

Use [`../scripts/hosting/release.sh`](../scripts/hosting/release.sh) and the [runbook](../run_docs/RUNBOOK.md) for release and rollback. `statcommentary`, `factsweep`, and eval retain their current roles. Offline probes must not claim live queue work unless explicitly designed to do so.

Plans and progress live in `../../scoracle-wiki/progress_docs/scoracle-backend/`. The [modernization plan](../../scoracle-wiki/progress_docs/scoracle-backend/2026-09-19_backend-modernization-plan.md) records the remaining character migration, queue ownership, DuckDB, richer-study, and retirement gates.

Current local acceptance: 496 ordinary Rust library tests pass with 25 isolated-database cases ignored; all 25 pass against a fresh disposable PostgreSQL 17 database loaded from the current schema through migration 260. Oracle adds service-free prepared creation, explicit empty-spread behavior, model-failure propagation, exact revision/reclaim fencing, crown and marker publication, product-free debounce, and pending/retryable/terminal readiness cases. All targets compile, Clippy passes with warnings denied, and the Go suite plus `go vet` pass. Existing prompt/evaluation contracts remain unchanged. These checks do not establish live deployment or model-quality acceptance.
