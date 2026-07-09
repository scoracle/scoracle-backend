# Rust Cognition Context Enrichment Plan

Created: 2026-06-29
Status: Phase 1 completed on 2026-06-30 in commit `ff258a1`; execution summary posted in `planning_docs/RUST_COGNITION_PHASE1_DURABLE_SPINE_EXECUTION.md` via commit `2d9cec4`.

Priority follow-on: `planning_docs/MULTI_LENS_COGNITION_PANEL_PLAN.md` names the product direction
this enrichment work should serve. Context ledgers, role routing, and evals should mature toward
stats, narrative, and transfer lenses that synthesize into the final Scoracle read.

## Core Concern

The Rust cognition layer may be drifting toward controlling the models instead of empowering
them. The original concept is stronger than a byte-parity port: Rust should prepare richer,
cleaner, better-scoped context so the model can produce more valuable judgments.

The current layer has good foundations: durable queueing, typed fail-closed outputs,
role-based model routing, provenance, parity harnesses, and CPU-side embeddings. The risk is
that the system treats parity and rigid prompt reproduction as the destination. That can choke
model value when the model needs better context, not narrower instructions.

The revised north star:

> Rust enriches, selects, grounds, compresses, and proves context. The model reasons over that
> context. Postgres records what happened. Go serves the finished product.

## Operating Principles

1. Context beats control.
   The model should receive the best available evidence, organized for the job. Do not solve
   quality problems by adding more prohibitions when the real issue is weak context.

2. Determinism belongs around the model, not inside the model.
   Rust should make input selection, provenance, dedupe, hashes, retries, and persistence
   deterministic. The model output can remain generative where that creates user value.

3. Fail closed only where false positives harm trust.
   Unknown transfer rumors and same-name entity matches should fail closed. Narrative phrasing
   and synthesis should not be smothered by excessive parser rigidity beyond the product contract.

4. Parity is a migration tool, not a product strategy.
   Keep parity gates for ports and regression checks. Once a stage is Rust-owned, optimize for
   measured quality, latency, durability, and user value.

5. The model router is a value engine.
   Roles should earn model choices through evals. A model swap is not just config plumbing; it
   must be backed by stage-specific quality tests.

## Diagnosis

The current layer has two competing personalities:

- Good: typed stage outputs, provenance, route seams, SQL-backed context loading, queue recovery,
  and embedding-backed disambiguation.
- Risky: byte-parity comments everywhere, stage prompts treated as frozen machinery, weak
  stage-specific evals, append-only outputs without enough freshness semantics, and some handoffs
  that are less durable than the architecture claims.

The likely effect is that models are often asked to fit an old Go-era prompt shape instead of
being given a deliberately enriched working set.

## Execution Plan

### Phase 1 - Stabilize the Durable Spine

Goal: make sure model value is not lost through orchestration gaps.

- Make `vibe -> sigil` enqueue failure retryable instead of warn-and-complete.
- Decide whether transfer pair DB/persist errors should fail the team item. Default answer:
  yes, infrastructure errors should retry; model non-commitment can remain an UNKNOWN row.
- Widen `work::Item.entity_id` and related binds to `i64`, or explicitly split article-keyed
  work from player/team work. This removes the scrub article-id ceiling.
- Add strict config parsing. Invalid numeric env values should fail boot with a clear error.
- Run `cargo fmt` and make `cargo fmt --check` part of the release gate.

Success criteria:

- No completed upstream work can silently drop a required downstream derivation.
- Bad config fails loudly.
- Formatting, tests, and clippy all pass.

### Phase 2 - Separate Parity Mode From Product Mode

Goal: stop letting migration constraints define product behavior.

- Tag each stage as one of:
  - `parity-bound`: still proving a port.
  - `rust-owned`: allowed to improve context and prompt shape.
  - `experimental`: shadow/eval only.
- Update comments and README language so parity is described as a regression tool, not the
  permanent definition of success.
- For rust-owned stages, create explicit product contracts:
  - inputs selected
  - context budget
  - required output fields
  - fail-closed conditions
  - freshness/debounce behavior
  - eval dataset

Success criteria:

- A future contributor can tell when byte-identical behavior matters and when quality improvement
  is expected.

### Phase 3 - Build a Context Ledger Per Stage

Goal: make context enrichment explicit, inspectable, and testable.

For every model call, define a `ContextLedger` concept, even if implemented per stage first:

- source rows read
- entity identity card
- recency window
- dedupe decisions
- transfer heat facts
- excluded candidates and why
- final prompt sections
- input hash

Start with narratives and transfers because they benefit most.

Planned shape:

```text
ContextLedger
  entity
  role
  source_ids
  included_evidence
  excluded_evidence
  deterministic_signals
  model_prompt
  request_body
  input_hash
```

Do not create a generic abstraction too early. First implement the ledger fields where they
immediately improve inspection and evals.

Success criteria:

- When a model output is bad, we can answer whether the model failed or the context failed.

### Phase 4 - Enrich Context Before Prompting

Goal: use Rust for real cognition support, not just orchestration.

Priority enrichments:

1. Entity identity expansion
   - Current club/team
   - position/role
   - league/sport
   - recent team membership
   - known aliases if available

2. Evidence ranking
   - source credibility
   - recency
   - article uniqueness
   - entity centrality
   - title proximity
   - corroboration count

3. Evidence compression
   - cluster near-duplicates
   - preserve the strongest representative
   - summarize repeated evidence deterministically where possible

4. Contradiction flags
   - current club mismatch
   - wrong sport/league
   - opponent/rival framing
   - historical/background mention
   - roundup/listicle weakness

5. Context sectioning
   Prompts should be structured as evidence packets:
   - identity
   - confirmed facts
   - candidate evidence
   - weak/noisy evidence
   - task
   - output contract

Success criteria:

- Fewer prompt prohibitions are needed because the context itself makes the right answer easier.

### Phase 5 - Stage-Specific Quality Evals

Goal: promote models and prompt/context changes by measured value.

Create small curated eval sets in `rust/` or a DB-backed eval table for:

- Scrub same-name disambiguation
- Transfer false positives and true positives
- Narratives grouping and grounding
- Vibe felt-read quality
- Rating statistical identity
- Sigil synthesis quality
- Headlines duplication and importance

Each eval case should record:

- entity
- source article ids or fixture/stat ids
- expected behavior
- failure type tags
- human label or rubric

Minimum rubrics:

- groundedness
- specificity
- false-positive risk
- useful synthesis
- concision
- freshness

Success criteria:

- `COGNITION_ROUTE_<ROLE>` changes are backed by stage-specific evidence.
- Prompt/context changes can be accepted even when they intentionally break Go parity.

### Phase 6 - Fix Stage Freshness And Idempotency

Goal: append-only provenance without duplicate or stale product behavior.

Stage actions:

- Headlines:
  - add input hash over corpus ids/titles/categories prompt version
  - add no-headlines marker or latest-run record
  - dedupe repeated generated headline rows

- Vibe:
  - consider input hash over latest narratives plus heat
  - keep append-only scores if trend history is valuable, but avoid pointless no-change reruns

- Narratives:
  - persist dedupe metadata or ledger fields for inspection
  - ensure marker rows intentionally supersede old storylines

- Transfers:
  - keep latest-row read semantics
  - retry infra errors
  - track unknown rate as an operational metric

- Sigil and rating:
  - keep input-hash debounce
  - ensure comments match actual behavior

Success criteria:

- Reprocessing is safe.
- Duplicate user-facing content is controlled.
- Latest-generation semantics are obvious.

### Phase 7 - Productize Model Observability

Goal: know whether the AI layer is creating value.

Add metrics/log fields around:

- stage latency
- model latency
- prompt token proxy or prompt byte size
- output parse success
- fail-closed count
- unknown marker count
- skipped unchanged count
- context item count
- dedupe drop count
- model/version by role

Keep it simple: tracing fields first, DB summary later if needed.

Success criteria:

- We can see when a model is starved, overloaded, over-constrained, or receiving weak context.

## First Implementation Slice

Recommended first slice for a fresh context:

1. Run `cargo fmt`.
2. Make config parsing strict.
3. Make `vibe -> sigil` enqueue failure retryable.
4. Make transfer infra/persist errors fail the team item.
5. Add `COGNITION_CONTEXT_ENRICHMENT_PLAN.md` to the repo as the guiding doc.
6. Update README wording: parity is a migration/regression gate, not the long-term target.
7. Add a tiny eval fixture format and seed 5 transfer false-positive cases.

This slice improves durability immediately and starts turning the model layer toward measured
context quality.

## Non-Goals

- Do not build a giant generic context framework before two stages prove the shape.
- Do not remove fail-closed behavior from high-trust surfaces like transfer rumors.
- Do not chase parallelism before the value path is clean.
- Do not optimize for byte parity once a stage is intentionally Rust-owned.

## Decision Record

The Rust layer should not be a cage for the model. It should be the model's preparation layer:
cleaner evidence, better entity resolution, richer context packets, stricter provenance, and
measurable quality gates.

Simple is still the target. The simplest valuable system is not the one with the fewest lines;
it is the one where each line gives the model better evidence or protects the product from bad
outputs.
