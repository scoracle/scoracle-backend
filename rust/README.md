# Studio: the architecture contract

**The harness provides the space → plugins bring tools and instructions → tools provide context → the LLM discovers what the clues support and articulates the answer. Cheap code owns collection; expensive compute owns discovery.**

The [governing wiki contract](../../scoracle-wiki/wiki/Architecture/Harness,%20Plugins,%20Tools,%20and%20LLM%20Contract.md) owns this platform boundary. Rust owns execution and context preparation; Postgres stores history and DuckDB analyzes it. Plugins assemble clues, relationships and limits so the model can reason, rather than preassembling a conclusion to paraphrase.

Plugins select the evidence, scope, relationships and instructions needed for their work. Postgres stores and queries the world; SQL and DuckDB calculate factual measurements and studies. Rust prepares the model input, invokes the model, accepts the response and publishes the result. Models receive prepared input and never call tools, search or retrieve. Plugins do not prewrite the interpretation or prescribe a conclusion. Classifier uses d1 through llama.cpp's `/v1/systemone`; character generation uses SmolLM3 through native llama.cpp.

**This README maps the current implementation under the governing wiki contract.** The completed [harness cleanup plan](docs/PLAN-harness-plugin-cleanup-2026-10-05.md) records the implementation and validation of this contract. The completed [plugin alignment plan](docs/PLAN-plugin-alignment-2026-09-27.md) and dated evaluations remain historical evidence. The [deployment record](docs/harness-cleanup-deployment-2026-10-05.md) records the Archbox release and paused Mac worker; context expansion remains separate.

The [Classifier launch](docs/scoracle_classifier_plugin_launch.md) records the design history. Classifier now owns acquisition, source-bound decision measurements and durable character handoffs. Shared source windows and the character-facing `SourceContext` live in `tools::source`.

The [governing Classifier target design](docs/scoracle_classifier_plugin_launch.md#target-design--october-7-2026) is Google discovery → native source acquisition → AI deconstruction → SQL retention/measurement → Rust character worlds → AI discovery/expression → SQL + Go serving. Relevance and character eligibility are derived from the measured spectrum and qualified evidence, without a separate AI yes/no admission test. Complete measurements with no supported downstream signal produce no new character work; unknown, incomplete or failed measurements never become absence. Classification quality comes first; model size and inference cost remain measured choices.

## Ownership

- **Plugins / Rust:** tool calls, evidence selection, scope, context assembly, task and tone instructions, model invocation, validation and publication policy.
- **Models:** reasoning across supplied clues, discovering supported meaning and articulating a reading, or bounded classification. Preserve attribution, uncertainty and evidence limits; current models do not call tools.
- **Harness and host:** plugin registration, runnable work, dependency injection, inference transport, budgets, claim fencing and atomic publication coordination.
- **Postgres and DuckDB / SQL:** stored evidence, joins, name matching, factual arithmetic and bounded studies with dates, coverage and provenance. Prior prose cannot become measurement evidence. Existing Rust calculations move into SQL with parity checks as their plugins are simplified.

## Classifier on native llama.cpp

1. `classifier_acquire` resolves the publisher URL and uses the shared local browser reader when `ARTICLE_READER_URL` is configured. The reader selects `article`, falls back to `main` only when `article` is missing, and retains the headline and returned body without the legacy scrub pass. Browser egress is pinned to public DNS addresses. Never-acquired articles older than 48 hours, inaccessible pages, unusable bodies, bodies below 80 words and bodies above 24,000 bytes receive durable skip receipts. Transport failures, server errors and domain holds get one deferred retry. Google URL mappings, reader provenance and accepted bodies persist across restarts. The current source is selected atomically in `classifier_source_heads`.
2. `classifier` sends the complete retained article, sport and real `meta.rs` identity context in one native decision request per explicit RSS subject. d1 returns four 0–100 scores: subject relevance, emotional charge, movement, and availability. Metadata identifies the subject; evidence comes from the article. Four native slots each have an 8,192-token context. Missing, malformed or oversized measurements remain failures; there is no first-window fallback.
3. Each subject's `classifier_spectra` row retains the exact request/reply, request hash, source revision, scrub version, criteria version and GGUF SHA-256 model alias. Identical inputs across RSS revisions can reuse a validated receipt with the new source binding. Failed attempts remain in `classifier_measurements`. Stored role aliases preserve the existing character-host source contract.
4. All subject measurements and gated obligations commit together under the queue's claim fence. Subject relevance must reach 55. Journalist can receive neutral relevant reporting; Influencer additionally requires emotion 15, Scout availability 90, and Insider movement 80. Repeated claims cannot duplicate obligations, and a newly acquired source supersedes older pending deliveries. The fresh-context consumers share the retained reader body through the existing `SourceContext`; character implementations were not changed for this rollout.
5. Character generation shares the Mac's four SmolLM3 slots. Archbox reaches the loopback-only Mac server through an SSH tunnel. The retired Python Laya service and Candle inference are outside this path.

The [reader rollout and calibration handoff](docs/classifier-reader-rollout-2026-10-17.md) records the current production state, reader decisions, FigJam flows, accuracy results and next-session work. The [October 15 four-score report](docs/classifier-four-score-calibration-2026-10-15.md) remains historical evidence.

Apply migrations 302–312 before deploying the worker. `scripts/hosting/run-classifier-model.sh` fingerprints the loaded GGUF and starts the native endpoint; the existing `scoracle-kev.service` name supplies restart recovery for d1. Characters acknowledge spectrum obligations through the shared fenced completion function. The four-score bank and cutoffs remain provisional. Scores nominate useful source material; character plugins establish attribution and supported facts. Historical Harvester/Editor code remains only for retained replay compatibility; neither task is registered in the fleet.

The [cold acquisition benchmark](docs/acquisition-benchmark-2026-10-15.md) accepted 73 of 192 real discoveries. It projects about 530–685 accepted articles in two hours; Google URL resolution remains the main throughput limit. This is a bounded sample, not a sustained production guarantee.

The [continuous coverage and daily character plan](docs/PLAN-continuous-coverage-daily-characters.md) describes hourly discovery and Classifier preparation on Archbox, a bounded coverage cutoff, frozen daily assignments, character work on the Mac and automatic reopening. The single JSON packages, phase-aware RSS monitor and daily schedule are deployed. See the [deployment record](docs/daily-pipeline-deployment-2026-10-16.md) for verification and recovery, and [the package and host setup](docs/daily-character-packages.md) for export commands and example environments.

Graph is retired: acquisition offers only Classifier work, no Graph task or model route is registered, and migration 308 removes outstanding Graph claims and rejects new offers. Historical extractions, event/story links, identities and candidate evidence remain retained. DuckDB memory studies remain available for later integration; unknown entity search is deferred to a future Investigator change. The [hosting retirement procedure](../scripts/hosting/README.md#graph-retirement) coordinates worker/configuration replacement with the migration. The [October 16 deployment record](docs/graph-retirement-deployment-2026-10-16.md) confirms the live cutover and retained recovery snapshots.

The [acquisition report](examples/acquisition_report.sql) shows skip reasons, latency, publisher yield and historical 25/50 rank slices. RSS remains at 100 until coverage is reviewed. The 24 KB intake cap is conservative, not proof of native token fit. Unchanged discovery keys reuse accepted text indefinitely; publisher corrections without a changed URL, title, publication time or supplied body are not polled.

For a read-only decision replay on the Mac, use `CLASSIFIER_URL=http://127.0.0.1:18081 cargo run --release --example classifier_replay -- cases.jsonl 4` through the persistent SSH forward; on Archbox use port 8081. Each JSONL case supplies `id`, `sport`, `body`, canonical `meta` (with `identity` and `identity_record`), and optional per-question boolean `expected` labels. The command emits scores and timing without touching the database or dispatching characters. Synthetic controls are diagnostics, not held-out calibration. The [October 15 capacity report](docs/classifier-native-calibration-2026-10-15.md) records the earlier eight-slot Kev benchmark; it does not establish d1's sustained production capacity.

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
- `studio/`, `application/` and `runtime/` are consolidated in `harness/`, retaining queue and provider subfolders. Tool grants and the scoped web broker share `harness/tools.rs`. The `evidence/` namespace is removed: shared acquisition is `tools/fetch.rs`, Scout readers and contested reports are local `sources.rs` and `reports.rs`, Graph extraction and its quote/result helpers are retired.

Entry points, tests, fixtures, evaluation utilities and documentation remain outside this production flow. A deletion is complete only when callers, exports and superseded paths are removed together; retain source integrity and durable execution behavior.

### Model input and acceptance

**Tone without an invented character is the target.** Describe prose qualities directly. Product names do not require the model to adopt a persona, backstory or motive.

Attach relationships to the evidence they qualify. Dates, sources, compatible comparisons, unknowns and coverage must survive articulation. Missing is different from zero; co-mention frequency is different from independent confirmation. Preparation supplies evidence without prescribing a conclusion.

Keep parsers, factual guards, source integrity, error handling, claim fencing, partial-progress receipts and atomic publication. Retire obsolete prompt composition, palettes and duplicate evaluation paths with their callers. Model requests, responses and full provenance remain inspectable outside the compact model input.

The Influencer model-directed `read_source` pilot and its provider tool-chat path are retired. The [dated evaluation](docs/scout-closure-2026-09-29.md#september-30-read-only-archbox-pilot) remains evidence. The [Journalist replay](docs/HANDOFF-journalist-finish-2026-09-28.md) and [memory studies](docs/journalist-memory-world-2026-09-28.md) record prepared-context behavior and fidelity work.

Character preparation now has one daily fresh package per canonical subject and character. Complete pending Classifier reporting supplies Journalist, Insider and Influencer; Scout uses the current measured profile and fresh reporting. Analyst and Oracle read that day's completed peer outputs. No character preparation calls a historical memory study. The memory layer is deferred.

Packages retain complete text and source qualifications. A package that exceeds the existing 24 KB reading budget is explicitly held; it is never split into repeated per-article publications. Migration 310 replaces weekly stamping and sealing with Eastern reporting dates, and its publication guard permits one publication transaction per character/subject/day when daily scheduling is enrolled. Multiple source reports can belong to one Journalist result.

`GET /{sport}/{entityType}/{id}/vibe` serves the latest saved card; `?date=2026-10-15` selects a daily card. Other reporting reads use the same date parameter. Weekly/year/scope reporting filters return 400, and `/weeks` is retired. The client may build longer views over stored daily outputs later. Historical records retain their original generation dates, reporting bounds and provenance.

The following paragraph records the earlier experimental deployment; it does not describe the new daily package's qualification.

This build is deployed experimentally on Archbox on October 7 with explicit user authorization. The focused contract, isolated PostgreSQL/DuckDB publication/idempotency and historical API checks pass. The [single resident SmolLM3 replay](fixtures/influencer/period-card-v19-review.json) is structurally valid but invents a coaching-staff change and fan/player feelings. Its exact packet, request and response are retained beside that review. This is a fidelity failure, not an acceptance baseline; the user accepted deployment for testing despite this result. Real-source lookup was unavailable in this session. Held-out Laya calibration, real-source fidelity review, historical frontend controls, and the remaining products (including Momentum/Oracle generated-prose dependencies) remain outstanding.

Reuse inspection: [Rig agent builder](https://github.com/0xPlaygrounds/rig/blob/main/crates/rig-agent/src/agent/builder.rs) separates preamble/context/transport; [Haystack PromptBuilder](https://github.com/deepset-ai/haystack/blob/main/haystack/components/builders/prompt_builder.py) renders explicit document inputs; [Semantic Kernel prompt plugins](https://github.com/microsoft/semantic-kernel/blob/main/python/samples/concepts/plugins/plugins_from_dir.py) keeps task prompts in plugins; [LlamaIndex context prompts](https://github.com/run-llama/llama_index/blob/main/llama-index-core/llama_index/core/prompts/default_prompts.py) marks context boundaries. Inspected the linked main-branch files on 2026-10-07. Retained our ordinary Rust functions and existing provider: none warrants importing a framework or another generation stage. These patterns do not fix the observed SmolLM3 fidelity failure.

## Memories are deferred

Character inputs currently contain one fresh daily context. Prior-report packages, source-history packets and rolling weekly studies are removed from their preparation paths. Existing stored evidence and unused general study tools remain available for a separate memory implementation later; the dated [memory design](docs/memory-studies.md) is historical context, not an active character dependency.

## Evidence, quality, and efficiency

- Compute arithmetic, scores, and comparisons upstream. Supply units, season/competition, comparison population, sample coverage, and uncertainty when they affect meaning.
- Keep historical interpretation out of the fresh package. A future memory layer must label its provenance and cannot manufacture facts or inflate corroboration.
- Retain full provenance and debugging detail outside the articulation input. Include necessary evidence once and budget the complete request plus reserved output.
- Use the same active preparation and contract for production and evaluation. Remove superseded prompts, parsers, flags and executable paths with their obsolete callers; preserve historical fixtures and evaluation data.
- Evaluate evidence fidelity and reasoning together: supported discoveries, unsupported additions, attribution, uncertainty, useful synthesis and reader clarity, then resource cost. SmolLM3 is the current selected model; live output-tuning replays still expose fidelity failures. The old paragraph and body character limits have been removed; the output form is a 140-character header and concise body paragraphs. Complete, traceable clues empower discovery; parser acceptance alone does not establish it.
- Measure useful coverage, specificity, additions and omissions, voice, tokens, latency, and retries. Valid JSON or a valid palette index alone does not establish product quality.
- Preserve source integrity, claim fencing, atomic provenance/follow-up publication, idempotency, and recovery when pruning legacy code. Every retained temporary dependency needs a named consumer and removal condition.

## Source map and operations

The current implementation has plugin preparation/publication under `src/plugins/<name>/`, including the six plugin-local voice files. `tools/form.rs` supplies output types, rendering, decoding and structural validation; `tools/guards.rs` supplies served-prose checks; `tools/source.rs` presents publisher language with mechanical HTML-reference decoding, retains original source bytes in provenance and detects explicit instruction overrides. Harvester probability scores route work; they do not establish the reading's emotional tone. Identity lives in `tools/meta.rs`; `tools/memories.rs` supplies the shared DuckDB study runner and compatible studies; `tools/memories/postgres.rs` exports stored observations. Character preparation currently uses fresh daily data; memory integration is deferred. These mechanisms use the three production homes described above. The [fleet](src/harness/fleet.rs) excludes Editor; its implementation, evaluation task and obsolete eval fixtures are removed; historical database records remain. Harvester owns surviving source and maintenance work.

`src/harness/` holds fleet registration and dependency binding, generic inference sessions and model/plugin contracts, configuration, database connections, routing, diagnostics and debounce readers. `harness/queue/` owns claim fencing, worker supervision, atomic publication and outbox recovery; `harness/providers/` owns real model transports. `src/tools/` holds shared acquisition, identity, memory studies, form and source presentation. Plugin-specific selection, source validation and product policy stay under `src/plugins/`. `src/evaluation/`, operator binaries, examples and fixtures remain outside the production flow. `statcommentary` is an explicit non-queue entry point. `factsweep` refuses before database access: reporting-based role and affiliation extraction is unavailable. Investigator runtime uses structured Wikimedia evidence without inference; historical prose evaluation remains offline.

Classifier acquisition and spectrum delivery replace the retired Harvester/Editor workers.
Their remaining cognition helpers support offline historical replays only. Enable current
workers through the explicit host configurations in `examples/coverage/`; coordinate the
daily schedule, migrations, and both hosts before activating the handoff.

Use [development guidance](../run_docs/DEVELOPMENT.md), the [runbook](../run_docs/RUNBOOK.md), and [analytical acceptance](../run_docs/RECOVERY_ANALYTICS_ACCEPTANCE.md) for repository operations. [Harvester cutover verification](docs/harvester-cutover-verification-2026-09-27.md) records dated operational evidence. The [cleanup plan](docs/PLAN-harness-plugin-cleanup-2026-10-05.md) records the current responsibility audit and migration sequence.

## Adding a plugin

Add a package under `src/plugins/<name>/` with its manifest, execution in `mod.rs`, and preparation in `prompt.rs` where needed. Register its manifest in `harness/fleet.rs` and bind concrete dependencies in `harness/registration.rs`. The registry validates task ownership, route/grant consistency, and resource declarations at boot.

Write a plugin-owned `prompt.rs` that calls the tools needed for that job, scopes their reads and prepares the model request. Keep a local `voice.rs` when articulation needs one. Share identity, studies, source presentation and form mechanics where sharing simplifies the work; keep other tools local. Add capabilities such as browser retrieval only for operations that need them, without empty parts or alternate instruction paths.

Define context, tools, structure, memory, voice where applicable, and factual boundaries before adding a model call. Add a database migration only for genuinely new persisted domain data. Prefer removing duplication to adding a workflow language, compatibility layer, or new abstraction.
