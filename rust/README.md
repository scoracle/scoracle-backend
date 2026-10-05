# Studio: the architecture contract

**SQL owns the data. Rust owns the cognition. Everything that performs work is a plugin.** The harness determines runnable work → the plugin assembles its world using shared or local tools → its model performs the bounded job → the plugin validates and publishes to Postgres.

Plugins select the evidence, scope, relationships and instructions needed for their work. Postgres stores and queries the world; SQL and DuckDB calculate factual measurements and studies. Rust prepares the model input, invokes the model, accepts the response and publishes the result. Models receive prepared input and never call tools, search or retrieve. Plugins do not prewrite the interpretation or prescribe a conclusion. Harvester is a peer plugin using a System 1 model, currently Laya; System 1 is a model inside that plugin.

**This README is the current architecture authority.** The [harness cleanup plan](docs/PLAN-harness-plugin-cleanup-2026-10-05.md) applies this contract to the audited implementation. The completed [plugin alignment plan](docs/PLAN-plugin-alignment-2026-09-27.md) and dated evaluations remain historical evidence. Planned cleanup is distinct from implementation, product verification and deployment.

## Ownership

- **Plugins / Rust:** tool calls, evidence selection, scope, context assembly, task and tone instructions, model invocation, validation and publication policy.
- **Models:** faithful articulation or bounded classification of supplied input; no invented evidence or tool calls.
- **Harness and host:** plugin registration, runnable work, dependency injection, inference transport, budgets, claim fencing and atomic publication coordination.
- **Postgres and DuckDB / SQL:** stored evidence, joins, name matching, factual arithmetic and bounded studies with dates, coverage and provenance. Prior prose cannot become measurement evidence. Existing Rust calculations move into SQL with parity checks as their plugins are simplified.

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

## Plugin structure and tool ownership

A plugin owns its `prompt.rs`, `manifest.rs` and execution wiring in `mod.rs`. Each articulation plugin keeps its own small `voice.rs`; plugins without expressive output need none. Shared tools live together; specialized tools live beside their plugin. Scout is on that layout (`prompt.rs`, `performance.rs`). The other plugins' prompt entry points and the remaining folder moves are recorded in the cleanup plan.

| File | Responsibility |
| --- | --- |
| `prompt.rs` | The Rust preparation entry point: calls the required tools with plugin-selected scopes, assembles model input and supplies task instructions. It may call local helpers; it need not contain every query or parser. |
| Local `voice.rs` | The plugin's tone and writing qualities only. |
| `manifest.rs` | Identity, task ownership, claim policy, inference routes, resource limits and capability grants consumed by the harness. |
| `mod.rs` | Connects preparation, model execution where needed, validation and claim-fenced publication. |

Tool calls are Rust code. Model instructions explain how to use the already supplied context. The model neither chooses tools nor requests additional evidence.

**Share a tool when sharing reduces complexity or fragility. Keep a tool plugin-specific when that reduces complexity or fragility.** Sharing is useful for stable mechanisms with compatible callers. A shared tool that accumulates unrelated plugin branches should become smaller shared mechanics plus local functions, or remain local.

For example, Scout and Journalist can share the DuckDB study runner while owning different queries, timeframes and presentation: Scout studies performance; Journalist studies reporting and narrative continuity. A shared verified-source reader can serve multiple characters. Harvester's acquisition and routing tools and Oracle's finished-card reader remain specific to their jobs.

### Useful shared tools

| Tool | Responsibility |
| --- | --- |
| `meta.rs` | Canonical identity and relevant stored attributes. Reuse identity already loaded by the plugin rather than adding a duplicate DB read. |
| `memories.rs` | Bounded SQL/DuckDB studies with dates, comparison scope, coverage and provenance. Plugins select the study and how to present its findings. |
| `fresh.rs` | Read newly available attributed source evidence for the plugin's scope. Reuse Harvester delivery and source presentation, including assignment eligibility and integrity checks. |
| `linked_entities.rs` | Read entity metadata, co-mention frequency and source references from SQL. Matching covers complete fetched articles; the plugin chooses the relevant scope. |
| Shared form helpers | Render and decode compatible output structures. Each plugin owns its actual output fields and acceptance rules. |

These are ordinary functions, not model-callable tools or a tool registry. Add a web-search tool when a plugin has a concrete live-retrieval job; Scout already reads source retained by Harvester.

`linked_entities.rs` is one context reader. SQL handles existing name/alias matching, counts and metadata joins. Reuse normalization and lightweight matching; distinguish uncertain identity matches. Count each entity once per canonical article and retain attributed source examples. Co-mentions describe reporting coverage; they do not establish an affiliation or transfer.

### Common execution, different work

Keep the existing `StudioPlugin` trait and `PluginManifest`. Plugin handler structs bind their dependencies; the worker invokes each through the common execution boundary. This provides encapsulation, composition and polymorphism with real consumers.

Harvester and Oracle share that boundary and canonical metadata, while retaining different inputs, tools, model protocols and outputs:

| Plugin | Prepared input and owned job |
| --- | --- |
| Harvester | Article/query identities, headline relevance and exact publisher-text windows. Its Rust recipe fetches source, supplies bounded classification questions, interprets scores and persists source/delivery receipts. |
| Oracle | Selected finished Journalist, Scout, Influencer, Analyst and Insider products, with explicit availability. Its Rust recipe prepares those cards and asks the model for one reading. |

Harvester uses a classification model and needs no voice. Oracle uses its local `voice::VOICE` and owns its finished-card selection. Deterministic plugins and insufficient-evidence outcomes can complete without inference. Articulation has one generation stage; existing bounded structural correction remains available.

`prompt.rs` owns the complete model input: tool calls, scope, selected data, ordering and task instructions. Separate assembly and parts layers are unnecessary. For the current fixed context shapes, a plugin-local `Serialize` struct and `serde_json::to_string` preserve field declaration order without a custom world renderer. Migrate callers and retain request-byte checks before deleting `assembly.rs`; keep useful measurement types and guards in specialized tools. Harvester keeps its typed classification requests. Existing inference, parser and classification interfaces remain useful where implementations actually differ. Capability grants control acquisition and inference permissions; ordinary context functions need no new grant registry.

### Production folders

The target is three production homes: `harness/` for execution infrastructure, `plugins/` for concrete workflows and specialized tools, and `tools/` for shared tools. Move working mechanisms into these homes while deleting their obsolete callers and forwarding modules.

- Dissolve `plugins/cognition/`: form types join the shared form tool; classification types belong to Harvester.
- Dissolve `plugins/support/`: real shared tools join `tools/`; scheduling constants and execution mechanics join the harness.
- Flatten each plugin's `adapter/` and `cognition/` layers as its recipe moves into `prompt.rs`. Keep real local tools and publication helpers by responsibility.
- Keep one shared `memories.rs` tool with the SQL and checks it uses. A dedicated memory namespace is optional organization, not a required layer; fold its small statistic adapter into the tool. The legacy `evidence::memories` package is gone. Canonical identity reads are `plugins/meta.rs`.
- Consolidate `studio/`, `application/` and `runtime/` under the harness as their code is simplified. Queue and provider subfolders may remain for substantial concrete mechanisms. Source readers leave `evidence/` for shared or specialized tools.

Entry points, tests, fixtures, evaluation utilities and documentation remain outside this production flow. A deletion is complete only when callers, exports and superseded paths are removed together; retain source integrity and durable execution behavior.

### Model input and acceptance

**Tone without an invented character is the target.** Describe prose qualities directly. Product names do not require the model to adopt a persona, backstory or motive.

Attach relationships to the evidence they qualify. Dates, sources, compatible comparisons, unknowns and coverage must survive articulation. Missing is different from zero; co-mention frequency is different from independent confirmation. Preparation supplies evidence without prescribing a conclusion.

Keep parsers, factual guards, source integrity, error handling, claim fencing, partial-progress receipts and atomic publication. Retire obsolete prompt composition, palettes and duplicate evaluation paths with their callers. Model requests, responses and full provenance remain inspectable outside the compact model input.

The Influencer model-directed `read_source` pilot and its provider tool-chat path are retired. The [dated evaluation](docs/scout-closure-2026-09-29.md#september-30-read-only-archbox-pilot) remains evidence. The [Journalist replay](docs/HANDOFF-journalist-finish-2026-09-28.md) and [memory studies](docs/journalist-memory-world-2026-09-28.md) record prepared-context behavior and fidelity work.

## Shared memory contract

**Postgres stores the world. DuckDB studies the world. Each plugin chooses its data and scope; the memory contract stays the same.**

Harvester and other source producers enrich the stored world. At preparation time, a plugin requests a study for a particular entity or pair, timeframe and comparison scope. DuckDB computes the findings on demand. The plugin selects and presents relevant findings alongside fresh content, ready for articulation; local memory helpers remain when they simplify that selection.

The shared contract is **request a scope → study the stored evidence → return findings with dates, coverage and provenance**:

- **The plugin owns the request:** which data matters, the historical timeframe, the comparison population and the context budget. Changing a plugin's lookback changes its request, without requiring a new precompute schedule.
- **The shared study layer owns computation:** filtering, grouping, frequency, ordering and compatible comparisons over a consistent snapshot. Results identify the study version, source records, time bounds, units where applicable, and missing or partial coverage. Source corrections and deletions must invalidate affected reuse.
- **The plugin's preparation owns context selection:** retain findings and qualifications useful to the current assignment. The shared memory tool supplies studies and receipts; full retrieval bookkeeping stays in provenance.
- **The LLM owns articulation:** express fresh material in the context of the supplied history. Retrieval, arithmetic and deciding whether reporting is valid have already been handled upstream.

Different plugins request different studies through this boundary. Journalist can request earlier reporting; Influencer can request observed reactions; Insider can request entity-pair reporting frequency and publisher breakdowns; Scout can request xG comparisons across specified match windows. Frequency measures recorded reporting, not independent confirmation. Numerical change retains its sample and comparison basis.

Reuse the stored world, study implementations and provenance where sharing simplifies the work, while keeping each plugin's selection policy local. See the [cross-plugin design](docs/journalist-memory-world-2026-09-28.md#shared-across-plugins) and [on-demand implementation](docs/memory-studies.md) for dated implementation evidence. Reporting and team-stat studies exist; the cleanup plan records the reporting query repair and remaining caller migration.

## Evidence, quality, and efficiency

- Compute arithmetic, scores, and comparisons upstream. Supply units, season/competition, comparison population, sample coverage, and uncertainty when they affect meaning.
- Select memory deliberately and label its provenance. A previous interpretation cannot manufacture a fact or inflate corroboration.
- Retain full provenance and debugging detail outside the articulation input. Include necessary evidence once and budget the complete request plus reserved output.
- Use the same active preparation and contract for production and evaluation. Remove superseded prompts, parsers, flags and executable paths with their obsolete callers; preserve historical fixtures and evaluation data.
- Evaluate models in this order: factual fidelity, articulation quality, voice consistency, concise synthesis, speed/efficiency, and general reasoning only where required. SmolLM3 is the selected articulation model; broad intelligence cannot compensate for an incomplete prepared world.
- Measure useful coverage, specificity, additions and omissions, voice, tokens, latency, and retries. Valid JSON or a valid palette index alone does not establish product quality.
- Preserve source integrity, claim fencing, atomic provenance/follow-up publication, idempotency, and recovery when pruning legacy code. Every retained temporary dependency needs a named consumer and removal condition.

## Source map and operations

The current implementation has plugin preparation/publication under `src/plugins/<name>/`, including the six plugin-local voice files. `support/form.rs` supplies output rendering, decoding and structural validation; `support/source.rs` presents intact publisher reporting and detects explicit instruction overrides. Identity lives in `plugins/meta.rs`; `plugins/memories.rs` supplies the shared DuckDB study runner and compatible studies. Plugins choose datasets and scopes and retain local study or presentation helpers when simpler. These mechanisms move into the three production homes described above. The [fleet](src/application/fleet.rs) excludes retired Editor; Harvester owns surviving source and maintenance work.

`src/studio/` holds generic inference sessions, generation envelopes, and plugin/tool contracts. `src/application/` assembles concrete dependencies and capability brokers, with generic durable work transport under `application/queue/`. `src/evidence/` holds shared concrete loaders. `src/runtime/` holds configuration, database connections, routing, and providers. `src/evaluation/` and the `eval` binary contain checks and replays whose legacy paths are included in the alignment cleanup. `statcommentary` is an explicit non-queue entry point. `factsweep` now refuses before database access: reporting-based role and affiliation extraction is unavailable. Investigator runtime uses structured Wikimedia evidence without inference; historical prose evaluation remains offline.

Harvester enrollment requires its additive migrations, `HARVESTER_INGEST_ENABLED=1` in Go ingestion, an explicit `COGNITION_STAGES` list containing `harvester`, and `HARVESTER_MODEL_ENDPOINT`. Go intake requires shadow mode off. Apply migrations 287–289 and remove `editor` from the configured stage list for the coordinated release; `HARVESTER_DELIVERY_CHARACTERS` still controls delivery enrollment. Do not infer deployment readiness from compilation or from a historical shadow report.

Use [development guidance](../run_docs/DEVELOPMENT.md), the [runbook](../run_docs/RUNBOOK.md), and [analytical acceptance](../run_docs/RECOVERY_ANALYTICS_ACCEPTANCE.md) for repository operations. [Harvester cutover verification](docs/harvester-cutover-verification-2026-09-27.md) records dated operational evidence. The [cleanup plan](docs/PLAN-harness-plugin-cleanup-2026-10-05.md) records the current responsibility audit and migration sequence.

## Adding a plugin

Add a package under `src/plugins/<name>/` with its manifest and adapter, plus cognition only where inference is needed. Register its manifest in `application/fleet.rs` and bind concrete dependencies in `application/plugins.rs`. The registry validates task ownership, route/grant consistency, and resource declarations at boot.

Write a plugin-owned `prompt.rs` that calls the tools needed for that job, scopes their reads and prepares the model request. Keep a local `voice.rs` when articulation needs one. Share identity, studies, source presentation and form mechanics where sharing simplifies the work; keep other tools local. Add capabilities such as browser retrieval only for operations that need them, without empty parts or alternate instruction paths.

Define context, tools, structure, memory, voice where applicable, and factual boundaries before adding a model call. Add a database migration only for genuinely new persisted domain data. Prefer removing duplication to adding a workflow language, compatibility layer, or new abstraction.
