# Studio: the architecture contract

**One harness runs independently scheduled skills.** The harness runs durable work. Capture owns Classifier, Investigator and Fixture Boxscore. Characters owns the six output skills. Skills bring tools and instructions; the LLM discovers what the supplied clues support and articulates the answer. Cheap code owns collection; expensive compute owns discovery.

A **voice is a Character skill**: Journalist, Influencer, Insider, Scout, Analyst or Oracle. All six belong to `scoracle.character`. Each voice owns its instructions, tools, validation and publication behavior under `skills/skill-<voice>/`. Adding or tuning a voice changes a skill within that plugin.

Plugins assemble dependencies and declare their skills. Each skill retains its own task, claim policy, resource budget, model routes and tool grants. The registry validates ownership and flattens enabled skills into the existing scheduler. Shared plugin ownership does not combine permissions, serialize character work, or change publication fences. See the [migration plan and inventory](docs/PLAN-plugin-skills-2026-11-15.md).

New generation-ledger rows use `scoracle.character` as `plugin_id` and the unchanged `stage` to identify the skill. Historical rows retain their former per-character IDs. Skill IDs preserve those old spellings for source receipts and input compatibility; no historical rows are rewritten.


## Package layout

Cargo builds the worker from `src/main.rs` and the operator commands from `src/bin/`. Runtime modules live under `src/`. Each directory under `skills/skill-*/` owns its instructions and implementation. Capture and Characters are registered in `src/registration/`.

Unit tests live beside the code they exercise. Integration checks live in `tests/`. Cargo discovers public-interface tests there directly. Some database and recovery checks exercise private implementation details. Their owning modules compile the files under `tests/internal/` only for test builds. Shared evaluation and replay inputs live in `fixtures/`.

From the repository root, run `cargo test --manifest-path harness/Cargo.toml --all-targets` for the ordinary suite. Use `scripts/check.sh` with an empty disposable PostgreSQL 18 database named `classifier_test_coverage*` for the full CI suite. The worker requires at least three connections in `COGNITION_DB_MAX_CONNS` for its listener, outbox transaction, and event reaction.

## Plugin and skill map

Paths below are relative to the package root. Each skill package contains `SKILL.md` and its Rust tools in `scripts/`. The empty `skills/graph.rs` reserves a place for future relationship capture. These are statically linked Rust skills, not dynamically loaded prompt folders. Each skill owns its preparation, instructions, allowed tools, validation and publication; the harness owns claims, retries, shared budgets and transaction fencing.

| Plugin | Skill | Durable queue task | Source directory | Output / effect |
| --- | --- | --- | --- | --- |
| Capture | Classifier | `classifier` | `skills/skill-classifier/` | Headline scores and character delivery obligations |
| Characters | Journalist | `narratives` | `skills/skill-journalist/` | News narratives |
| Characters | Influencer | `vibe` | `skills/skill-influencer/` | Coverage tone |
| Characters | Scout | `rating` | `skills/skill-scout/` | Statistical assessment |
| Characters | Insider | `transfers` | `skills/skill-insider/` | Movement reporting and findings |
| Characters | Analyst | `momentum` | `skills/skill-analyst/` | Performance and mood trajectory |
| Characters | Oracle | `sigil` | `skills/skill-oracle/` | Synthesis of the five character pillars |
| Capture | Investigator | `investigate_entity` (unavailable) | `skills/skill-investigator/` | Retained entity-maintenance machinery |
| Capture | Fixture boxscore | `fixture_boxscore` (unavailable) | `skills/skill-fixture-boxscore/` | Retained fixture-data machinery |

See the [Classifier request](skills/skill-classifier/references/sample-request.json) and [Journalist prompt](skills/skill-journalist/references/sample-prompt.json).

Classifier routes evidence to character work. Rating and Vibe feed Momentum; Oracle reads finished cards in continuous mode and waits for its five dependencies in scheduled editions. The harness claims each skill independently using the existing queue and dependency gates. Plugins introduce ownership, not another queue or a model choosing the next job.

`COGNITION_STAGES` still accepts queue task names. Unset enables the six Character skills; Classifier is opt-in. The shipped service explicitly enables Classifier plus those six skills. Investigator and Fixture Boxscore remain outside the worker roster, and `factsweep` refuses before database access. There is no factsweep cron job.

[Compare the old and new flow in Figma](https://www.figma.com/board/Ja3MlbUwEDby5GpRkx9GVs). The [baseline comparison](docs/qualification/2026-11-15-plugin-skills/BASELINE-COMPARISON.md) records unchanged fixture outcomes; prompt/parser quality is separate from structural parity.


The [governing wiki contract](../../scoracle-wiki/wiki/Architecture/Harness,%20Plugins,%20Tools,%20and%20LLM%20Contract.md) owns this platform boundary. Rust prepares evidence and executes model assignments; Postgres stores evidence, work and products. Models receive prepared input and never call tools, search or retrieve.

## Reporting pipeline

**Hourly team RSS searches → known entity extraction → decision-model headline scoring → daily character work → saved products → client API.**

The Go collector searches team entities. Rust scores the query subjects and known teams, players and people named in the returned headlines. The existing sport-scoped name index resolves unique canonical names and stored aliases. Coaches, agents, owners, executives, officials, family and other known figures retain the canonical `person` type; IDs remain separate from player and team IDs. Ambiguous names stay unresolved. Unknown-person discovery and registration are a [subsequent session task](docs/headline-entity-pipeline.md#follow-up-unknown-entities).

Classifier sends the headline, publisher and canonical subject metadata to d3 Q8_0 through llama.cpp's `/v1/systemone`. Its four scores are subject relevance, emotion, movement and availability. Relevance ≥55 selects Journalist; Influencer also requires emotion ≥15, Insider movement ≥80, and Scout availability ≥85 for teams/players. Routing policy `headline-routing-v3-identity` lowers the previous 90 cutoff: the user selected 85 and confirmed inclusion of owner arena bans. It admits the observed practice update while excluding the saved transfer-only calibration false positive. It still needs human-reviewed held-out validation. Scores route headline evidence; they do not establish facts, corroboration or relationships.

Obvious decorated livestream solicitations are rejected by a shared Unicode-normalized headline guard. Ordinary broadcast guides and availability reporting remain eligible. New decisions retain their raw scores and an explicit spam rejection; already queued spam is settled without writing-model inference or an edition blocker, and it cannot return through headline memories. The archive is preserved. Plain-text spam is outside this narrow guard. New model questions use `classification-criteria-v17-shared-relations`; historical v9/v10 receipts remain readable; routing has its own version in source identity and gates. Existing scores are not silently rerouted: changing the cutoff affects new decisions, while backfilling current retained headlines is a separate bounded operator operation.

Explicit team gender is read from `teams.meta.gender` (`men`, `women`, `mixed`, or missing/null) into the decision context and structured target metadata. Missing gender is not inferred from names. The [d3 gender trial](fixtures/classifier/team-gender-2026-11-01/README.md) verifies the local request change; real metadata population, the earlier candidate-filter boundary and deployment remain outstanding.

Classifier performs no publisher fetch, article extraction, browser rendering or full-text scoring. Harvester/Editor replay implementations, browser-reader runtime and the `classifier_acquire` worker are retired. Archived article evidence, decisions and products remain stored.

## Character packages and publication

All six characters receive `task` → `target_context` → `target` → `evidence_context` → `evidence` through [shared prompt assembly](src/tools/prompt.rs). Each skill's `character_instructions.rs` owns its role, voice, task and output rules. [Meta](src/tools/meta.rs) and each skill's `fresh.rs` supply their data together with relationship guidance and explicit `(field_path, meaning)` definitions. Skill `request.rs` prepares both components; the assembler has no database-record dependency. It includes definitions only for supplied fields, preserving zero and false while treating missing/null as unknown and empty collections as no items. There is no trailing tone field or separate `context.rs`/`context.txt`. Character `request.rs` modules retain schemas, preparation and transport options. Production and fixture replay share the assembly path; the decision model also supplies context with its rules while retaining its native scoring protocol.

During backlog debugging, all article-consuming characters receive at most ten articles ranked by the decision model: `subject` for Journalist, `movement` for Insider, `emotion` for Influencer and `availability` for Scout. The shared Rust cap and SQL selection policy agree; ranked exclusions keep explicit receipts and are never labeled model-used. Ties retain incoming order; absent/invalid scores follow valid scores, including zero. Full selected source wording, publishers and qualifications remain. Article IDs, URLs and publication timestamps stay in internal records and receipts; the writing payload omits them. Dates within source text and dated metadata/measurement windows remain. The [clear-payload trial](fixtures/characters/clear-payload-2026-11-02/README.md) records the readable JSON layout and its controlled comparison. The [direct-instructions trial](fixtures/characters/direct-instructions-2026-11-02/README.md) records the plain Journalist, Insider and Influencer briefs. Journalist writes sourced paragraphs first, then one hook for the recap, using request-local integer IDs for citations. Insider and Influencer also write as many concise paragraphs as the evidence supports; Influencer follows the reading with one overall vibe score and one hook. The [ten-article follow-up](fixtures/characters/insider-influencer-2026-11-02/README.md) records the current limit and the Insider/Influencer review. The [five-article trial](fixtures/characters/top-five-2026-11-02/README.md) records token savings and remaining factual failures. The [shared-prompt refactor](fixtures/characters/shared-prompt-2026-11-02/README.md) records the initial shared-assembler comparison. The [context-component refactor](fixtures/characters/context-components-2026-11-02/README.md) removes voice files and prose parsing while preserving complete requests. [Prompt ownership and tuning](docs/character-prompts.md) explains where to make changes and how new plugins reuse the structure. These are development results, not held-out accuracy findings. Historical context remains deferred. D3 routing does not verify reporting.

The [context-guide trial](fixtures/characters/context-guide-2026-11-02/README.md) compares this template with the previous job/target assembly on identical captured evidence. Historical context remains deferred.

The [job, target and fresh-context assembly pass](fixtures/characters/job-target-fresh-2026-11-02/README.md) includes real structured identity captures, a Chelsea request preview and a 24-generation comparison across all six characters. The assembly is implemented locally; semantic output quality remains mixed. Historical article and generated-card context remains deferred.

The [useful-perspectives implementation and evaluation](fixtures/characters/useful-perspectives-2026-11-01/README.md) records the current local candidate, all six roles, representative real-data replays, source review and processing costs. This product direction supersedes the perfection gates and proposed next experiments in the historical investigations below. Those artifacts remain diagnostic evidence; the per-article preparation pipeline is not adopted. The candidate is not deployed, and parser acceptance does not establish semantic quality or daily capacity.

The [bounded Insider index trial](fixtures/characters/insider-indices-2026-11-01/README.md) tests twelve new packages in a 24-call comparison. Explicit indices fix three wrong-source references, but introduce material meaning regressions and do not improve complete-card acceptance. The variant is retained as an experimental patch; the pretrial runtime input is restored.

The subsequent [context and instruction review](fixtures/characters/context-review-2026-11-02/README.md) audits all six request contracts and iterates on Insider in local batches. That Insider trial used complete worked examples and limited finding candidates to literal full-name quote eligibility; the later five-article trial shortened the instructions while retaining that eligibility rule. This removes six unsupported findings in the twelve-case comparison, but recurring meaning errors and parser limitations remain; routine output quality is not established.

The Journalist three-context pilot is not qualified for rollout. Its current [instruction contract](skills/skill-journalist/SKILL.md) defines source ownership, uncertainty, story selection and output rules. Runtime, hashing and evaluation read the same contract; the historical trials below used longer instructions with worked examples. The [detailed-instruction trials](fixtures/journalist/detailed-instructions-2026-11-01/README.md) retain development, held-out and workload results. The earlier [assembly investigation](fixtures/journalist/assembly-2026-11-01/README.md) traces the removed legacy claim-preparation stage. Parser evidence matches the exact headline or excerpt served to the writer; passing that check still does not prove factual fidelity.

The [real-data packet-size experiment](fixtures/journalist/packet-size-2026-11-01/README.md) freezes three production-loader packets with database article IDs and tests 1/4/12/40 headlines twice on the unchanged 4B model. All 24 generations complete and all repeats match, but no size is reliably faithful in this sample: short inputs still resolve ambiguity, while larger inputs introduce unsupported relationships and source-link errors. Manual fidelity, canonical-name parser rejections and timing are recorded separately.

The [topic-grouping follow-up](fixtures/journalist/topic-grouping-2026-11-01/README.md) preserves the same source objects through reviewed and model-generated groups. All-40 grouping does not fix fidelity; automatic grouping is semantically wrong and adds processing cost. Serving only three complete reviewed groups improves selection, but only one of three subjects is fully faithful. Statement-level source ownership remains unresolved; this does not qualify an extra grouping stage or establish daily capacity.

The [statement-ownership experiment](fixtures/journalist/statement-ownership-2026-11-01/README.md) compares ordinary prose, stronger instructions and source-bound statements in 36 calls on the same 4B. Structured statements fix specific attribution and fixture-completion errors with one call and similar median latency, but still invent meanings for ambiguous details and produce title/readability failures. All source mappings are valid; factual reliability remains unqualified. Runtime instructions and parser are unchanged.

The [worked-example calibration](fixtures/journalist/statement-calibration-2026-11-01/README.md) adds 30 real-data calls, including three new subjects. Examples improve some qualifications and topic coverage, but still import metadata into claims and fail grammar/agency checks. A missing-detail annotation field loses topics. The examples add 1,013 input tokens per call without another inference stage; neither candidate is promoted, and daily capacity remains unmeasured.

The [metadata and serialization investigation](fixtures/journalist/production-order-2026-11-01/README.md) runs 45 further calls across context layouts, lossless typed metadata and three labelled counterfactual diagnostics. It finds that recursive fixture-key sorting changed production field order; the corrected builder preserves captured order and joins sources by durable article ID. That restores the tested speaker but does not resolve whole-output fidelity. Metadata can affect attribution, while amount interpretation and source-ownership errors also survive without background. Future comparisons should preserve the production wire format. Production remains unchanged; the three new-subject packets remain unused because no candidate qualified.

The [source-preparation investigation](fixtures/journalist/source-preparation-2026-11-01/README.md) separates each source's subject, speaker, claim and qualifications before writing. Across 104 calls, the retained worked-example writer reaches 6/6 faithful packets with manual notes; automatic notes reach 4/6 clear passes plus one title concern, versus 2/6 in that control. Valid copied spans do not prevent semantic errors. Automatic preparation plus writing costs 3.65× direct writing on these packets. No production promotion or new-subject qualification follows; all failures and costs are retained.

The [preparation meaning/order follow-up](fixtures/journalist/source-preparation-2026-11-01/semantic-preparer/README.md) adds 70 calls on the same 4B and real-data packets. Expanded instructions and claim-first emission reach 3/6 and 2/6 clear packets respectively, failing the reliability gate despite some individual corrections and reduced duplication. Single-source preparation can itself invent facts; final writing also retains independent errors. Combined cost is 178.14 and 157.61 seconds versus the historical 44.38-second direct control. Total source-preparation evidence is 174 calls; production and unused validation remain unchanged.

The [copied-evidence follow-up](fixtures/journalist/source-preparation-2026-11-01/copied-evidence/README.md) adds 41 calls and reaches 5/6 clear packets with the unchanged writer. All 29 preparations copy every text field from their own source, but one final packet still borrows facts across reports. Preparation plus writing costs 156.49 seconds versus 46.59 seconds for a same-session complete-headline-copy control, which reaches 2/6 clear packets. The remaining factual failure blocks validation/promotion. Cumulative source-preparation evidence: 215 calls, 1,145.725 seconds; production remains unchanged.

The native generation client enables matching-prefix reuse to avoid recomputing unchanged instructions. Every request still supplies the full token list and retains exact input, context, model-identity and completion checks. Sequential and four-slot comparisons are retained with the detailed-instruction trials; they do not establish full-night throughput or factual reliability.

- Journalist groups supported fresh headlines into up to three narratives, retaining exact contributing report keys.
- Influencer interprets expressed reactions and the emotional tone of supplied coverage, or abstains. Its score estimates coverage tone, not population sentiment.
- Insider produces an attributed movement reading with exact headline quotations for supported findings. Co-mentions alone do not establish a move.
- Scout explains supplied measurements and limits, with a separate verbatim attributed availability-only headline section. Code owns arithmetic and scores.
- Analyst relates performance and coverage tone, exposing up to two existing dated comparisons when available. Notability is statistical distinctiveness, not skill; without comparisons the reading stays with the current balance.
- Oracle synthesizes the five available finished perspectives and their tensions. Original card dates are retained; repetition across cards is not independent confirmation. These statistical stages run for teams and players; people receive the reporting products.

Fresh copies from the same publisher are supplied once, regardless of RSS URL or routing scores. Every delivery still receives a durable outcome. Feed rank controls queue ordering and cannot make unchanged evidence fresh again.

Historical article and generated-card prose injection remains deferred. Character loaders do not fetch headline memories. Legacy memory fields and helpers remain for old captures and explicit offline investigations; they do not contribute to the new writing inputs. Analyst's dated computed comparisons and current finished peer cards are distinct from historical prose injection.

The recovery candidate closes collection at 22:00 America/Detroit and freezes daily assignments at midnight after every captured classification obligation settles. Late drain and carry-forward are proposed cutover policies. Unresolved work retains the edition indefinitely; only a qualified complete edition changes serving visibility. Queue leases, current-source validation and claim-fenced transactions protect retries and publication. Reverting a headline to an archived revision reconciles its current pointer and outstanding obligations. Completed evidence is not republished merely because its discovery candidates change.

The daily path is D3-selected evidence → Narratives, Vibe, Scout and Transfers → Analyst → Oracle. Eligible changed packages receive at most one generation per execution attempt, without routine preparation, judges or rewrite calls. Existing unchanged-product reuse and durable queue retries remain; one attempt is not a lifetime one-call guarantee. Complete inputs must fit the 7,900-token request budget, including instructions and output reservation. Oversized packages are held, never silently clipped. Output parsers retain structural checks, source mappings, attribution boundaries and served-prose guards; some conservative guards still reject useful wording. Edition completion policy is unchanged.

## Serving

The API reads saved products and performs no inference. The reporting routes `/news`, `/vibe`, `/transfers`, `/headlines` and identity route `/meta` accept `team`, `player` and `person`. Reporting reads accept `date=YYYY-MM-DD`; the default selects each entity's latest qualified version, preserving original dates under the explicit carry-forward policy. Statistical routes remain scoped to teams and players. Weekly reporting filters and `/weeks` are retired.

## Ownership and source map

| Owner | Responsibility |
| --- | --- |
| `src/registration/` | Capability ownership and skill assembly. |
| `skills/skill-<name>/` | Evidence selection, task, voice, inference, acceptance and publication policy. |
| `src/` | Registration, model transports, budgets, queue claims, coverage schedule, diagnostics and atomic publication coordination. |
| `src/tools/` | Shared identity, source presentation, form decoding and output guards. Network fetch tools remain for structured Investigator and box-score work. |
| Postgres / SQL | Canonical identities, exact name matching, source revisions, spectra, delivery obligations, daily membership, factual arithmetic and stored products. |
| `src/evaluation/`, `examples/`, `fixtures/` | Replays, contract checks and retained evaluation evidence outside production execution. |

Share ordinary tool functions when doing so reduces complexity. Plugins assemble clues rather than prewrite conclusions. Preserve dates, uncertainty, unknown values, publisher attribution and complete evidence. Historical prose cannot become measurement evidence. Keep model requests and responses inspectable outside compact character inputs.

`tools/meta.rs` provides canonical identity; `tools/fresh.rs` presents evidence; `skills/skill-classifier/scripts/memories.rs` selects and ranks bounded headline history in Rust; `tools/form.rs` owns structural forms and decoding; `tools/guards.rs` owns served-prose checks. Other DuckDB studies remain available for explicit offline work.

The six Character skills retain their distinct interfaces and share one production Qwen route. `statcommentary` remains an explicit statistics command. Investigator uses structured Wikimedia evidence; `factsweep` remains unavailable pending a qualified discovery gate.

## Activation and verification

The [d3 qualification and rollout](../docs/progress_docs/2026-10-27-d3-classifier.md) records the pinned Q8_0 model, v11 prompt selection, held-out limitations, migration 325 and rollback procedure.

The [October 20 release record](docs/headline-qwen35-deployment-2026-10-20.md) documents deployed migration 315 and matching Go/Rust binaries on both hosts. The [all-character Qwen release](docs/qwen-character-production-2026-10-20.md) pins all six character routes to one Qwen3.5-4B server with four slots, a 7,900-token request budget and evidence-only prompts. Known factual-output defects remain documented in the release record. See [entity pipeline changes and follow-up](docs/headline-entity-pipeline.md).

The [pipeline guard fixes](docs/pipeline-guard-fixes-2026-10-20.md) add conservative identity and source checks, explicit missing-peer readings, exact-token holds and verified memory-helper startup.

For a read-only model replay: `CLASSIFIER_URL=http://127.0.0.1:18081 cargo run --release --example classifier_replay -- cases.jsonl 4`. Cases supply `id`, `sport`, `headline`, `publisher` and canonical `meta`.

Use [development guidance](../run_docs/DEVELOPMENT.md), the [operations runbook](../run_docs/RUNBOOK.md), and `scripts/check.sh` with an empty disposable PostgreSQL 18 database. Database checks exercise publication and recovery; live-model replays separately assess evidence fidelity and writing quality. Historical deployment notes and article benchmarks describe their dated implementations.

The runtime registers Classifier, Character and Caretaker plugin owners. Only Classifier and the six Character skills can be enabled; Caretaker remains inactive. One shared Qwen client and
four aggregate running character claims replace per-role production routing. Headline-history
ranking is a bounded Rust sort; DuckDB remains available to offline analytics. Existing
Investigator/fixture modules live under Caretaker for their retained callers; factsweep still refuses execution.

See [recovery validation and remaining release gates](../docs/progress_docs/2026-10-22-production-recovery.md).
