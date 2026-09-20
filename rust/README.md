# Scoracle Studio and Rust application

**Postgres stores the world. DuckDB studies it. Studio supplies cognition.** `src/studio/` is the single harness for all nine seats: Analyst, Influencer, Scout, Journalist, Insider, Oracle, Editor, Investigator, and Graph.

Read the [backend README](../README.md), [data-flow map](../../scoracle-wiki/DATA_FLOW.md), and [development rules](../run_docs/DEVELOPMENT.md) before changing a boundary. The [runbook](../run_docs/RUNBOOK.md) owns operational procedures.

## One assignment, end to end

```text
application: retrieve evidence + select model
  -> prepared Studio assignment
  -> prompt -> bounded model session -> parser/guards -> Generation<T>
  -> application: exact-claim transaction -> product/provenance + required effects + completion
```

Studio receives domain values and an injected `model::Inference`. It owns prompts, parsers, product assembly, provenance, and the existing bounded corrective rewrites. Invalid output fails before publication. `Generation<T>` captures the actual successful request and responding model; uncalled markers retain explicit provenance. `Publisher<T>` remains the small injected sink used by Studio callers that need one.

Studio production modules import no application, evidence provider, queue, routing, or database code. Pure shared text/fingerprint helpers live in `src/util.rs`. Source selection and rendered memory packages live in `evidence/memories`; the application passes prepared material into Studio. Measurement and prior interpretation remain distinct.

## Application execution and durability

`main.rs` binds each application handler to its storage pool and model routes/limits. `WorkHandler::handle` accepts only the exact claimed `Item`, and returns `Completed`, `Superseded`, or `Deferred`. There is no default publication or worker completion path. The worker owns claims, concurrency, deadlines, retries, stale recovery, notifications, shutdown, and periodic maintenance; it holds no model router or cognition context.

Application helpers take the dependencies they use: SQL-only loaders, debounce queries, and outbox recovery take a pool; model work receives separate `Models` routes and limits. This small configuration bundle contains no storage, queue operations, or execution methods. Fixture acquisition needs only storage and its fetcher. Evaluation and `statcommentary` use the same explicit dependencies and Studio contracts.

After inference, each publisher locks the exact claim token and captured revision. Required product/provenance/effects and completion commit in one short transaction. Stale or reclaimed work publishes nothing. Insider retains committed partial pairs and target-specific Oracle obligations across budget deferral. Oracle is terminal; evidence seats commit their own effects directly, so none needs an artificial outbox. Optional cognition diagnostics remain best-effort after commit.

The narrow `application_outbox` retries Momentum/Oracle obligations after publication, without model routing or a model service. Replay coalesces queue work; dispatch failure durably backs off. `LISTEN/NOTIFY` wakes workers, while persisted work and obligations survive process loss. Existing host slot groups, stage caps, FIFO/priority ordering, and retry policy remain intact.

`runtime/harness.rs`, `junctions/`, and `composition/` are retired. Model/form/guard aliases and migration-only completion wrappers are gone. Unused `sql` and `multilang` routing roles are removed; Graph retains its deployed `emotional-news` route key. Active stages, public products, prompt versions, and schema are unchanged by this cleanup.

## Character expression

`studio/form.rs` owns shared form and output contracts; each seat owns its brief. `evaluation/memory.rs` supports offline memory probes using those same briefs.

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
Editor, Investigator, and Graph also create through prepared Studio assignments.
The Insider's extraction contracts live in `studio/insider/verification.rs`.

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
| `src/studio/` | Nine prepared assignments, character/evidence contracts, model session, validation, provenance, and optional injected publication. |
| `src/application/` | Concrete preparation, work policy, atomic publication, and diagnostics for each seat. |
| `src/application/worker.rs`, `outbox.rs` | Queue execution, maintenance, supervision, and durable follow-up recovery. |
| `src/application/models.rs`, `products.rs` | Separate model configuration and SQL product lookups. |
| `src/runtime/work.rs`, `stage.rs` | Exact lease operations and the bound queue-handler contract. |
| `src/runtime/route.rs`, `providers/` | Role selection, per-host concurrency governors, and model transports. |
| `src/runtime/config.rs`, `fetch.rs`, `ledger.rs`, `db.rs` | Environment, acquisition infrastructure, optional diagnostics, and pool construction. |
| `src/evidence/` | Shared retrieval, sourced-memory selection/rendering, packets, and deterministic evidence preparation. |
| `src/evaluation/`, `src/bin/eval.rs`, `fixtures/` | Offline evaluation, inspection, and frozen material. |
| `src/bin/statcommentary.rs`, `factsweep.rs` | Nightly/single/backfill Scout work and maintenance adjudication. |
| `src/util.rs` | Pure text, rounding, and fingerprint helpers. |

`statcommentary` nightly enqueues current-season Rating; single and historical backfill can generate inline. Persisted queue names and registration order remain operational contracts, not a mandatory chain through the seats.

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
cargo test --lib --bins
cargo clippy --all-targets -- -D warnings
cargo build --bin scoracle-cognition --bin statcommentary
```

These mechanical checks do not establish live model quality or database durability. Use the existing eval binary and fixtures for model/prompt changes. `eval --task momentum --fixtures --live-system` selects the current system prompt rather than the system text frozen in a fixture; it makes model calls and needs a configured backend. Measure a baseline before declaring a regression.

Claim/publication-fencing integration tests use the production SQL and are ignored unless explicitly run with an isolated, migrated `TEST_DATABASE_URL` (`cargo test --lib -- --ignored --test-threads=1`). Ordinary library tests compile them and exercise the no-database contracts. Apply migrations 256–261, then drain/stop every older status-only worker before starting the token-aware worker. Migration 257 must exist before the new worker starts because every tick drains its outbox; migrations 258–261 must exist before the claim-aware Analyst, Scout, Journalist, and Insider record their completion or fanout kinds. Migration 256 is additive and its trigger keeps older statements schema-compatible, but ownership begins only after older workers exit because they do not present claim tokens.

The model route is independent of the character. `COGNITION_ROUTE_<ROLE>` selects the backend/model; per-host governors bound concurrency. `VOICE_NUM_CTX` resolves the shared voice window (default 4096); Analyst reserves 700 output tokens in either window, independent of the 1,200-character body and 140-character hook ceilings. Influencer keeps 700 tokens for windows up to 4096 and 800 for larger windows; eval retains its existing 800-token reservation with `num_ctx=0`. Keep options consistent with the resident model's resource budget.

## Runtime and operations

The existing executable remains `scoracle-cognition`; the existing service remains `scoracle-cognition.service`. No `studio` process needs to be launched separately. Environment parsing lives in `src/runtime/config.rs`; use `DATABASE_PRIVATE_URL` or `DATABASE_URL` plus the configured model routes. Do not infer the active host/model from a dated README measurement.

Use [`../scripts/hosting/release.sh`](../scripts/hosting/release.sh) and the [runbook](../run_docs/RUNBOOK.md) for release and rollback. `statcommentary`, `factsweep`, and eval retain their current roles. Offline probes must not claim live queue work unless explicitly designed to do so.

Plans and progress live in `../../scoracle-wiki/progress_docs/scoracle-backend/`. The [modernization plan](../../scoracle-wiki/progress_docs/scoracle-backend/2026-09-19_backend-modernization-plan.md) records the remaining character migration, queue ownership, DuckDB, richer-study, and retirement gates.

Current local acceptance: 499 ordinary Rust library tests pass with 29 isolated-database cases ignored; all 29 pass against a fresh disposable PostgreSQL 17 database loaded from the current schema through migration 261. Insider adds service-free pair and wrap creation, explicit UNKNOWN behavior on pair transport failure, exact revision/reclaim fencing, cleared-pair progress, and per-target durable Oracle fanout. All targets compile, Clippy passes with warnings denied, and the Go suite plus `go vet` pass. Existing prompt/evaluation contracts remain unchanged. These checks do not establish live deployment or model-quality acceptance.

### Production-shaped recovery rehearsal

The opt-in isolated database suite now includes real child-process exit before/after publication and after dispatch, plus Insider partial-progress reconnect and the real worker safety tick with no listener. It uses fake creation outputs and makes no model calls. See [acceptance operations](../run_docs/RECOVERY_ANALYTICS_ACCEPTANCE.md) for scope, canary and mixed-version rollback gates. Passing synthetic recovery does not establish live deployment or recovery.
