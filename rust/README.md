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
- **Postgres remembers; analytical code and DuckDB studies compute:** preserve source evidence, identity, relationships, products, continuity, and work; compute bounded comparisons and trends with their meaning and limitations. Prior prose may supply continuity but cannot become new measurement evidence.

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

Harvester has one worker/replay path. The historical packet compiler, choice-response parsing and teacher fields are removed. The [cleanup pass](docs/harvester-cleanup-2026-09-27.md) records other retired tools. Shared subject metadata is available to the other plugins; only Harvester has been migrated in this phase. Real-world route calibration and release remain open.

Older cutover documents describe three-sentence excerpts, broad forwarding, or advisory-only relevance. Those descriptions are historical and do not describe this worker contract. Deployment reports are also dated evidence: verify the actual host revision, migrations, flags, and worker state before claiming the checked-in behavior is live. This README update does not deploy code or release character delivery.

## Character preparation and articulation

**Relevant identity + selected evidence + relevant memory + output structure + character voice + factual boundaries.**

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

Finite phrasing selection is not the definition of articulation. Evaluate whether each plugin gives SmolLM3 enough prepared context and expressive latitude for natural, concise, character-consistent language while preserving factual fidelity. Any replacement must enforce its factual interface and demonstrate its value on representative cases.

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
