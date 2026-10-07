# Client-focused card building session — October 7, 2026

Status: Influencer period-card slice implemented; production deployment in progress on October 7, 2026.

## Implementation and rollout — October 7

The user confirmed emotional valence: 0 strongly distressing, 25 troubled, 50 genuinely mixed/balanced, 75 hopeful, 100 strongly joyful. Unknown emotion produces no card.

Implemented accepted SQL evidence → existing DuckDB selection → one reporting-week packet → one resident SmolLM3 call → atomic historical Vibe card publication. Migration 290 stores reporting bounds, cutoff, scoring version, frozen source references and durable generation attempts. The dedicated Vibe endpoint serves coherent current cards and explicit `season`/`week` history; `heat` remains the existing frontend compatibility field alongside `score`.

Focused Rust contract and PostgreSQL/DuckDB publication/idempotency checks, Go historical/current API checks and compilation passed. No broad test sweep was run. The retained single-call SmolLM3 replay passed structure but invented facts. The user explicitly authorized experimental production deployment despite that fidelity failure; this authorization does not change the evaluation result.

Deployment uses the existing Archbox release script, applies migration 290 before restart and enables the Editor Laya source-relevance stage. The Mac worker remains paused. Live release identity and verification results will be recorded here after deployment.

Remaining work: real-source and held-out fidelity review, measured Laya calibration, historical frontend controls, and migration of the remaining products, including Momentum/Oracle dependencies on generated prose. This release implements the first vertical slice, not the complete all-product target.

## Desired outcome

The consumer receives a card with a 0–100 score, a tweet-friendly headline and a body telling the ongoing story in neat, concise paragraphs. Each product has a dedicated API endpoint. The frontend eagerly loads the cards; opening a page does not trigger inference.

The backend serves saved historical outputs, including outputs from previous reporting periods. Resident SmolLM3 generates each reading from a clear context packet assembled from SQL evidence and DuckDB analysis, with basic instructions on voice and structure. Daily RSS acquisition populates the evidence tables, with entity relevance vetted by a cheap, accurately calibrated Laya System 1 process.

**Rust provides the collection; the AI provides the discovery.** Code collects and measures evidence. The model reasons across it, discovers what it supports and expresses that reading in the product's voice. Earlier LLM summaries are not a required input stage.

This target governs the new build where dated implementation documents describe conflicting behavior. Preserve useful existing infrastructure and historical records; remove superseded executable paths after their replacements work.

## Target flow

```text
Daily RSS sweep
  → acquire and retain publisher reporting
  → Laya entity-relevance scoring
  → accepted evidence in SQL
  → Rust collection + DuckDB measurements and selection
  → one packet per entity/product/reporting period
  → resident SmolLM3 discovers and writes the card
  → validate and append the complete output
  → dedicated current/historical product APIs
  → frontend eagerly loads cards
```

The unit of generation is an entity/product packet, not an individual article. Collect the period's relevant reporting together. Coalesce incoming work and avoid regenerating unchanged material. One normal generation call produces one complete reading; operational retries remain bounded and visible.

## Two contracts to establish first

### Context packet

Every product presents the same understandable outline, with product-specific evidence and instructions:

```text
TARGET
Entity identity, applicable metadata and reporting period.

RELEVANT HISTORY
Dated, attributed reporting and applicable measured history.

FRESH EVIDENCE
New source reporting and measurements, preserving speakers,
quotations, qualifications, uncertainty and comparison limits.

OUTPUT
This product's 0–100 score definition.
One headline, at most 140 characters.
A body of concise, complete paragraphs separated by blank lines.

CHARACTER
The perspective and voice used to discover and express the story.
```

`prompt.rs` is the readable entry point for collection, rendering and model instructions. It explains what each part contains and how to use it. Keep substantive SQL, parsing and publication helpers where they already make the code easier to follow; delete forwarding layers and duplicated instructions. Production and replay must use the same preparation and request path.

The model reads evidence, not queue bookkeeping or delivery policies. Keep source IDs, exact packet/request bytes and full receipts available outside the compact presentation. Trace every supplied report to stored publisher text. Source reporting is evidence of what was reported, not automatic proof that its claims are true.

A compact packet must not lose a late denial or qualification. Start with bounded, complete source material; record unavailable or oversized material explicitly. Do not introduce an LLM summarizer to make packets smaller. Inspect actual packet sizes and retained evidence before changing the budget or selection policy.

### Published card

Each published card has:

- A score within 0–100, on a named product-specific scale.
- A headline within the existing 140-character limit.
- A body with concise, complete paragraphs.
- Entity/product identity, reporting window and generation timestamp.
- Source references and the packet/input fingerprint.
- Model, prompt and scoring versions sufficient to reproduce the generation attempt.

Reuse existing append-only output tables where they fit. A common client contract does not require an immediate migration of every product into one table. Save score and prose as one coherent product; never combine the latest prose with a score from another generation.

Define the score's meaning before implementation. Preserve established, useful measured scores and their authoritative calculations. For interpretive scores, give SmolLM3 explicit anchors. Do not invent a composite or present the relevance probability as the consumer's score. Audit existing 1–100 constraints and downstream readers before enabling zero.

Influencer's character voices the evidence's place on an emotional spectrum, including intensity, ambiguity, differences between speakers and mixed emotions. Avoid good/bad/neutral categories. A single score summarizes a defined axis; the prose expresses the fuller spectrum. Emotional valence is the confirmed and implemented axis, using the anchors in the rollout section above. Unknown feelings remain unknown; missing evidence is not a score of 50.

Insufficient evidence produces an explicit absence, not a fabricated card. Failed generation must not overwrite a valid historical output or falsely mark pending evidence as successfully consumed. Current reads may retain the latest valid card with its actual date; period-specific reads must accurately show that period's availability.

## Responsibilities and reuse

| Component | Owns |
| --- | --- |
| RSS acquisition | Discovery, publisher acquisition, canonical identities, source text and acquisition dates. Fetch reusable article text once. |
| Laya System 1 | Entity-relevance estimates bound to the exact input and checkpoint. It does not write summaries or determine the emotional reading. |
| Postgres | Durable evidence, historical products and work/publication state. |
| DuckDB | Scoped ranking, known-copy deduplication, source coverage, comparable measurements and trajectories. Frequency is not independent confirmation. |
| Rust/plugin | Evidence scope, packet assembly, model invocation, structural validation and atomic publication. |
| SmolLM3 | Reasoning, discovery and expression in the requested character. |
| Product API | Reading saved outputs for current and historical scopes. |
| Frontend | Eager endpoint loading and rendering scores, headlines, paragraphs, dates and absence states. |

Reuse the existing queue, claim fencing, transactional publication, provider integration, DuckDB runner and dedicated API routes. Simplify editorial dependencies rather than replacing infrastructure that already performs necessary work.

Laya calibration is a prerequisite for claiming production accuracy, not an assumption. Current thresholds are provisional. Evaluate the complete deployed decision process, including any window aggregation. A maximum over window scores must not be called a calibrated article probability without validation. Preserve complete-input coverage; one logical probability process does not require sending an oversized article in one physical request. Retain additional routing stages only where measured accuracy or cost justifies them.

Product-specific selection must not require an earlier model's emotional verdict or finished story. Existing source indexes can be useful retrieval aids, but the new path must work without upstream generated summaries. Momentum and Oracle should receive relevant source evidence and measured inputs for their own discovery, rather than treating earlier generated prose as their source of truth.

## Exploratory repositories: reuse before invention

Inspect these implementations early. They offer useful patterns; none has been demonstrated to solve our SmolLM3 fidelity failures. Prefer an existing compatible solution when it removes more work than it adds. Record the exact file/commit and the reuse decision. Do not combine several frameworks or replace the current harness merely to resemble an example.

| Repository and entry point | Exploration question |
| --- | --- |
| [Rig](https://github.com/0xPlaygrounds/rig), especially [Rust agent builder](https://github.com/0xPlaygrounds/rig/blob/main/crates/rig-agent/src/agent/builder.rs) | Can its separation of system preamble, supplied context and model transport simplify our existing Rust call path? Check for a compatible small component before writing one. |
| [Haystack](https://github.com/deepset-ai/haystack), especially [PromptBuilder](https://github.com/deepset-ai/haystack/blob/main/haystack/components/builders/prompt_builder.py) | How does its explicit document collection → rendering → generation flow make the complete model input inspectable? Borrow the pattern where our existing functions suffice. |
| [Semantic Kernel](https://github.com/microsoft/semantic-kernel), especially [directory-based prompt plugins](https://github.com/microsoft/semantic-kernel/blob/main/python/samples/concepts/plugins/plugins_from_dir.py) | How does a plugin own a task template and accept explicit inputs without scattering instructions through its runtime? |
| [LlamaIndex](https://github.com/run-llama/llama_index), especially [default context prompts](https://github.com/run-llama/llama_index/blob/main/llama-index-core/llama_index/core/prompts/default_prompts.py) | Which simple context boundaries and task placement are useful for direct evidence reading? Test an applicable pattern on our replay cases rather than assuming it cures fabrication. |

Exploration should end with a short reuse decision and a working example when adoption is justified, then proceed to the first complete product path. Useful primary references: [DuckDB window functions](https://duckdb.org/docs/current/sql/functions/window_functions) and [probability calibration](https://scikit-learn.org/stable/modules/calibration.html).

## Implementation sequence

### 1. Establish the actual starting point

Read the current architecture contract and trace acquisition → relevance → evidence selection → generation → publication → API. Inspect every caller before removing a path. The workspace contains substantial existing uncommitted work; distinguish it from changes made in the fresh session and preserve unrelated edits.

Settle the first product's score anchors, reporting-period boundaries and historical API selection semantics using existing product definitions and the agreed consumer experience. Resolve material ambiguity with the user while continuing independent inspection. Do not turn this into a framework redesign.

Capture an inspectable baseline packet, full request, raw response and saved/API result. Existing failures are regression evidence, not a working acceptance baseline.

### 2. Establish source admission and direct collection

Reuse publisher acquisition and immutable source receipts. Make accepted entity/source evidence queryable independently of generated narrative summaries. Obtain reviewed real relevance labels, fit any required calibration mapping separately from threshold selection, and evaluate on held-out sources and time periods. Track precision, recall, probability reliability, coverage and scoring cost. Do not declare calibration complete from synthetic positives or a tuned development set.

Collect Influencer's fresh reporting and historical candidates directly by accepted entity association and dates. Use the existing DuckDB runner for bounded selection and applicable findings. Preserve competing reports and speakers. Freeze the selected evidence for each attempt, including the information actually available at the cutoff.

### 3. Complete the Influencer slice

Replace article-by-article generation with one coherent entity/period packet. Place the context explanation, task, score anchors, output structure and emotional character together in `prompt.rs`. Restore the score to the complete card contract and wire parser/schema/storage consistently.

Exercise the resident SmolLM3 path with the actual production template, thinking setting, schema and generation limits. Keep exact request bytes and raw responses for failed attempts too. Tune against evidence and reader quality; avoid adding a new instruction for every failed fixture.

Publish score/headline/body atomically with all selected source references. Preserve claim fencing, supersession checks and idempotency. Mark all covered work consistently; defer evidence excluded by a budget rather than silently consuming it. Fingerprints must cover the selected evidence and relevant contract versions.

### 4. Complete current and historical delivery

Update the dedicated Vibe endpoint to serve the coherent card and requested historical scope. Retain the existing cache/coalescing behavior where appropriate. Test an entity whose current and previous periods differ, including an empty period.

Distinguish publication time from the evidence window. Historical reproduction must exclude information acquired later, even if its publisher date is earlier. A later correction creates a new revision/output; it does not silently rewrite what was served before. Use the existing reporting calendar rather than introducing another week calculation.

Verify the consuming frontend eagerly requests each product endpoint and renders the agreed shape. Locate its implementation in the fresh session; do not assume backend route existence proves frontend behavior. Use a compatible API transition where existing consumers need it.

### 5. Expand and delete superseded paths

After Influencer passes end to end, apply the packet/card contract to the remaining products. Preserve useful product-specific evidence and measured score semantics. Replace generated-prose dependencies in Momentum/Oracle with appropriate evidence and measured findings. Remove obsolete prompt variants, forwarding layers and old executable paths with their callers. Preserve historical outputs and replay evidence.

Compare generation count, input/output tokens, Laya cost, elapsed time and factual quality against the baseline. Complete the deployment and operational checks required by the repository; local test success alone is not production cutover.

## Acceptance checks

| Area | Required evidence |
| --- | --- |
| Collection | Source-bound packet with correct entity, dates, attribution, qualifications and deduplication; no required upstream LLM summary. |
| Relevance | Held-out real cases demonstrate the chosen admission tradeoff and probability reliability; unrelated reporting is not silently routed as relevant. |
| Discovery | Human-reviewed replays show useful supported connections and emotional nuance without invented speakers, events, feelings or chronology. |
| Structure | Score in 0–100, headline within 140 characters, concise complete body paragraphs; score and prose describe the same reading. |
| Sparse/conflicting data | Unknowns and differences remain explicit; no forced neutral score, fabricated recovery or manufactured consensus. |
| Persistence | Atomic complete output, reproducible packet/request, retry idempotency and supersession/source-change behavior. |
| History | Current and previous periods select the correct saved output; no later-knowledge leakage or backdated generation. |
| API/frontend | Dedicated endpoints serve saved cards; eager loading does not cause inference; absence and actual freshness render correctly. |
| Efficiency | Normal work coalesces to one generation per changed entity/product packet; measure costs before adding caching or concurrency layers. |

Use real publisher cases alongside focused synthetic controls: positive and negative reporting, mixed emotions, multiple speakers, denials/corrections, quiet periods, missing history, unrelated entities, duplicate reporting, late qualifications and source changes. Parser acceptance checks shape, not truth. Run focused Rust checks, necessary isolated database/API tests and actual model replays; manually review fidelity. Freeze a reviewed evaluation set so each new prompt is not judged only on cases used to tune it.

## Starting map for the fresh session

Paths below are relative to the `rust/` workspace unless prefixed with `../`:

- `README.md`: current collection/discovery contract and source/memory boundaries.
- `src/plugins/influencer/{prompt,mod,memories,parser,publish}.rs`: first product path. Current generation is per article, score is absent, and memory lookup depends on story-index membership.
- `src/plugins/harvester/{adapter,cognition,context,delivery,policy}.rs` and `src/plugins/editor/`: current acquisition, relevance and routing paths. Checked-in changes are not proof of deployed behavior.
- `src/tools/memories.rs`, `src/tools/memories/postgres.rs`, `src/tools/memories/reporting.sql`, `src/tools/reader.rs`: existing source collection, analysis bridge and full-text reader.
- `../go/internal/analytics/duckdb/`: existing reporting, score-history and measurement studies.
- `src/plugins/analyst/prompt.rs`, `src/plugins/oracle/prompt.rs`: current generated-card dependencies to replace after the pilot.
- `../go/internal/db/vibe.go`, `../go/internal/api/handler/data.go`, `../go/internal/api/server.go`: current/historical API behavior. Current Vibe history excludes unscored rows.
- `../sql/schema/schema.sql` and `../sql/migrations/`: durable tables, score constraints, period stamps and publication/work dependencies. Follow the repository's migration process.
- `examples/influencer_replay.rs`: existing production-path model replay entry point.
- `fixtures/influencer/direct-context-v18-review.json` and `fixtures/influencer/presentation-investigation-review.json`: baseline failures and legacy/model/layout controls. The 25-case replay had 20 structurally valid cards, five truncated outputs and no correct abstentions on three expected-abstention cases; structural success did not establish fidelity.
- `docs/editor-system-one-build-2026-10-06.md`: source-reader changes, calibration failures and deployment limitations.

## Fresh-session instruction

Read this plan and inspect the current workspace before editing. Build the complete Influencer path first, from accepted source evidence through the historical API. Explore the linked repositories for existing solutions before adding new machinery. Reuse the operational infrastructure, keep collection separate from discovery, test real model output as well as code, and delete superseded paths once the replacement is verified. This is an architecture simplification with a consumer-visible acceptance target, not another prompt-only patch.
