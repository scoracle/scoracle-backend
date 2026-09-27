# Plugin alignment plan

## Kickoff — the user's opening directive

> **THIS IS AN ALIGNMENT SESSION. USER HAS MOVED AWAY FROM A PROMPT-BASED APPROACH TO SYSTEM 1/LLM RESPONSIBILITIES. THE TARGET GOAL IS:**
>
> **SYSTEM 1 FILTERS**
>
> **PLUGINS FRAME**
>
> **LLM ARTICULATES**
>
> **THIS SESSION WILL GO THROUGH EACH OF THE PLUGINS (STARTING WITH HARVESTER, PROGRESSING THROUGH THE CHARACTERS, AND ENDING WITH GRAPH) AND AUDIT THESE SO THAT THE PLUGIN PROVIDES THE CONTEXT, TOOLS, STRUCTURE, MEMORY, VOICE, AND BOUNDARIES. THE LLM (SMOLLM3) PROVIDES THE ARTICULATION.**
>
> **WHILE OPTIMIZING FOR THIS APPROACH, IT'S CRITICAL OLD SCAR TISSUE, REDUNDANCY, AND UNNECESSARY LEGACY CODE BE PRUNED SO THE FINAL TARGT STATE IS LEAN. SLOP IS NOT ACCEPTABLE.**

Planning date: September 27, 2026. Planning and README correction are complete. Harvester phase one is implemented and locally verified; production release and classifier calibration remain open. This is the governing alignment plan; older architecture and cutover documents remain evidence of earlier work, not competing target contracts.

## Ownership contract

**System 1 filters reality. Plugins frame reality. LLM articulates reality.**

The user's clarification is binding: **neither System 1 nor the LLM builds the decision world; plugins do. The LLM's job is articulation.** The plugin determines **WHAT can be said**; SmolLM3 determines **HOW it is said**. The model receives a bounded, prepared world. It must not determine what is true, discover relevant facts, reconstruct missing context, or decide what information exists.

Within that world, articulation includes natural expression, coherent synthesis, character voice, choice of emphasis, and compression that preserves important meaning. The plugin supplies factual boundaries and required qualifications; the model has expressive latitude inside them. This is broader than choosing a fixed sentence from a menu, and narrower than inventing an interpretation unsupported by the supplied facts.

The question for SmolLM3 is **“Given this reality, how should it sound?”**, never **“What is reality?”** Evaluate every inference with: **“Did the model express the supplied world well without adding anything that was not there?”** Unsupported facts, statistics, events, relationships, and assumptions are interface violations, not an accepted consequence of model size.

- **System 1:** cheap probabilistic relevance, classification, routing, filtering, and downstream eligibility over plugin-selected inputs. Laya is the current Harvester classifier. It returns signals and uncertainty; plugin policy decides what those signals permit. Do not use the LLM for these decisions when a cheaper System 1 mechanism can perform them reliably. Deterministic eligibility checks stay in code. Unsupported classification capabilities must be measured or implemented explicitly; do not assume Laya already replaces every generative judgment.
- **Plugins:** own evidence selection, tool scope, product structure, memory selection, voice, permitted claims, scores, admission policy, and publication boundaries. A prompt located inside a plugin does not establish ownership if the model still chooses what is true, eligible, scored, or persisted.
- **SmolLM3:** articulates material already selected and bounded by the plugin. It must not serve as a second relevance filter, invent evidence, resolve canonical identities, compute scores, decide database facts, or promote its own prior prose into evidence.
- **Studio and the application:** provide generic inference, scoped capabilities, budgets, claim fencing, transactions, durable dispatch, and dependency assembly. Domain policy belongs to the owning plugin.
- **Persistence and studies:** Postgres retains evidence, provenance, continuity, products, and work. Analytical code and bounded studies compute measurements and comparisons. Prior interpretations may supply continuity; they cannot increase the evidence supporting a new measurement.

### Routing ownership

The harness executes routing; plugins own semantic eligibility. Harvester selects
source evidence, defines the bounded theme predicates and applies its versioned
policy to System 1 scores. Its output names eligible, registered plugin IDs and
retains the evidence and decision provenance. The receiving plugin owns whether
that material is sufficient for its product; a Harvester recommendation is not
permission to invent a product or a measured score.

The harness validates destinations and executes durable dispatch subject to
enabled destinations, budgets, deduplication, claim fencing and retry policy.
It must not interpret sports content or choose theme thresholds. Delivery controls
can suppress a recommendation; they cannot manufacture semantic eligibility.
These are ownership boundaries, not a requirement for another router abstraction.

Laya returns scores for supplied predicates. SmolLM3 articulates the downstream
plugin's prepared material. Neither model chooses arbitrary destinations. Keep
semantic policy out of the harness and operational dispatch out of model prompts.

Internal plugins may need no articulation call. Harvester and Fixture Boxscore should not acquire a voice just to make the architecture uniform. Investigator and Graph must expose their remaining generative extraction/adjudication responsibilities as migration gaps, not relabel them as articulation.

Existing finite phrasing palettes are an implementation to assess, not the definition of the target. Their structural guarantees are useful. Their claim selection, specificity, voice, repetition, and actual need for a model call still require review. Do not replace them with unconstrained generation or add another abstraction without representative evidence of a benefit.

Model evaluation priorities, in order: factual fidelity to supplied context; articulation quality; voice consistency; concise synthesis; speed and efficiency; general reasoning only where required. SmolLM3 is the selected articulation model. This plan does not reopen a general model contest: make the task narrow and the supplied world complete enough for the smaller model to express it well.

## One plugin per fresh context window

Use this single plan as the durable index. Start each plugin in a fresh context window with only its section, the shared ownership contract, and the immediately relevant handoff evidence. Do not run the ten audits in one accumulated conversation. The order is an audit order, not the worker's dispatch order.

| Window | Plugin / task | Scope | Status |
| --- | --- | --- | --- |
| 1 | Harvester / `harvester` | Intake, source extraction, System 1 filtering, delivery contract | V7 scalar frame and shared metadata implemented; development/DB checks passed; calibration/release pending |
| 2 | Journalist / `narratives` | Source-linked developments and reporting continuity | Not started |
| 3 | Influencer / `vibe` | Observed emotional evidence and mood articulation | Not started |
| 4 | Scout / `rating` | Measured performance, source triggers, statistical voice | Not started |
| 5 | Insider / `transfers` | Relationship evidence, transfer state, heat, identity obligations | Not started |
| 6 | Analyst / `momentum` | Scout/Influencer synthesis and supported direction | Not started |
| 7 | Oracle / `sigil` | Five-product synthesis, readiness, score, final reading | Not started |
| 8 | Investigator / `investigate_entity`, `factsweep` entry point | Canonical identity and metadata evidence gates | Not started |
| 9 | Fixture Boxscore / `fixture_boxscore` | Deterministic acquisition and measured-data boundary | Not started |
| 10 | Graph / `graph` | Source-bound relationships; close final retirement dependencies | Not started |

The current fleet in `src/application/fleet.rs` has eleven registrations. The user explicitly excludes Editor from this audit because it is being pruned, leaving ten plugin windows. Editor gets no redesign or standalone session. Remove its obsolete dependencies as the affected plugins migrate, recording any required cross-plugin remainder and closing it in the final Graph window. Account for surviving side effects before deleting their old implementation; this is cleanup within the owning plugin, not an Editor audit.

### Procedure for every window

1. **Establish current state.** Read the repository guidance, this contract, the plugin manifest, production caller, preparation, model calls, parser, writer, memory loaders, evaluation callers, and relevant SQL. Record the revision and distinguish checked-in behavior from verified deployment state. Trace every live path, including flags and fallbacks.
2. **Write the responsibility map.** For context, tools, structure, memory, voice, and boundaries, name the current owner and target owner. Account for every model call: inputs, actual decision, output consumer, cost, and failure behavior. Mark generative filtering and factual adjudication as ownership violations even when schema-constrained.
3. **Choose the smallest complete change.** Define the typed evidence and permitted output before touching prompts. Name the policy, unknown/abstention behavior, and concrete old path to remove. If a required classifier is not yet capable, record that gap and its evaluation work; do not silently substitute keyword heuristics or an unmeasured threshold.
4. **Implement and prune together.** Move responsibilities to their owners. Delete superseded prompts, correction loops, parsers, flags, DTOs, loaders, fixtures, examples, and configuration after migrating necessary callers. Inspect application, evaluation, Go/SQL, and client dependencies where relevant. Avoid permanent dual paths.
5. **Verify the boundary and the product.** Run focused contract tests, relevant isolated database tests, and bounded representative model evaluations where behavior changes. Test source tampering, wrong entities, missing material, uncertainty, and stale claims as relevant. Check useful coverage, specificity, voice, token cost, latency, and retries; parser success alone is insufficient.
6. **Leave a compact handoff.** Update this plugin's status and record what changed, deleted paths, retained dependencies with exit conditions, verification evidence, deployment state, and the next plugin's interface. Stop at that plugin's boundary. Continue unfinished work in a fresh window for the same plugin if necessary.

No plugin is marked complete while an unexplained legacy production fallback remains. A necessary temporary dependency must have a named consumer, removal condition, and owning later window. Separate code-complete, locally verified, and deployed/verified status. A deployment or data-removal step is not implied by a documentation update.

### Shared acceptance gates

- Every published claim traces to selected evidence or an explicitly defined computation. An exact quote proves containment, not entity relevance or semantic support by itself.
- Articulation may change wording, emphasis, and compression, but must preserve material meaning, attribution, uncertainty, and required qualifications. Unsupported invention fails the interface. Test factual additions and meaning lost through omission alongside naturalness and voice.
- Scores, entity IDs, source IDs, dates, units, and uncertainty cannot be changed by articulation. Missing stays unknown. Absence, measured zero, abstention, acquisition failure, and classifier failure remain distinct.
- Memory has an explicit selection rule, budget, freshness policy, and provenance class. Retain only memory that affects the permitted reading; do not inject history merely because it is available.
- Tools are scoped to the plugin's needs. Model/network work stays outside publication transactions; claim and input-revision checks fence effects. Product, provenance, and required follow-up intent commit together.
- Production and evaluation use the same active preparation and contract. Historical replay tools cannot silently define production behavior. Remove tests that only enforce retired behavior; preserve meaningful invariants on the replacement path.
- Every deletion is checked for callers and operational consumers. Do not remove source provenance, idempotency, publication fencing, or recovery receipts as cosmetic cleanup.
- Record actual before/after model calls, request/output size, latency, retries, and file/path removals where relevant. Do not introduce a new metrics framework just for the audit.

## Window 1 — Harvester

**Purpose:** turn ranked source candidates into attributed, unchanged publisher context for eligible consumers. Harvester does not summarize articles or create editorial products.

**Start with:** `src/plugins/harvester.rs`; `src/plugins/harvester/{adapter,cognition,context,policy,delivery,maintenance}.rs`; `src/runtime/providers/system_one.rs`; `src/evidence/fetch.rs`; Harvester assembly in `src/application/plugins.rs`; relevant ingestion and SQL consumers. Read `harvester-plugin-boundary-2026-09-27.md` and `harvester-headline-calibration-2026-09-27.md` for the current contract and calibration limits. Older cutover handoffs contain superseded broad-routing behavior.

**Observed baseline:** the worker evaluates Google headline/target relevance before fetching publisher text, then classifies four themes over a bounded opening. Plugin policy uses 0.25 for reading and 0.50 for theme routing. The retained character excerpt is the first three available paragraphs; Laya receives a bounded prefix. The historical `Harvester::harvest` packet API and its replay callers still coexist with the source-context worker.

**Work:**

1. Trace query provenance, canonical article selection, deduplication, headline gate, acquisition, paragraph extraction, theme scoring, assignments, and terminal outcomes. Confirm that RSS descriptions never replace publisher evidence and fetch failures never become relevance negatives.
2. Audit System 1 questions as predicates, with admission and routing policy entirely in the plugin. Verify probability validation, versioning, uncertainty, and source binding. Review missed-useful cases and sport/entity diversity before treating any threshold as calibrated.
3. Establish the lean source contract: source identity, dates, exact headline/opening, retained-body hash and offsets, selected entity, classification provenance, and delivery receipt. Keep audit-only payloads out of character articulation inputs. Decide whether the opening budget loses necessary context using representative articles.
4. Review web grants, fetch budgets, reuse rules, redirects, paragraph quality, duplicate work, retry behavior, and durable memory. Harvester memory consists of acquisition/provenance/disposition state; it has no editorial voice and needs no SmolLM3 call.
5. Migrate necessary offline/review callers to the same source-context preparation used by the worker. Remove historical packet compilation, `Classification`/compatibility plumbing, and teacher-baseline fields if caller analysis confirms they are obsolete. Preserve useful source-integrity tests on the surviving contract.
6. Account for maintenance and non-editorial Editor duties already adopted: links, dedup, nominations, fixture/Graph handoff, and scheduling. Define their source-bound contracts without expanding into the downstream plugin implementations.

**Verify:** admitted/rejected/uncertain headlines; wrong same-name entity; no fetch after rejection; failed acquisition; flat or short prose; paragraph and Unicode byte boundaries; source mutation; selective theme routing; no assignments in shadow; claim fencing and idempotent delivery. Run focused Harvester tests and relevant isolated DB checks. Evaluate reviewed usefulness separately from historical classifier agreement; provisional AI labels are not human gold.

**Current implementation:** the [v7 frame](harvester-frame-2026-09-27.md) replaces
the broad choice questions with seven native scalar predicates, a plugin-owned
route table and complete bounded coverage of the selected opening. Shared
`src/plugins/meta.rs` supplies subject name, type, sport and canonical ID; Harvester
renders identity in every question, separately from source evidence. Headline
scoring asks for explicit reference, not unsupported relationship reasoning. The
harness continues to dispatch only plugin-approved destinations.

**Cleanup and integrity:** the [cleanup](harvester-cleanup-2026-09-27.md) removed
nine retired tools, unused RSS input and duplicate route mapping. V7 also removes
the choice-answer/distribution contract. The [integrity repairs](harvester-integrity-review-2026-09-27.md)
retain exact source fencing, immutable receipts, complete SDK question coverage,
and compatibility for named pending delivery consumers. Same-body source revisions
still require an explicit new receipt revision when receipt identity collides.

**Evaluation status:** the frozen candidate matches 90/96 synthetic development
route expectations, with three false routes and three missed routes. This set was
used to refine metadata syntax and multisport wording; it is not independent gold.
Reviewed real-source calibration, headline-negative coverage and live release stay
open. Deploy the Python coverage adapter, Rust and Go intake producer together.
Nothing in this work enabled production character delivery.

**Exit:** one active acquisition/classification/context path, no editorial summarization, explicit provisional/calibrated policy status, verified provenance, and a precise handoff for Journalist/Influencer/Scout/Insider and Graph. List remaining external consumers before any broader retirement.

## Window 2 — Journalist

**Purpose:** articulate selected, attributed developments and their supported continuity.

**Start with:** `src/plugins/journalist/{manifest.rs,adapter/mod.rs,cognition/mod.rs,cognition/inputs.rs,cognition/brief.rs}` and their tests; narrative evidence/memory loaders; `news_summaries` publication and source links.

**Work:**

1. Trace Harvester assignments and the remaining packet/storyline corpus paths. Establish which source selection, deduplication, impact calculation, and no-material decisions are already deterministic and which still depend on model output.
2. Frame each permitted development with exact source IDs, entity, time, attribution, uncertainty, and supporting context. Decide when multiple reports corroborate one development and when they describe distinct events. System 1 may classify eligible material; the plugin chooses what enters the product.
3. Select continuity from prior sourced developments with dates and identity, retaining it only when it explains what changed. Remove packet framing and generated story dependencies once source-based continuity has a verified replacement.
4. Audit the current palette and source selection together. Define a concise reporting voice that preserves specificity and attribution. Remove unused open-prose builders/parsers and duplicate brief rules when their evaluation callers are migrated.
5. Keep source disposition, product publication, source junctions, and downstream completion intent fenced and atomic. Review tool grants; reporting should not acquire unbounded retrieval through articulation.

**Verify:** one development, duplicate reports, conflicting claims, outdated report, wrong entity, no new material, and several distinct developments. Check source IDs against every selected statement; compare specificity and useful coverage, not just valid palette choices.

**Exit:** a source-based reporting product with plugin-owned selection/impact/continuity, SmolLM3 confined to expression, and no hidden Editor packet dependency in the migrated path.

## Window 3 — Influencer

**Purpose:** articulate supported human emotion around the entity, with clear scope and uncertainty.

**Start with:** `src/plugins/influencer/adapter/{harvester,mod}.rs`; `cognition/{mod,inputs,brief}.rs`; sentiment/memory loaders and tests.

**Work:**

1. Audit the generative `decide_reaction` gate and subsequent source-card call. Move eligibility filtering to a measured System 1 predicate and plugin policy; do not count a SmolLM3 boolean response as articulation.
2. Define the evidence unit: who reacted, to what, when, quoted support, and whether it represents one speaker or a broader observed reaction. Neutral scheduling, reporting tone, and predicted reactions must not become observed mood.
3. Own register, score, evidence sufficiency, and abstention before articulation. Audit packet-register scoring and the no-current-material/previous-score fallback so old sentiment cannot masquerade as a fresh observation.
4. Select dated continuity without letting old cards create new emotional evidence. Give the character an expressive voice within the observed scope; do not impose a crowd-wide mood from one quote.
5. Remove superseded reaction prompts/corrections, packet blocks, legacy loaders, and duplicate source/card paths after caller migration. Keep article disposition and product provenance atomic.

**Verify:** explicit cheering, criticism, a single quoted feeling, neutral administrative news, predicted excitement, conflicting reactions, stale memory, and empty input. Measure missed useful reactions and false mood claims as well as latency and the number of model calls.

**Exit:** filtering and sentiment policy precede articulation; every mood claim has an attributable scope; no generative eligibility gate or packet-only scoring fallback remains in the completed path.

## Window 4 — Scout

**Purpose:** articulate prepared measurements and supported comparisons, with explicit limits.

**Start with:** `src/plugins/scout/adapter/{harvester,evidence,materials,mod}.rs`; `cognition/{mod,inputs,brief}.rs`; rating core/studies; `src/bin/statcommentary.rs`; rating evaluation callers.

**Work:**

1. Separate article-trigger eligibility from statistical evidence. Audit the generative `none|performance|roster|availability` source decision; move filtering to System 1/plugin policy without turning a news article into a measured rating.
2. Audit measurement identity, sport/role, season, units, sample coverage, comparison population, and freshness before selection. Keep arithmetic, ranks, trends, and rating values in measured computation.
3. Review palette selection: why each strength, limitation, composite, or trend is chosen; whether small samples and incompatible comparisons are withheld; whether useful material is lost by fixed selection limits.
4. Use prior measurements for valid comparisons and prior prose only for labeled continuity. Define a precise scouting voice through the plugin. Preserve no-stats and unavailable states without inventing a neutral measurement.
5. Unify queued rating, source-triggered rating, `statcommentary`, and evaluation preparation where they represent the same contract. Remove archived open-prose parsing, stale prompt fixtures, duplicated fallback model names, and legacy news/story filters that have no surviving owner.

**Verify:** strong/weak profile, sparse profile, composite only, missing stats, measured zero, incompatible seasons/populations, valid trend, source trigger with no measurements, and duplicate trigger. Confirm every numeral and comparison comes from prepared evidence.

**Exit:** source filtering cannot create statistics; all selected measurements and limitations survive articulation; production and rating evaluation use the same active contract.

## Window 5 — Insider

**Purpose:** articulate sourced transfer and relationship states without upgrading rumors into facts.

**Start with:** `src/plugins/insider/adapter/{mod,harvester,identity}.rs`; `adapter/harvester/{identity,wrap}.rs`; `cognition/{mod,inputs,brief,verification}.rs`; pair/heat scoring and identity-review consumers.

**Work:**

1. Map source filtering, resolved candidate pairs, rumor/stage verdicts, deterministic heat, identity review, and scored-board wraps separately. Identify every generative decision, including the Graph inference route used by Insider.
2. Frame allowed subject/object IDs, source claims, relationship direction, stage, dates, contradiction state, and confidence. System 1 supplies classification only within an evaluated vocabulary; plugin policy controls admissible states. A co-mention or verbatim quote alone does not establish a move.
3. Keep heat/likelihood definitions explicit and source-based. Audit source independence and duplicate reports. Neither previous Insider prose nor junction-authored Graph events may increase evidence counts.
4. Preserve pair checkpoints, source dispositions, identity-review obligations, superseded wraps, and claim fencing. Hand canonical identity questions to Investigator's evidence gate instead of letting articulation update affiliations.
5. Define a restrained voice for reported, disputed, agreed, and confirmed states. Remove obsolete packet/legacy transfer paths and duplicate verdict/correction machinery as their source and board consumers migrate.

**Verify:** rumor, explicit agreement, confirmed move, negated move, stale report, wrong person/team, non-transfer co-mention, duplicate coverage, conflicting sources, ineligible identity review, and resumed/superseded work. Test that a positive source claim cannot bypass the canonical write boundary.

**Exit:** plugin-owned relationship and score policy, bounded articulation, durable pair/identity/wrap obligations, and a named contract for later Investigator and Graph work.

## Window 6 — Analyst

**Purpose:** articulate the relationship between measured form and observed mood, with supported direction.

**Start with:** `src/plugins/analyst/{manifest.rs,adapter/mod.rs,cognition/mod.rs,cognition/inputs.rs,cognition/prompt.rs}`; trajectory studies; rating/vibe completion reactions and memory selectors.

**Work:**

1. Verify upstream product identity, dates, revisions, readiness, and compatibility. The plugin selects eligible Scout/Influencer products; raw article relevance should not be reclassified here.
2. Audit direction and conviction computations, sample counts, study windows, and steady-band policy. Determine what a missing rail permits. Missing mood or form must not become neutral evidence.
3. Review the current copying/truncation of upstream statements and palette assembly. Preserve qualifications and evidence scope. Define useful synthesis claims in the plugin; a stylistic connective cannot invent causation or a new trend.
4. Retain only dated memory that changes a supported comparison. Remove unused memory fields, old prompt/parser alternatives, redundant labels, and duplicated score representations after checking callers.
5. Preserve debounce, input hashes, dependency eligibility, and atomic completion events. Give the Analyst a comparative voice without recomputing its upstream products through prose.

**Verify:** agreement, divergence, one rail absent, both absent, stale/mismatched seasons, insufficient trajectory samples, unchanged inputs, and delayed upstream completion. Check no qualifier disappears when an upstream card is shortened.

**Exit:** one preparation and synthesis path with explicit partial-input semantics, computed direction, and articulation limited to selected relationships.

## Window 7 — Oracle

**Purpose:** articulate the current overall reading from the five finished character products.

**Start with:** `src/plugins/oracle/{manifest.rs,adapter/mod.rs,cognition/mod.rs,cognition/inputs.rs,cognition/brief.rs}`; barrier reactions; component provenance and readiness logic.

**Work:**

1. Inspect all five input contracts, their dates/revisions, readiness barrier, terminal outcomes, and missing-product handling. Select compatible finished products before articulation.
2. Audit `crown_score`, pillar comparisons, convergence, and omen computation. Record what each score means, which inputs contribute, and whether any default manufactures a neutral value when evidence is absent. Deterministic arithmetic still needs a justified product meaning.
3. Frame permitted overall claims and explicit unknowns. Do not use a prior crown as a sixth evidence source or treat several products derived from the same article as independent corroboration.
4. Review palette specificity, internal jargon, repetitive numerical recitation, and voice. Preserve upstream qualifications and prevent broad conclusions unsupported by the selected components.
5. Remove obsolete open-prose parsing, duplicate score extraction, old builders, redundant envelopes, and stale fallback logic once callers use the active contract. Keep the completion barrier and publication provenance intact.

**Verify:** all five products, partial products, none, conflicting directions, duplicate underlying evidence, stale component, component update during work, and unresolved upstream work. Validate computed scores independently of wording.

**Exit:** plugin-owned readiness, score, selection, and synthesis claims; SmolLM3 supplies only their final expression.

## Window 8 — Investigator

**Purpose:** resolve or update canonical identity only from sufficient retained evidence.

**Start with:** `src/plugins/investigator/adapter/{mod,discover,publish,factsweep}.rs`; `cognition/{mod,gate,prompt}.rs`; `src/bin/factsweep.rs`; source-document and entity-surface consumers.

**Work:**

1. Trace nominations from Harvester, Insider, Graph, and any remaining Editor path. Separate candidate discovery, external retrieval, deterministic matching, metadata adjudication, and database promotion.
2. Preserve and assess the existing name/sport/team discriminator gate. Identify generative extraction or factsweep decisions that still choose identity facts. Move classification to evaluated System 1 capabilities and deterministic policy where supported; unresolved cases remain ambiguous rather than being guessed by SmolLM3.
3. Scope Wikimedia tools and retained source receipts. Verify source name containment, identity discriminators, temporal affiliation evidence, and conflict handling. Tool success is not identity proof.
4. Use prior attempts and revisions as durable memory for deduplication and review. Do not let repeated model nominations manufacture corroboration. No public voice or articulation call is needed to persist verified canonical data.
5. Prune duplicate nomination/resolution routes, obsolete Editor coupling, prompt-based fact authority, and unused enrichment fields after mapping their consumers. Keep metadata revision history, invalidation, and rating follow-up obligations.

**Verify:** exact supported match, same-name collision, wrong sport, ambiguous candidates, old team affiliation, conflicting sources, unsupported occupation, repeated nomination, failed fetch, and stale claim. A model-generated citation must resolve to the retained evidence it claims to support.

**Exit:** no canonical promotion depends on SmolLM3's factual assertion. Any unimplemented classifier/extractor replacement is an explicit remaining blocker, not a completed alignment claim.

## Window 9 — Fixture Boxscore

**Purpose:** acquire and, where implemented, validate structured fixture measurements without generative invention.

**Start with:** `src/plugins/fixture_boxscore/{manifest,adapter,mod}.rs`; source registry, fetch receipts, parser-family and promotion consumers; Scout's structured-data requirements.

**Work:**

1. Verify implemented retrieval versus declared future parser/discovery/promotion work. The adapter currently documents those limits; do not silently turn this audit into building every planned capability.
2. Trace fixture identity, UTC event date, allowed source/domain, URL plan, trust state, budget, cache, and retained source document. Keep eligibility and parsing deterministic; introduce no model dependency.
3. Define the measurement boundary for score, periods, teams, players, units, and provenance. Missing parser/data remains unavailable. Fetch success cannot imply score reconciliation or canonical promotion.
4. Use fetch/retry/cache receipts as memory. Voice is not applicable. Check that downstream Scout receives only evidence with a supported trust and reconciliation state.
5. Remove unused scaffolding, duplicate registry defaults, dead fields, and misleading success paths only after consumer checks. Preserve the smallest extension points actually needed by implemented parsers.

**Verify:** supported/unsupported source, cache hit, redirected host, failed fetch, date boundary, fixture mismatch, incomplete document, unsupported parser, and conflicting final score where reconciliation exists.

**Exit:** an honest deterministic capability with no fabricated measurements, no unnecessary articulation, and documented remaining product limitations.

## Window 10 — Graph and final closure

**Purpose:** publish source-supported relationships, person nominations, and fixture evidence under explicit policy.

**Start with:** `src/plugins/graph/{manifest.rs,adapter/mod.rs,adapter/fixture.rs,cognition/mod.rs,cognition/prompt.rs}`; source context/candidate loaders; `narrative_events`, person nomination, typed-link and likelihood consumers; temporary dependencies recorded by earlier plugin handoffs.

**Work:**

1. Reconstruct the post-alignment inputs from Harvester, Insider, and Investigator. Remove Editor-dependent article eligibility, descriptions, and route/resource assumptions. Verify exact publisher context reaches each extraction/classification step.
2. Audit the remaining SmolLM3 relation/person/result extraction. Define source anchors, closed entity candidates, allowed predicates, direction, negation, time, and uncertainty before persistence. Evaluate System 1 classification and deterministic/source-span extraction for these bounded tasks; do not describe structured JSON generation as articulation.
3. Require semantic support as well as valid IDs and quote containment. An in-range candidate index or allowed predicate does not prove a relationship. Unknown people remain source-bound nominations for Investigator; fixture results require matching fixture identity and verified source text.
4. Audit memory and evidence aggregation. Keep extraction-origin measurement separate from junction-origin continuity so generated products cannot inflate typed links or transfer likelihood. Preserve the database provenance firewall and its consumer coverage.
5. Remove legacy Graph prompt/parser paths, Editor coupling, duplicate candidate representations, and the final recorded retirement dependencies once replacement behavior is verified. Close any remaining obsolete Editor registration, route, flag, job, packet/story consumer, and operational reference identified by earlier passes. Durable data removal requires its own migration/recovery checks. Graph needs no public voice unless it has a separately justified articulation product.
6. Run the final integration check across ingestion, source assignments, six products, identity work, fixtures, graph publication, and downstream barriers. Close all temporary-dependency entries and update README/fleet/configuration to the actual surviving architecture.

**Verify:** explicit supported relation, negation, unrelated co-mention, wrong entity, reversed direction, rumor versus confirmation, source mutation, duplicated evidence, unknown person, ambiguous fixture, repeat processing, and stale claim. Verify that junction-authored outputs cannot enter measurement consumers.

**Exit:** no generative factual authority hidden in Graph; source-bound relationships and nominations; no remaining Editor/packet dependency; current evaluation and operational tooling reflect the lean surviving paths. Report any unfinished capability honestly rather than weakening this boundary.

## Handoff record to fill after each plugin

Keep one compact entry per plugin here or link an existing focused evidence document. Do not create parallel plans that repeat this contract.

- **Plugin / revision / status:** audit, implementation, local verification, deployment verification.
- **Ownership changes:** decisions moved to System 1 or plugin policy; articulation that remains.
- **Context, tools, structure, memory, voice, boundaries:** final owner and implementation for each; use “not applicable” where appropriate.
- **Deleted:** old paths and migrated/removed callers, fixtures, flags, and documentation.
- **Retained temporarily:** exact consumer, reason, removal condition, responsible window.
- **Evidence:** commands/results, representative cases, quality observations, cost changes, unrun checks and why.
- **Next interface:** inputs, outputs, provenance, unknown/error states, and publication obligations the next plugin may rely on.

## Window 1 handoff

See [Harvester alignment](harvester-alignment-2026-09-27.md) for the responsibility map, removed paths, v6 interface, verification, and retained dependencies. Predicates and thresholds are unchanged; the context contract now versions stricter source/probability validation. No deployment or delivery release ran. The [real-source smoke](harvester-alignment-smoke-2026-09-27.md) passed acquisition/storage with live Laya, but all seven theme passes recommended all four characters; reviewed selectivity remains a release gate.

## Next fresh context: Journalist

Read the ownership contract, shared procedure, Window 2 and the Harvester handoff. Audit and implement only Journalist and necessary callers in a fresh context. Use v6 source receipts; do not restore the removed Harvester packet compiler. Verify plugin-owned source/claim/impact/continuity selection before SmolLM3 articulation, and record remaining Editor dependencies with their removal conditions.
