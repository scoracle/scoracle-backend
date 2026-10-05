# Studio: the architecture contract

**Everything is a plugin.** The harness determines runnable work → the plugin supplies data, relationships and instructions → its cognition model performs the job → the plugin validates and publishes.

Plugins retrieve and assemble relevant DB evidence, retain provenance and compute supported measurements. They do not prewrite the interpretation or prescribe a conclusion. Cognition synthesizes the supplied world within its factual boundaries. Harvester is a peer plugin using a System 1 model, currently Laya; System 1 is not a separate architectural layer.

The [plugin alignment plan](docs/PLAN-plugin-alignment-2026-09-27.md) is the current design authority. It separates implementation, checks, product verification and deployment. Local six-part migrations for Insider, Analyst and Oracle still need database and product replay before release.

## Ownership

- **Plugins:** relevant data, explicit relationships, job and tone instructions, validation and publication policy.
- **Cognition:** faithful synthesis and expression of that supplied world; no invented evidence.
- **Harness and host:** runnable work, shared inference and tools, budgets, claim fencing and atomic publication coordination.
- **Postgres and DuckDB:** stored evidence and bounded studies with dates, coverage and provenance. Prior prose cannot become measurement evidence.

## Harvester supplies source text

Harvester replaces the legacy Editor's article-summarization role with source acquisition, System 1 filtering, and verbatim context extraction. It does not generate a summary or editorial packet for the characters.

The current checked-in worker follows this path:

1. Retain Google candidate provenance, query entity, headline, and publisher identity. Shared `plugins/meta.rs` supplies canonical name, entity ID, type and sport. The System 1 model scores explicit headline reference to that supplied identity before publisher acquisition.
2. Apply Harvester's reading policy. Fetch or reuse usable publisher text only when at least one query entity passes. Acquisition failures remain acquisition outcomes, not negative relevance judgments.
3. Select the publisher's first three available paragraphs verbatim, with body hash and UTF-8 byte offsets. Score every retained non-whitespace character through windows of at most 100 words and 1,200 bytes. Openings requiring more than eight windows fail visibly.
4. The System 1 model returns seven scalar predicate scores. Harvester aggregates support across windows and applies its route table to select Journalist, Influencer, Insider and Scout. Source evidence stays separate from subject metadata; no model-selected destination or generated summary is accepted.
5. Persist exact source context, scores, window coverage and versioned policy under the queue claim. The harness dispatches approved destinations subject to shadow mode and character enrollment. Receiving plugins own product sufficiency and articulation.

The current local contract is `harvest-context-v7`, with `harvest-headline-v3` gates. Reading uses a **0.25** threshold; theme predicates use **0.50**, except performance at **0.70**. These are provisional development policy values, not calibrated accuracy claims. See the [v7 frame and evaluation](docs/harvester-frame-2026-09-27.md) and [source boundary](docs/harvester-plugin-boundary-2026-09-27.md). Google descriptions do not substitute for publisher text. “Verbatim” refers to retained extracted text, not raw HTML.

Harvester has one worker/replay path. The historical packet compiler, choice-response parsing and teacher fields are removed. The [cleanup pass](docs/harvester-cleanup-2026-09-27.md) records other retired tools. Shared subject metadata is available to the other plugins; Harvester's accepted v7 behavior predates the six-part cleanup; the target module layout is not yet fully implemented. A [local Laya/Fastino comparison](docs/harvester-systemone-comparison-2026-10-01.md) retained Laya and the current policy pending fresh reviewed calibration; operational deployment remains separate.

Older cutover documents describe three-sentence excerpts, broad forwarding, or advisory-only relevance. Those descriptions are historical and do not describe this worker contract. Deployment reports are also dated evidence: verify the actual host revision, migrations, flags, and worker state before claiming the checked-in behavior is live. This README update does not deploy code or release character delivery.

## Character preparation and articulation

The target plugin supplies a lean task and subject, tone/output requirements, and only the database tools needed for its work. The model requests evidence through those tools; the application executes scoped reads and returns attributed results. Existing deterministic retrieval and DuckDB arithmetic remain reusable mechanisms; interpretation belongs to cognition.

**Tone without an invented character is the target.** Describe the desired qualities of the prose directly. Do not ask the model to play a person, adopt a backstory or motive, or invent a role. Product names remain identities, not acting assignments.

### Six model-facing parts

The base toolkit organizes these six responsibilities; it does not eagerly assemble all evidence into the initial request. **Each plugin uses only the parts its work needs.** These names define the target module responsibilities; shared implementations are reused rather than copied into every plugin.

| Part | Owns |
| --- | --- |
| `meta.rs` | The subject entity: canonical identity and relevant entity attributes. Identity is context, not evidence of a new event. |
| `voice.rs` | Tone and writing qualities only. No persona, backstory, task, required conclusion or invented motive. Omit when no expressive output is needed. |
| `memories.rs` | DuckDB-powered research over stored evidence: plugin-selected scope, dated history, compatible comparisons, units, coverage and provenance. |
| `fresh.rs` | New content from the daily sweep, including relevant newly available records or measurements, with source attribution, dates, qualifications and explicit unknowns. |
| `form.rs` | Output structure: fields, types, paragraph structure, dimensions, serialization and structural decoding. It does not prescribe a main finding, significance, supporting detail or story. |
| `prompt.rs` | How the supplied elements relate and what the model should do with them. One concrete task, with factual boundaries and supported partial/unknown outcomes. |

**Harvester uses only `meta.rs`, `fresh.rs`, `form.rs` and `prompt.rs`.** Its form is the bounded classification output; its prompt defines the System 1 task over the subject and fresh source evidence. It does not need voice or memories. The same six-part vocabulary applies to other plugins without requiring six files, six model calls or empty placeholder parts.

The model receives minimal initial context and instructions, then evidence through its selected tools. Rust filenames and implementation commentary are not model input. Keep evidence and writing controls distinguishable in the request. Attach relationships to the data they qualify: a correction identifies its claim, a comparison identifies its compatible measurements, and a publication date remains distinct from an event date. `prompt.rs` explains those relationships without asking the model to reconstruct missing joins or invent a story. Cognition retains synthesis and expression; the plugin does not prewrite its conclusion.

### Base toolkit and plugin selection

Build reusable tools for direct interactions with the database and source evidence. **The plugin brings only the tools it needs for its work.** Postgres provides durable records; DuckDB supplies bounded research through `memories.rs`. Reuse the existing identity, source acquisition/presentation, study, decoding and publication mechanisms. The plugin owns selection, scope, eligibility and publication policy; the host supplies the scoped capabilities, budgets and durable execution mechanics.

The six parts organize the model input; they are not the complete list of available tools. Investigator, for example, needs a `browser.rs` tool for browser-based source retrieval and inspection. Its source receipts can feed `fresh.rs` and retained evidence. Browser access does not make retrieved content verified identity evidence. Add further tools as concrete plugin work requires them; plugins do not inherit every installed tool automatically.

This is a base toolkit, built by consolidating working capabilities and filling demonstrated gaps. Use the existing plugin boundary and capability grants. No new toolkit registry, universal assembler, plugin DSL or compatibility framework is needed. Shared tools supply mechanisms; they do not inject writing policy. Model-callable tools are exposed only when the owning operation needs them; DB preparation can use ordinary functions directly.

The first read-only Influencer pilot is `cargo run --example influencer_tools -- SPORT team ID MODEL` with `DATABASE_PRIVATE_URL` or `DATABASE_URL` and optional `OLLAMA_BASE_URL`. It starts with identity and instructions, exposes only `read_source`, reuses verified Harvester delivery on invocation, and requires a validated tool call before accepting any answer. It never publishes. Native Ollama conversation transport and the shared GPU governor support this path; other backends fail explicitly until implemented. The provider adapts the installed SmolLM3 template's documented XML tool format; SmolLM3 still skipped the read in the live pilot. Granite 3B completed the live unavailable-data loop, but no eligible fresh source existed for an available-evidence test, so **end-to-end replacement remains unaccepted**. See [tool pilot status](docs/scout-closure-2026-09-29.md#september-30-read-only-archbox-pilot).

The acquisition choice is now under [matched comparison](docs/scout-closure-2026-09-29.md#september-30-acquisition-cost-and-garbled-output-diagnosis). For one mandatory source read, lean plugin preparation used fewer model calls and input tokens than native calling; neither route passed product review. The six responsibilities apply to either route. Native tool selection still needs to demonstrate a benefit from choosing or avoiding reads.

### Remove legacy instruction paths

The six-part contract replaces the accumulated instruction stack. Retire shared `support/prompt.rs::compose`, persona briefs, content demands in form/schema descriptions, generic editorial rewrite prompts, obsolete palettes and duplicate preparation/evaluation paths as their live callers migrate. Use `voice.rs` for surviving tone instead of maintaining `brief.rs`, `journalist.rs` or `influencer.rs` aliases. Delete instructions that exist only to satisfy a phrase check or snapshot; do not copy them into the new modules. A valid factual or product constraint stays with its responsible part and is verified through behavior.

Keep parsers, source integrity, error handling, claim fencing and atomic publication where needed. Parsers and adapters are implementation mechanisms, not additional model-facing parts. Each migration removes the superseded path together with its callers; the [plan's retirement ledger](docs/PLAN-plugin-alignment-2026-09-27.md#retirement-ledger) tracks remaining work. These are target requirements, not a claim that all current modules have already migrated.

Tool sessions retain their explicit request/call/result transcript within one assignment. `memories.rs` exposes relevant DuckDB research when selected by the plugin; no hidden cross-assignment conversation is assumed. Full retrieval provenance stays with the plugin. The [live memory inspection and DuckDB trial](docs/journalist-memory-world-2026-09-28.md) records existing studies and remaining integration work.

| Character | Plugin-selected material |
| --- | --- |
| Journalist / Narratives | Attributed source developments, their dates, and supported continuity. |
| Influencer / Vibe | Observed emotional evidence, its speaker/group scope, and relevant dated memories. |
| Scout / Rating | Prepared statistics, compatible comparisons, trends, sample coverage, and limitations. |
| Insider / Transfers | Sourced relationship and transaction states, uncertainty, and relevant history. |
| Analyst / Momentum | Finished Scout and Influencer findings plus supported trajectory studies. |
| Oracle / Sigil | Selected findings from the other five finished products and explicit component availability. |

The plugin constructs the bounded environment before the model call. Identity, dates, evidence scope, uncertainty, and missing-input status must survive articulation. Missing stays unknown; measured zero and observed absence are distinct findings. Direction requires a supported comparison. Compression must not erase a qualification or imply causation the evidence does not support.

[`form.rs`](src/plugins/support/form.rs) supplies structural publishing form. Local plugin alignment is closed through Graph and Editor retirement. Graph publishes source-bound nominations and review receipts; generated relations never become canonical facts. Unsupported relation, reporting-based identity and fixture extraction remain unavailable. Historical evaluation paths remain offline.

The [Journalist Window 2 completion](docs/HANDOFF-journalist-finish-2026-09-28.md) uses one natural articulation stage over a plugin-prepared observation package. Shared `form.rs` supplies keyed structure without content direction; `journalist.rs` currently contains tone only and will become `voice.rs` under the six-part contract. `journalist/cognition/prompt.rs` owns the articulation task and explains how the pieces actually present fit together. Fresh-only calls receive the compact source-to-key instruction. When studied history is present, the manual identifies `identity`, `history`, `fresh`, `voice` and `form` and locates history inside the report text it contextualizes. It does not ask the model to discover a story or significance.

Fresh source presentation lives in `journalist/cognition/fresh.rs`: fetched reporting, attribution, publication time and a request-local output key. SmolLM3 returns report text only. The plugin derives titles from complete source openings and the edition headline from the first selected title. Historical reporting remains distinct from fresh evidence and retains dated attribution, population window and publisher counts. Provenance and scoring stay outside the writing context. The n94 no-thinking replay passed manual fidelity review across fresh, conflicting, qualified, ordered, source-instruction and useful-memory cases; exact requests and outputs are retained under `fixtures/journalist/`. No generative claim-preparation or prose-judging stage was adopted. Commit `573d6a8e` was deployed on `archbox` on September 28, 2026; the API reported that revision, database health passed, and the API and cognition services plus their path watchers were active.

Scout s66 uses measured memory windows and verified publisher reports. Task instructions and voice remain plugin-owned. Product fidelity evidence and calibration remain separate from the completed local alignment migration; no change here claims deployment.

*Thinking note:* SmolLM3 reasoning is operational when a route explicitly selects `think:true`; the Ollama boundary supplies the model-specific tagged-reasoning cue and keeps the trace outside published prose. The full n94 comparison confirmed real reasoning but found it slower, less faithful and capable of exhausting its larger allowance. Journalist therefore remains `think:false`; thinking is available for targeted evaluation if a later task demonstrates a need.

Finite phrasing selection is not the definition of articulation. Evaluate whether each plugin gives SmolLM3 enough prepared context and expressive latitude for natural, concise, language consistent with the supplied tone while preserving factual fidelity. Any replacement must enforce its factual interface and demonstrate its value on representative cases.

## Shared memory contract

**Postgres stores the world. DuckDB studies the world. Each plugin chooses its data and scope; the memory contract stays the same.**

Graph and the other source producers enrich the stored relational world. At preparation time, a plugin requests a study of that world for a particular entity or pair, timeframe, and comparison scope. DuckDB computes the findings on demand. Each plugin's `memories.rs` selects and presents the relevant findings alongside fresh content, ready for articulation.

The shared contract is **request a scope → study the stored evidence → return findings with dates, coverage and provenance**:

- **The plugin owns the request:** which data matters, the historical timeframe, the comparison population and the context budget. Changing a plugin's lookback changes its request, without requiring a new precompute schedule.
- **The shared study layer owns computation:** filtering, grouping, frequency, ordering and compatible comparisons over a consistent snapshot. Results identify the study version, source records, time bounds, units where applicable, and missing or partial coverage. Source corrections and deletions must invalidate affected reuse.
- **`memories.rs` owns the context selection:** retain the findings and qualifications useful to the current assignment. Full retrieval bookkeeping stays in provenance.
- **The LLM owns articulation:** express fresh material in the context of the supplied history. Retrieval, arithmetic and deciding whether reporting is valid have already been handled upstream.

Different plugins request different studies through this boundary. Journalist can request earlier reporting; Influencer can request observed reactions; Insider can request entity-pair reporting frequency and publisher breakdowns; Scout can request xG comparisons across specified match windows. Frequency measures recorded reporting, not independent confirmation. Numerical change retains its sample and comparison basis.

Memory is shared product infrastructure. Reuse the stored world, study implementations and provenance while keeping each plugin's selection policy local. See the [cross-plugin design](docs/journalist-memory-world-2026-09-28.md#shared-across-plugins) and [on-demand implementation](docs/memory-studies.md). Reporting and team-stat studies are implemented, with deployed Journalist n94 as the first integrated consumer; the other character selectors remain to be migrated.

## Evidence, quality, and efficiency

- Compute arithmetic, scores, and comparisons upstream. Supply units, season/competition, comparison population, sample coverage, and uncertainty when they affect meaning.
- Select memory deliberately and label its provenance. A previous interpretation cannot manufacture a fact or inflate corroboration.
- Retain full provenance and debugging detail outside the articulation input. Include necessary evidence once and budget the complete request plus reserved output.
- Use the same active preparation and contract for production and evaluation. Remove superseded prompts, parsers, fixtures, flags, and compatibility paths with their obsolete callers.
- Evaluate models in this order: factual fidelity, articulation quality, voice consistency, concise synthesis, speed/efficiency, and general reasoning only where required. SmolLM3 is the selected articulation model; broad intelligence cannot compensate for an incomplete prepared world.
- Measure useful coverage, specificity, additions and omissions, voice, tokens, latency, and retries. Valid JSON or a valid palette index alone does not establish product quality.
- Preserve source integrity, claim fencing, atomic provenance/follow-up publication, idempotency, and recovery when pruning legacy code. Every retained temporary dependency needs a named consumer and removal condition.

## Source map and operations

`src/plugins/<name>/` owns each plugin's manifest, preparation, cognition where needed, publication adapter, and tests. Plugins provide tools and instructions and may share tools. `src/plugins/support/form.rs` is the shared output-form tool: shape, dimensions, decoding and structural validation. `support/source.rs` presents intact publisher reporting and detects explicit instruction overrides; Journalist and Influencer consume it. Plugins select the tools and supply content instructions, selection policy and publication guards. Canonical identity lives in `plugins/meta.rs`; the shared memory tool lives in `plugins/memories.rs`. Plugins choose reporting or match-statistic tables and the request scope; one DuckDB runner handles execution and provenance. Plugin-local memory files retain their request and presentation policy. `src/plugins/support/` also supplies guards and resource profiles. The [fleet](src/application/fleet.rs) excludes retired Editor; Harvester owns surviving source and maintenance work.

`src/studio/` holds generic inference sessions, generation envelopes, and plugin/tool contracts. `src/application/` assembles concrete dependencies and capability brokers, with generic durable work transport under `application/queue/`. `src/evidence/` holds shared concrete loaders. `src/runtime/` holds configuration, database connections, routing, and providers. `src/evaluation/` and the `eval` binary contain checks and replays whose legacy paths are included in the alignment cleanup. `statcommentary` is an explicit non-queue entry point. `factsweep` now refuses before database access: reporting-based role and affiliation extraction is unavailable. Investigator runtime uses structured Wikimedia evidence without inference; historical prose evaluation remains offline.

Harvester enrollment requires its additive migrations, `HARVESTER_INGEST_ENABLED=1` in Go ingestion, an explicit `COGNITION_STAGES` list containing `harvester`, and `HARVESTER_MODEL_ENDPOINT`. Go intake requires shadow mode off. Apply migrations 287–289 and remove `editor` from the configured stage list for the coordinated release; `HARVESTER_DELIVERY_CHARACTERS` still controls delivery enrollment. Do not infer deployment readiness from compilation or from a historical shadow report.

Use [development guidance](../run_docs/DEVELOPMENT.md), the [runbook](../run_docs/RUNBOOK.md), and [analytical acceptance](../run_docs/RECOVERY_ANALYTICS_ACCEPTANCE.md) for repository operations. [Harvester cutover verification](docs/harvester-cutover-verification-2026-09-27.md) records dated operational evidence. The [alignment plan](docs/PLAN-plugin-alignment-2026-09-27.md) governs the current responsibility audit and its handoffs.

## Adding a plugin

Add a package under `src/plugins/<name>/` with its manifest and adapter, plus cognition only where inference is needed. Register its manifest in `application/fleet.rs` and bind concrete dependencies in `application/plugins.rs`. The registry validates task ownership, route/grant consistency, and resource declarations at boot.

Select the needed parts from `meta.rs`, `voice.rs`, `memories.rs`, `fresh.rs`, `form.rs` and `prompt.rs`. Reuse shared identity, DuckDB studies, source presentation and form tools; keep dataset selection, scope, tone and task instructions with the plugin. Select additional capabilities such as `browser.rs` only for operations that need them. Extend the base toolkit when a real capability is missing, without adding empty parts or alternate instruction paths.

Define context, tools, structure, memory, voice where applicable, and factual boundaries before adding a model call. Add a database migration only for genuinely new persisted domain data. Prefer removing duplication to adding a workflow language, compatibility layer, or new abstraction.
