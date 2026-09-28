# Studio: the architecture contract

**Plugins prepare and govern. System 1 scores. LLM articulates.**

Plugins build the decision world for both System 1 and the LLM. The LLM's job is articulation, not world-building. A plugin prepares the world the model may express: context, admissible facts, tools, structure, memory, voice, and factual boundaries. The plugin determines **what can be said**. SmolLM3 determines **how it is said**.

The model may express facts naturally, synthesize supplied context, choose emphasis, apply a character's voice, and compress information while preserving important meaning. It must not discover the relevant facts, reconstruct missing context, decide what is true, or invent facts, statistics, events, relationships, or assumptions. Unsupported invention is an interface violation.

The test for every inference is: **Did the model express the supplied world well without adding anything that was not there?**

The [plugin alignment plan](docs/PLAN-plugin-alignment-2026-09-27.md) starts with the user's kickoff and defines one fresh context window per plugin: Harvester, the six characters, Investigator, Fixture Boxscore, and Graph last. Editor is being pruned and has no separate audit window. This contract describes the target; remaining legacy code and historical documents do not redefine it.

## Ownership

- **System 1 scores:** cheap probabilistic support for plugin-defined relevance and eligibility predicates. Laya is the current Harvester classifier. Plugins define its predicates and apply policy to its signals and uncertainty. Use deterministic checks where sufficient; do not use generative inference when a cheaper System 1 mechanism performs the decision reliably.
- **Plugins frame:** select evidence and relevant memories, scope tools, compute or obtain supported measurements, define admissible claims and scores, prepare output structure and voice, and validate publication. A policy instruction in a prompt does not replace an enforced boundary.
- **SmolLM3 articulates:** answer “Given this reality, how should it sound?” within the prepared facts and qualifications. Model output is never authoritative evidence by itself.
- **Studio and the application host:** provide inference, validation machinery, scoped capabilities, budgets, claim fencing, atomic publication coordination, durable dispatch, and dependency assembly. Plugins retain domain policy and product writes.
- **Postgres stores the world; DuckDB studies the world:** Postgres preserves source evidence, identity, relationships, products, continuity, and work. DuckDB computes bounded, dated findings from those records. Plugins select relevant studied memory as context; SmolLM3 articulates it. Prior prose may supply continuity but cannot become new measurement evidence.

Internal plugins need no articulation call when they have no language product. Harvester and Fixture Boxscore do not need a character voice. Remaining generative classification or factual adjudication in other plugins is an alignment gap to resolve explicitly.

## Harvester supplies source text

Harvester replaces the legacy Editor's article-summarization role with source acquisition, System 1 filtering, and verbatim context extraction. It does not generate a summary or editorial packet for the characters.

The current checked-in worker follows this path:

1. Retain Google candidate provenance, query entity, headline, and publisher identity. Shared `plugins/meta.rs` supplies canonical name, entity ID, type and sport. Laya scores explicit headline reference to that supplied identity before publisher acquisition.
2. Apply Harvester's reading policy. Fetch or reuse usable publisher text only when at least one query entity passes. Acquisition failures remain acquisition outcomes, not negative relevance judgments.
3. Select the publisher's first three available paragraphs verbatim, with body hash and UTF-8 byte offsets. Score every retained non-whitespace character through windows of at most 100 words and 1,200 bytes. Openings requiring more than eight windows fail visibly.
4. Laya returns seven native scalar predicate scores. Harvester aggregates support across windows and applies its route table to select Journalist, Influencer, Insider and Scout. Source evidence stays separate from subject metadata; no model-selected destination or generated summary is accepted.
5. Persist exact source context, scores, window coverage and versioned policy under the queue claim. The harness dispatches approved destinations subject to shadow mode and character enrollment. Receiving plugins own product sufficiency and articulation.

The current local contract is `harvest-context-v7`, with `harvest-headline-v3` gates. Reading uses a **0.25** threshold; theme predicates use **0.50**, except performance at **0.70**. These are provisional development policy values, not calibrated accuracy claims. See the [v7 frame and evaluation](docs/harvester-frame-2026-09-27.md) and [source boundary](docs/harvester-plugin-boundary-2026-09-27.md). Google descriptions do not substitute for publisher text. “Verbatim” refers to retained extracted text, not raw HTML.

Harvester has one worker/replay path. The historical packet compiler, choice-response parsing and teacher fields are removed. The [cleanup pass](docs/harvester-cleanup-2026-09-27.md) records other retired tools. Shared subject metadata is available to the other plugins; only Harvester has been migrated in this phase. The user accepts the current 90/96 development result for now. [Further calibration is deferred](docs/harvester-calibration-follow-up-2026-09-27.md) until experience warrants it; operational deployment remains separate.

Older cutover documents describe three-sentence excerpts, broad forwarding, or advisory-only relevance. Those descriptions are historical and do not describe this worker contract. Deployment reports are also dated evidence: verify the actual host revision, migrations, flags, and worker state before claiming the checked-in behavior is live. This README update does not deploy code or release character delivery.

## Character preparation and articulation

**Relevant identity + selected evidence + relevant memory + output structure + character voice + factual boundaries.**

**Each file in a plugin owns one task.** Keep the context package modular:

| File | Owns |
| --- | --- |
| `meta.rs` | Canonical entity identity. |
| `fresh.rs` | Newly fetched reporting, its attribution and publication time. |
| `memories.rs` | Selection and presentation of relevant studied history from the relational world. |
| `form.rs` | Output structure only: fields, types, counts, limits and structural parsing. |
| A voice file such as `journalist.rs` | Tone. |
| `prompt.rs` | The sole articulation instruction and assembly manual for the prepared inputs. |

These are responsibility boundaries, not a requirement to duplicate shared files in every plugin. The memory package contains prepared findings, rather than asking the model to query history, calculate comparisons or decide whether reporting is valid. Articulation calls are stateless and memory-informed: the plugin supplies historical context for each call, rather than asking the model to evolve a story or carry prior conversational state. Full retrieval provenance stays with the plugin. The [live memory inspection and DuckDB trial](docs/journalist-memory-world-2026-09-28.md) records the existing matrix, measured study performance and the remaining integration work.

| Character | Plugin-selected material |
| --- | --- |
| Journalist / Narratives | Attributed source developments, their dates, and supported continuity. |
| Influencer / Vibe | Observed emotional evidence, its speaker/group scope, and relevant dated memories. |
| Scout / Rating | Prepared statistics, compatible comparisons, trends, sample coverage, and limitations. |
| Insider / Transfers | Sourced relationship and transaction states, uncertainty, and relevant history. |
| Analyst / Momentum | Finished Scout and Influencer findings plus supported trajectory studies. |
| Oracle / Sigil | Selected findings from the other five finished products and explicit component availability. |

The plugin constructs the bounded environment before the model call. Identity, dates, evidence scope, uncertainty, and missing-input status must survive articulation. Missing stays unknown; measured zero and observed absence are distinct findings. Direction requires a supported comparison. Compression must not erase a qualification or imply causation the evidence does not support.

[`form.rs`](src/plugins/support/form.rs) supplies shared publishing form. Existing publisher paths also use [`palette.rs`](src/studio/palette.rs) to constrain choices among prepared phrasings. These are current mechanisms, not proof that every source selector, score, or upstream gate meets this architecture. Some source adapters still ask generative models to make eligibility or factual decisions, and legacy packet/evaluation paths remain. The alignment plan audits and removes those gaps one plugin at a time.

The [Journalist Window 2 completion](docs/HANDOFF-journalist-finish-2026-09-28.md) uses one natural articulation stage over a plugin-prepared observation package. Shared `form.rs` supplies keyed structure without content direction; `journalist.rs` contains tone only. `journalist/cognition/prompt.rs` owns the articulation task and explains how the pieces actually present fit together. Fresh-only calls receive the compact source-to-key instruction. When studied history is present, the manual identifies `identity`, `history`, `fresh`, `voice` and `form` and locates history inside the report text it contextualizes. It does not ask the model to discover a story or significance.

Fresh source presentation lives in `journalist/cognition/fresh.rs`: fetched reporting, attribution, publication time and a request-local output key. SmolLM3 returns report text only. The plugin derives titles from complete source openings and the edition headline from the first selected title. Historical reporting remains distinct from fresh evidence and retains dated attribution, population window and publisher counts. Provenance and scoring stay outside the writing context. The n94 no-thinking replay passed manual fidelity review across fresh, conflicting, qualified, ordered, source-instruction and useful-memory cases; exact requests and outputs are retained under `fixtures/journalist/`. No generative claim-preparation or prose-judging stage was adopted. Commit `573d6a8e` was deployed on `archbox` on September 28, 2026; the API reported that revision, database health passed, and the API and cognition services plus their path watchers were active.

Finite phrasing selection is not the definition of articulation. Evaluate whether each plugin gives SmolLM3 enough prepared context and expressive latitude for natural, concise, character-consistent language while preserving factual fidelity. Any replacement must enforce its factual interface and demonstrate its value on representative cases.

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

`src/plugins/<name>/` owns each plugin's manifest, preparation, cognition where needed, publication adapter, and tests. `src/plugins/support/` supplies shared publishing form, guards, and resource profiles. The [fleet](src/application/fleet.rs) still includes legacy Editor until its removal; registration is not an endorsement of its target role.

`src/studio/` holds generic inference sessions, generation envelopes, and plugin/tool contracts. `src/application/` assembles concrete dependencies and capability brokers, with generic durable work transport under `application/queue/`. `src/evidence/` holds shared concrete loaders. `src/runtime/` holds configuration, database connections, routing, and providers. `src/evaluation/` and the `eval` binary contain checks and replays whose legacy paths are included in the alignment cleanup. `statcommentary` and `factsweep` are explicit non-queue entry points into their owning plugins.

Harvester enrollment requires its additive migrations, `HARVESTER_INGEST_ENABLED=1` in Go ingestion, an explicit `COGNITION_STAGES` list containing `harvester`, and `HARVESTER_MODEL_ENDPOINT`. Shadow mode and `HARVESTER_DELIVERY_CHARACTERS` control delivery separately. Do not infer deployment readiness from compilation or from a historical shadow report.

Use [development guidance](../run_docs/DEVELOPMENT.md), the [runbook](../run_docs/RUNBOOK.md), and [analytical acceptance](../run_docs/RECOVERY_ANALYTICS_ACCEPTANCE.md) for repository operations. The earlier [plugin architecture plan](docs/plugin-architecture-plan.md) records host separation; [Harvester cutover verification](docs/harvester-cutover-verification-2026-09-27.md) records dated operational evidence. The [alignment plan](docs/PLAN-plugin-alignment-2026-09-27.md) governs the current responsibility audit and its handoffs.

## Adding a plugin

Add a package under `src/plugins/<name>/` with its manifest and adapter, plus cognition only where inference is needed. Register its manifest in `application/fleet.rs` and bind concrete dependencies in `application/plugins.rs`. The registry validates task ownership, route/grant consistency, and resource declarations at boot.

Define context, tools, structure, memory, voice where applicable, and factual boundaries before adding a model call. Add a database migration only for genuinely new persisted domain data. Prefer removing duplication to adding a workflow language, compatibility layer, or new abstraction.
