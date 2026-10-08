# Studio: the architecture contract

**The harness provides the space → plugins bring tools and instructions → tools provide context → the LLM discovers what the clues support and articulates the answer. Cheap code owns collection; expensive compute owns discovery.**

The [governing wiki contract](../../scoracle-wiki/wiki/Architecture/Harness,%20Plugins,%20Tools,%20and%20LLM%20Contract.md) owns this platform boundary. Rust owns execution and context preparation; Postgres stores history and DuckDB analyzes it. Plugins assemble clues, relationships and limits so the model can reason, rather than preassembling a conclusion to paraphrase.

Plugins select the evidence, scope, relationships and instructions needed for their work. Postgres stores and queries the world; SQL and DuckDB calculate factual measurements and studies. Rust prepares the model input, invokes the model, accepts the response and publishes the result. Models receive prepared input and never call tools, search or retrieve. Plugins do not prewrite the interpretation or prescribe a conclusion. Harvester and Editor are peer plugins requesting the System 1 model capability, currently bound to Laya. System 1 and text generation are model plugins; consuming plugins own their questions, requested routes and inference settings in prompt.rs.

**This README maps the current implementation under the governing wiki contract.** The completed [harness cleanup plan](docs/PLAN-harness-plugin-cleanup-2026-10-05.md) records the implementation and validation of this contract. The completed [plugin alignment plan](docs/PLAN-plugin-alignment-2026-09-27.md) and dated evaluations remain historical evidence. The [deployment record](docs/harness-cleanup-deployment-2026-10-05.md) records the Archbox release and paused Mac worker; context expansion remains separate.

The [Classifier launch](docs/scoracle_classifier_plugin_launch.md) replaces Harvester/Editor intake with an independent measurement plugin. Its source-bound encoder and expression replays are offline today; production registration and delivery cutover remain pending. Shared source windows and the character-facing `SourceContext` live in `tools::source`, outside either intake plugin.

The [governing Classifier target design](docs/scoracle_classifier_plugin_launch.md#target-design--october-7-2026) is Google discovery → native source acquisition → AI deconstruction → SQL retention/measurement → Rust character worlds → AI discovery/expression → SQL + Go serving. Relevance and character eligibility are derived from the measured spectrum and qualified evidence, without a separate AI yes/no admission test. Complete measurements with no supported downstream signal produce no new character work; unknown, incomplete or failed measurements never become absence. Classification quality comes first; model size and inference cost remain measured choices.

## Ownership

- **Plugins / Rust:** tool calls, evidence selection, scope, context assembly, task and tone instructions, model invocation, validation and publication policy.
- **Models:** reasoning across supplied clues, discovering supported meaning and articulating a reading, or bounded classification. Preserve attribution, uncertainty and evidence limits; current models do not call tools.
- **Harness and host:** plugin registration, runnable work, dependency injection, inference transport, budgets, claim fencing and atomic publication coordination.
- **Postgres and DuckDB / SQL:** stored evidence, joins, name matching, factual arithmetic and bounded studies with dates, coverage and provenance. Prior prose cannot become measurement evidence. Existing Rust calculations move into SQL with parity checks as their plugins are simplified.

## Harvester screens; Editor reads and routes

Harvester uses cheap headline-only relevance scoring. An admitted headline queues a separate, claim-fenced Editor job. Editor acquires or reuses retained publisher text, scores the full retained article in exact bounded windows, and routes source references to downstream plugins. Neither stage generates a summary, emotional verdict or editorial packet.

1. Harvester binds the Google candidate to canonical entity metadata and scores explicit headline reference before any publisher fetch. The provisional admission threshold remains 0.25.
2. Editor loads the verified headline receipts and acquires the article. Acquisition failures remain visible outcomes rather than negative relevance judgments.
3. Editor scores all retained non-whitespace text through windows of at most 100 words and 1,200 bytes. More than 64 windows fails visibly. Each window includes an article relevance predicate; windows below its provisional 0.5 threshold cannot contribute downstream theme scores. All raw scores and source offsets remain in provenance.
4. Editor applies the routing policy: theme thresholds are 0.5, except performance at 0.7. Journalist, Influencer and Insider use the shared reader to obtain the full verified retained source. Scout keeps the opening excerpt. Actual publisher language, not routing scores, reaches text generation.
5. Source presentation mechanically decodes HTML punctuation while retained source bytes remain unchanged. Article consumers declare their generation route and reading settings in prompt.rs. Their bounded article packages request at least 32,768 context tokens; oversized packages are explicit failures or dispositions, never silent truncation.

The new receipt contract is `harvest-context-v9-entity-vibe`; the headline contract stays `harvest-headline-v3`. Existing historical receipt tables are reused, and legacy `editor_reads` summaries remain unused. The thresholds are provisional and require corpus calibration; the first replay already missed a relevant Sunderland article. Full article access also does not by itself solve unsupported inference or output-format failures.

Both stages are opt-in through `COGNITION_STAGES=harvester,editor,...`. Configure `HARVESTER_MODEL_ENDPOINT` and `EDITOR_MODEL_ENDPOINT`; they can select the same typed scorer deployment. `HARVESTER_DELIVERY_CHARACTERS` and `HARVESTER_SHADOW_MODE` retain their existing downstream release controls, now applied at Editor publication. The checked-in build is not a production deployment. See the [build ledger and model ownership contract](docs/editor-system-one-build-2026-10-06.md).

## Plugin structure and tool ownership

A plugin owns its `prompt.rs`, `manifest.rs` and execution wiring in `mod.rs`. Each articulation plugin keeps its own small `voice.rs`; plugins without expressive output need none. Shared tools live together; specialized tools live beside their plugin. Scout preparation and rendering are `prompt.rs`; performance SQL, selection and measurements are `performance.rs`; response guards are `parser.rs`; execution is `mod.rs` with local publication/delivery tools; Influencer preparation, execution and publication are `prompt.rs`, `mod.rs` and `publish.rs`. The cleanup plan records the completed ownership moves and their validation.

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
| `memories.rs` | DuckDB memory studies over read-only Postgres exports, with dates, comparison scope, coverage and provenance. Plugins select the study and relevant context. |
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

`prompt.rs` owns the complete model input: tool calls, scope, selected data, ordering and task instructions. Separate assembly and parts layers are unnecessary. The six articulation worlds render with a plugin-local `Serialize` struct and `serde_json::to_string`. `assembly.rs` is gone. Keep useful measurement types and guards in specialized tools. Harvester keeps its typed classification requests. Existing inference, parser and classification interfaces remain useful where implementations actually differ. Capability grants control acquisition and inference permissions; ordinary context functions need no new grant registry.

### Production folders

Production uses three homes: `harness/` for execution infrastructure, `plugins/` for concrete workflows and specialized tools, and `tools/` for shared tools. Obsolete top-level namespaces and their forwarding exports are removed.

- `plugins/cognition/` is removed: `tools/form.rs` owns form types and decoding; `plugins/system_one.rs` owns the classification protocol; `plugins/text_generation.rs` binds generative model implementations. `plugins/harvester/decision.rs` is a compatibility export for historical callers.
- `plugins/support/` is removed: form, guard and source tools live in `tools/`; correction policies live in `harness/session.rs`, and model slot constants live in `harness/fleet.rs`.
- Flatten each plugin's `adapter/` and `cognition/` layers as its recipe moves into `prompt.rs`. All six articulation plugins now use these local owners. Keep real local tools and publication helpers by responsibility.
- `tools/memories.rs` owns shared reporting and statistic studies, with one DuckDB runner; its SQL and integration checks stay alongside it. The statistic adapter is folded into the tool. Canonical identity reads and display-name lookup are `tools/meta.rs`.
- `studio/`, `application/` and `runtime/` are consolidated in `harness/`, retaining queue and provider subfolders. Tool grants and the scoped web broker share `harness/tools.rs`. The `evidence/` namespace is removed: shared acquisition is `tools/fetch.rs`, Scout readers and contested reports are local `sources.rs` and `reports.rs`, and Graph owns quote and result helpers.

Entry points, tests, fixtures, evaluation utilities and documentation remain outside this production flow. A deletion is complete only when callers, exports and superseded paths are removed together; retain source integrity and durable execution behavior.

### Model input and acceptance

**Tone without an invented character is the target.** Describe prose qualities directly. Product names do not require the model to adopt a persona, backstory or motive.

Attach relationships to the evidence they qualify. Dates, sources, compatible comparisons, unknowns and coverage must survive articulation. Missing is different from zero; co-mention frequency is different from independent confirmation. Preparation supplies evidence without prescribing a conclusion.

Keep parsers, factual guards, source integrity, error handling, claim fencing, partial-progress receipts and atomic publication. Retire obsolete prompt composition, palettes and duplicate evaluation paths with their callers. Model requests, responses and full provenance remain inspectable outside the compact model input.

The Influencer model-directed `read_source` pilot and its provider tool-chat path are retired. The [dated evaluation](docs/scout-closure-2026-09-29.md#september-30-read-only-archbox-pilot) remains evidence. The [Journalist replay](docs/HANDOFF-journalist-finish-2026-09-28.md) and [memory studies](docs/journalist-memory-world-2026-09-28.md) record prepared-context behavior and fidelity work.

The [client-focused build](docs/client-focused-card-building-session-10-7-2026.md) now has a local Influencer period path. Accepted entity/source receipts feed direct collection without storyline membership or generated summaries. The existing DuckDB runner deduplicates known copies and selects at most 20 reports over the reporting week and 30-day history. Complete fresh reporting must fit the existing 24,000-byte budget; overflow defers publication visibly. Historical full text has a 4,000-byte allowance, with excluded source IDs and reasons retained. No text is summarized or silently clipped.

`influencer/prompt.rs` owns the TARGET → RELEVANT HISTORY → FRESH EVIDENCE packet, emotional-valence anchors, structure and character. One model call produces the coherent score/headline/body card, or all-null abstention. The agreed scale is 0 distressing, 25 troubled, 50 genuinely mixed/balanced, 75 hopeful, 100 joyful; missing emotional evidence is absence. Laya now admits Influencer by entity relevance alone, leaving emotional discovery to SmolLM3. Incoming delivery uses the existing queue with a 60-second coalescing window. Its relevance/window scores remain provisional and uncalibrated.

Migration 290 admits zero and retains period bounds, acquisition cutoff, scoring version and frozen source references on append-only `vibe_scores`. `vibe_card_attempts` retains exact prepared packets, transport request objects and raw responses, including parse/truncation failures. Claim fencing, source rechecks, all covered assignment dispositions and publication remain atomic. An unchanged successful packet/request reuses its original card. Failed generation leaves the previous card and pending work intact.

`GET /{sport}/{entityType}/{id}/vibe` serves the latest saved card; `?season=2026&week=1` selects the latest saved revision in that reporting-calendar week. Empty periods return `current:null`; generation time and evidence window remain separate. `score` and compatibility `heat` come from the same row. The existing Solid frontend eagerly loads dedicated endpoints and renders `current.heat`, headline, paragraphs, date and absence; no frontend edit is needed for this compatible current-card contract.

This build is deployed experimentally on Archbox on October 7 with explicit user authorization. The focused contract, isolated PostgreSQL/DuckDB publication/idempotency and historical API checks pass. The [single resident SmolLM3 replay](fixtures/influencer/period-card-v19-review.json) is structurally valid but invents a coaching-staff change and fan/player feelings. Its exact packet, request and response are retained beside that review. This is a fidelity failure, not an acceptance baseline; the user accepted deployment for testing despite this result. Real-source lookup was unavailable in this session. Held-out Laya calibration, real-source fidelity review, historical frontend controls, and the remaining products (including Momentum/Oracle generated-prose dependencies) remain outstanding.

Reuse inspection: [Rig agent builder](https://github.com/0xPlaygrounds/rig/blob/main/crates/rig-agent/src/agent/builder.rs) separates preamble/context/transport; [Haystack PromptBuilder](https://github.com/deepset-ai/haystack/blob/main/haystack/components/builders/prompt_builder.py) renders explicit document inputs; [Semantic Kernel prompt plugins](https://github.com/microsoft/semantic-kernel/blob/main/python/samples/concepts/plugins/plugins_from_dir.py) keeps task prompts in plugins; [LlamaIndex context prompts](https://github.com/run-llama/llama_index/blob/main/llama-index-core/llama_index/core/prompts/default_prompts.py) marks context boundaries. Inspected the linked main-branch files on 2026-10-07. Retained our ordinary Rust functions and existing provider: none warrants importing a framework or another generation stage. These patterns do not fix the observed SmolLM3 fidelity failure.

## Shared memory contract

**Postgres stores the world. DuckDB studies the world. Each plugin chooses its data and scope; the memory contract stays the same.**

Harvester and other source producers enrich the stored world. At preparation time, a plugin requests a study for a particular entity or pair, timeframe and comparison scope. DuckDB computes the findings on demand. The plugin selects and presents relevant findings alongside fresh content, ready for articulation; local memory helpers remain when they simplify that selection.

The shared contract is **request a scope → study the stored evidence → return findings with dates, coverage and provenance**:

- **The plugin owns the request:** which data matters, the historical timeframe, the comparison population and the context budget. Changing a plugin's lookback changes its request, without requiring a new precompute schedule.
- **The shared study layer owns computation:** filtering, grouping, frequency, ordering and compatible comparisons over a consistent snapshot. Results identify the study version, source records, time bounds, units where applicable, and missing or partial coverage. Source corrections and deletions must invalidate affected reuse.
- **The plugin's preparation owns context selection:** retain findings and qualifications useful to the current assignment. The shared memory tool supplies studies and receipts; full retrieval bookkeeping stays in provenance.
- **The LLM owns discovery and articulation:** reason across fresh evidence and studied history to discover what they support. Tools own retrieval and authoritative measurements; source selection does not predecide the reading or certify every report as true.

Oracle has no memories. Scout studies availability, transfers and event statistics; Journalist narrative continuity; Influencer relevant reporting and source-stated emotional context; Insider publisher records and transfer trends; Analyst Scout notability and Influencer sentiment. Different plugins request different studies through this boundary. Journalist can request earlier reporting; Influencer can request observed reactions; Insider can request entity-pair reporting frequency and publisher breakdowns; Scout can request xG comparisons across specified match windows. Frequency measures recorded reporting, not independent confirmation. Numerical change retains its sample and comparison basis.

Reuse the stored world, study implementations and provenance where sharing simplifies the work, while keeping each plugin's selection policy local. See the [cross-plugin design](docs/journalist-memory-world-2026-09-28.md#shared-across-plugins) and [on-demand implementation](docs/memory-studies.md) for dated implementation evidence. Reporting and team-stat studies exist; the cleanup plan records the reporting query repair and completed caller migration.

## Evidence, quality, and efficiency

- Compute arithmetic, scores, and comparisons upstream. Supply units, season/competition, comparison population, sample coverage, and uncertainty when they affect meaning.
- Select memory deliberately and label its provenance. A previous interpretation cannot manufacture a fact or inflate corroboration.
- Retain full provenance and debugging detail outside the articulation input. Include necessary evidence once and budget the complete request plus reserved output.
- Use the same active preparation and contract for production and evaluation. Remove superseded prompts, parsers, flags and executable paths with their obsolete callers; preserve historical fixtures and evaluation data.
- Evaluate evidence fidelity and reasoning together: supported discoveries, unsupported additions, attribution, uncertainty, useful synthesis and reader clarity, then resource cost. SmolLM3 is the current selected model; live output-tuning replays still expose fidelity failures. The old paragraph and body character limits have been removed; the output form is a 140-character header and concise body paragraphs. Complete, traceable clues empower discovery; parser acceptance alone does not establish it.
- Measure useful coverage, specificity, additions and omissions, voice, tokens, latency, and retries. Valid JSON or a valid palette index alone does not establish product quality.
- Preserve source integrity, claim fencing, atomic provenance/follow-up publication, idempotency, and recovery when pruning legacy code. Every retained temporary dependency needs a named consumer and removal condition.

## Source map and operations

The current implementation has plugin preparation/publication under `src/plugins/<name>/`, including the six plugin-local voice files. `tools/form.rs` supplies output types, rendering, decoding and structural validation; `tools/guards.rs` supplies served-prose checks; `tools/source.rs` presents publisher language with mechanical HTML-reference decoding, retains original source bytes in provenance and detects explicit instruction overrides. Harvester probability scores route work; they do not establish the reading's emotional tone. Identity lives in `tools/meta.rs`; `tools/memories.rs` supplies the shared DuckDB study runner and compatible studies; `tools/memories/postgres.rs` exports stored observations. Plugins choose datasets and scopes and retain local study or presentation helpers when simpler. These mechanisms use the three production homes described above. The [fleet](src/harness/fleet.rs) excludes Editor; its implementation, evaluation task and obsolete eval fixtures are removed; historical database records remain. Harvester owns surviving source and maintenance work.

`src/harness/` holds fleet registration and dependency binding, generic inference sessions and model/plugin contracts, configuration, database connections, routing, diagnostics and debounce readers. `harness/queue/` owns claim fencing, worker supervision, atomic publication and outbox recovery; `harness/providers/` owns real model transports. `src/tools/` holds shared acquisition, identity, memory studies, form and source presentation. Plugin-specific selection, source validation and product policy stay under `src/plugins/`. `src/evaluation/`, operator binaries, examples and fixtures remain outside the production flow. `statcommentary` is an explicit non-queue entry point. `factsweep` refuses before database access: reporting-based role and affiliation extraction is unavailable. Investigator runtime uses structured Wikimedia evidence without inference; historical prose evaluation remains offline.

Harvester enrollment requires its additive migrations, `HARVESTER_INGEST_ENABLED=1` in Go ingestion, an explicit `COGNITION_STAGES` list containing `harvester`, and `HARVESTER_MODEL_ENDPOINT`. Go intake requires shadow mode off. Apply migrations 287–289 and remove `editor` from the configured stage list for the coordinated release; `HARVESTER_DELIVERY_CHARACTERS` still controls delivery enrollment. Do not infer deployment readiness from compilation or from a historical shadow report.

Use [development guidance](../run_docs/DEVELOPMENT.md), the [runbook](../run_docs/RUNBOOK.md), and [analytical acceptance](../run_docs/RECOVERY_ANALYTICS_ACCEPTANCE.md) for repository operations. [Harvester cutover verification](docs/harvester-cutover-verification-2026-09-27.md) records dated operational evidence. The [cleanup plan](docs/PLAN-harness-plugin-cleanup-2026-10-05.md) records the current responsibility audit and migration sequence.

## Adding a plugin

Add a package under `src/plugins/<name>/` with its manifest, execution in `mod.rs`, and preparation in `prompt.rs` where needed. Register its manifest in `harness/fleet.rs` and bind concrete dependencies in `harness/registration.rs`. The registry validates task ownership, route/grant consistency, and resource declarations at boot.

Write a plugin-owned `prompt.rs` that calls the tools needed for that job, scopes their reads and prepares the model request. Keep a local `voice.rs` when articulation needs one. Share identity, studies, source presentation and form mechanics where sharing simplifies the work; keep other tools local. Add capabilities such as browser retrieval only for operations that need them, without empty parts or alternate instruction paths.

Define context, tools, structure, memory, voice where applicable, and factual boundaries before adding a model call. Add a database migration only for genuinely new persisted domain data. Prefer removing duplication to adding a workflow language, compatibility layer, or new abstraction.
