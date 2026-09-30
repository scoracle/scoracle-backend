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

Planning date: September 27, 2026. Revised September 29, 2026 after the Ponytail audit.

This is the current execution plan. It replaces the accumulated sketches,
superseded decisions and completion claims previously in this file. Git history
and the linked evidence documents preserve that work; do not implement an older
sketch because it appears in a historical handoff. This revision changes the
plan, not runtime behavior, release state or Harvester's accepted calibration.

## Ownership contract

**Harness determines runnable work → plugin reads, selects, computes and assembles
from durable evidence → model performs its bounded job → plugin publishes.**

For character products, that model job is articulation: **System 1 filters;
plugins frame; LLM articulates.** The product is a living database expressed
through different plugins. The same runtime boundary serves Harvester's System 1
work and deterministic Fixture Boxscore work without requiring either to generate
prose.

| Owner | Responsibility |
| --- | --- |
| Harness / application / Studio | Registered capabilities, work claims, scheduling, budgets, model transport, bounded recovery, transactions and durable dispatch |
| Plugin | Evidence selection, joins, memory, arithmetic, eligibility, allowed claims, tools, voice, output shape, response acceptance and publication policy |
| System 1 | Scores for plugin-supplied predicates; uncertainty stays explicit; plugin policy interprets the scores |
| LLM | Wording, coherent synthesis, emphasis, compression and voice within the supplied world |
| Postgres / analytical studies | Durable evidence, revisions, receipts and products; reproducible measurements over bounded inputs |

### Assemble versus articulate — the binding definition

The plugin decides what exists, what belongs together and what can be claimed.
It joins history to its report, resolves identities, selects comparable windows
and computes scores before inference. The LLM decides how the prepared material
reads. It must not retrieve missing facts, pair parallel arrays, calculate a
score, classify eligibility, resolve canonical identity or manufacture evidence.

**Delete the model mentally: a reader must still know what this product is about,
which evidence supports it and which limits apply.** That is the assembly test.
Natural synthesis is welcome; fixed phrasing menus are not the target for the
reader-facing characters. No new claim-preparation LLM, judge or correction
pipeline is introduced to compensate for incomplete parts.

Source qualifications, dates, attribution, units and uncertainty are part of the
world. Absence, unreadable data, measured zero, abstention, failed acquisition and
failed classification remain distinct. Prior prose may supply labeled continuity;
it cannot become independent evidence or increase a measurement's support.

### Everything is a plugin

Keep the existing `PluginManifest` and `StudioPlugin::execute` boundary. Plugins
own their implementation and receive scoped capabilities from the application.
Plain functions and plugin-owned data types do the work inside that boundary.

"Everything" means every work-producing capability has a plugin owner. It does
not mean every helper, part, table, study or parser becomes another plugin. It
does not require identical files, a fixed number of parts, one universal memory
type, or a model call where none is useful. The current eleven registrations
include Editor, which is being retired rather than redesigned.

Do not add another plugin trait, assembler trait, slot framework, plugin DSL,
service layer or registry. `Contract`, `SLOTS` and their slot-kind bookkeeping
currently do not enforce model-call roles in production. Remove that unused
parallel vocabulary in the closure work below; retain the live `DecisionModel`,
`Prose`, `Dimensions`, parsers and capability grants. Do not replace the table
with the same assertions copied into manifests.

A JSON schema constrains response shape, not factual truth. Graph and Investigator
remain unaligned until their evidence write boundaries stop depending on generated
factual authority; renaming their response type does not close that work.

### Routing ownership

The harness determines what can run operationally. Harvester owns semantic source
routing, using plugin-defined predicates and policy over Laya scores. Receiving
plugins own product sufficiency. Delivery controls may suppress a recommendation;
they may not manufacture relevance. Keep this distinction inside the existing
runtime, without adding a fourth orchestration layer or a router abstraction.

### Durable execution

A plugin reconstructs its work from durable state and its claimed input revision,
without depending on another plugin's in-memory result. Missing or partial upstream
data yields an explicit supported result, unavailable state or deferred obligation.
Queue ordering may avoid wasted work; it must not hide an input precondition.
Each remaining window checks its readiness behavior directly.

Idempotency is per claimed work/input revision, not "this entity can never run
again." Preserve claim fencing, stale-input checks, immutable source receipts and
atomic product/provenance/follow-up publication. Model and network work stays
outside publication transactions. A new relevant revision may produce a new
product. External fetches and model calls do not make a plugin a pure mathematical
function of the database.

## The parts contract

A plugin prepares **one authoritative world for each model operation**. It contains
only what that operation needs, with relationships already attached. Common parts
include identity, fresh evidence, selected memory, computed facts, limits, voice
and form; these are useful roles, not a mandatory list or directory structure.

- Reuse `meta.rs` for canonical identity and `assembly::World` for deterministic,
  ordered rendering. Preserve plugin-chosen wire order; converting ordered structs
  through `serde_json::Value` can sort keys and change model behavior.
- Reuse source presentation and memory studies where they fit. Reporting history,
  arithmetic over fixtures and canonical identity records need not share a type.
  Plugins own selection, budgets, time windows and presentation. Do not load unused
  memory just because a shared loader can supply it.
- Keep computed boundaries in data: sample limits, comparison eligibility,
  provenance class and unknown states. Do not encode those decisions only in prose.
- Keep content instructions in the plugin's manual and voice. Shared form supplies
  keys, types, dimensions, serialization and decoding only. Existing writing
  instructions in shared form/correction helpers migrate to the owning plugin or
  disappear when unnecessary; moving them must preserve or deliberately replay
  behavior. Do not replace the shared composer with another shared instruction stack.
- Keep deterministic scores and titles outside model output. Request only fields
  that will actually be consumed. Use the shared prose decoder with plugin-chosen
  keys; small plugin parsers may attach products and enforce evidence-specific rules.
- Product fingerprints cover the completed world plus required source revisions,
  contract versions and provenance inputs. Reuse the existing hashing mechanic;
  do not maintain a second hand-assembled approximation of the model's world.
- Production, fixture replay, live evaluation and capture use the same plugin
  preparation/rendering/options/acceptance functions. Preserve plugin-specific
  parsing context such as report count, comparison directions and measurement bands.
  Reuse the existing `Prepared` concept; do not build a generic request framework.

**Share mechanisms when they have real consumers. Keep policy with the plugin.**
A helper does not need an interface because a second consumer might appear. A
shared tool that weakens a consumer is worse than two small honest implementations.
Do not relocate working modules merely to make the directory tree symmetrical.

The 1,200-character prose-body ceiling remains a product constraint. Influencer
keeps its existing 140-character paragraph rule; Journalist and Scout retain their
explicit non-participation. Titles have separate dimensions. Do not reinterpret
these decisions as new writing requirements or tune them without product evidence.

## One plugin per fresh context window

Use this file as the durable index. The order below is execution priority for this
alignment work, not a prescribed runtime pipeline. Read the contract, next window
and relevant source/evidence; follow every live caller before editing.

| Window | Plugin / task | Current status and remaining work |
| --- | --- | --- |
| 0 | Shared foundation | Useful renderer, studies, prose decoder and parts fixtures landed. Do not restart a foundation project. Remove inert slot bookkeeping and repair remaining evaluation paths with Scout closure. |
| 1 | Harvester / `harvester` | V7 behavior accepted at 90/96 synthetic development routes; calibration deferred. Operational release remains separate. |
| 2 | Journalist / `narratives` | n94 deployed at `573d6a8e`. Nested-history n95 / `narratives-v11-nested-history` landed and replayed locally, unreleased. Live evaluation needs the closure repair. |
| 3 | Influencer / `vibe` | v3 parts/manual local, undeployed; current manual lacks live replay. Byte-identical parts refactors do not establish new product evidence. |
| 4 | Scout / `rating` | s61 parts landed at `73374a72`, unreleased and unreplayed. **Alignment reopened:** source eligibility LLM, duplicate palette, incomplete fingerprint and evaluation path remain. Next work. |
| 5 | Insider / `transfers` | Next plugin after Scout closure; relationship policy, pair memory, identity obligations and articulation migration. |
| 6 | Analyst / `momentum` | Current production sends a palette; align actual selection and synthesis, including partial inputs. |
| 7 | Oracle / `sigil` | Current production sends a palette; align five-product synthesis and evidence/readiness policy. |
| 8 | Investigator / `investigate_entity`, `factsweep` | Canonical evidence gate; generative factual authority remains to be removed or explicitly blocked. |
| 9 | Fixture Boxscore / `fixture_boxscore` | Deterministic acquisition audit; do not build unrequested future parser capabilities. |
| 10 | Graph / `graph` | Evidence extraction boundary and final Editor/legacy retirement; preserve Journalist's grouping dependency. |

Statuses above combine retained handoff evidence with the September 29 code audit;
this revision did not re-verify production deployments. The audit ran
`cargo test --offline --lib`: 558 passed, 73 ignored. That is a library-only result,
not a replacement claim for earlier all-target totals, DB checks or model replays.

## Window 4 — Scout closure before Insider

This is a bounded completion pass over demonstrated gaps, not another foundation
phase. Keep the working measured/reported memory separation, typed `Limit` states,
selected comparisons and natural articulation. Do not reopen Harvester calibration
or Journalist's accepted prose design to make Scout look uniform.

### S1 — Remove the second eligibility job

**Paths:** `scout/adapter/harvester.rs`, `scout/adapter/mod.rs`, Harvester delivery
and policy consumers. All source paths are relative to `src/plugins/` here.

Trace queued/source-triggered Scout, direct/backfill `statcommentary` and replay.
`harvester::decide` still asks an LLM for
`none|performance|roster|availability` before possibly calling articulation.
Replace this eligibility operation with existing Harvester signals and
plugin-owned deterministic checks where they satisfy the actual requirements.
Keep the independent entity link and source-linked roster/availability checks.
A news trigger must never become a statistical measurement.

If a necessary distinction cannot be established by existing evidence or an
already-supported System 1 predicate, name the missing capability and leave that
case unavailable/pending with its durable reason. Do not silently broaden routing,
drop records, invent keyword policy or assume Laya supports a new task. Do not
claim closure while a required replacement is missing. Any needed capability work
must name its bounded input, policy and representative evidence; this is not a
request for general Harvester recalibration.

**Done:** no Scout LLM eligibility call; every source disposition and required
follow-up still has an owner. Verify supported performance, source-linked roster
and availability, irrelevant/wrong-entity input, missing stats and repeated work.

### S2 — Remove duplicate product construction

**Paths:** `scout/cognition/mod.rs`, `scout/adapter/mod.rs`, `scout/cognition/parts.rs`.

Remove Scout's unused phrase alternatives, `Assignment.palette` and
`rating_palette` after preserving any real no-stats/admission conditions directly
against selected measurements. `Palette::validate` checks phrase structure;
`check_supported` does not inspect the palette at all. Delete its redundant
rendered-world self-check once serialization and material selection are covered.
Keep checks on input validity, identity, comparison support and numeric fidelity;
a serialization check is not a substitute for them.

Stop requesting and validating a model headline that `create` discards for a
plugin-derived title. Keep title generation and publication behavior. Remove
unused option copies, obsolete palette version labels and comments. Inventory
legacy memory/material renderers before cutting them: some still feed fingerprints,
historical-season policy or retained provenance even when their text is not sent.
Migrate those obligations first, then delete unused formatting/loading paths.

Review existing semantic guard/correction code against concrete failure evidence
and the new parts. Remove guards that enforce retired surfaces or merely duplicate
preparation; retain necessary evidence boundaries. Add no speculative prose-policing
layer. Shared recovery must not silently impose "one paragraph, one supporting
detail" on all characters; put any retained content policy with its plugin.

**Done:** selected parts are the only content representation used to create the
Scout product; no unused phrase menu or requested-and-discarded headline. Replay
checks factual fidelity, retained qualifications and useful articulation.

### S3 — Fingerprint the completed input

**Paths:** `scout/adapter/mod.rs`, source-trigger augmentation, all skip-unchanged
and publication consumers.

The current hash precedes measured-memory loading. Construct it from the completed
world and required contract/source revision inputs using the existing hash tools.
Preserve provenance and source fencing; do not hash only display text and lose
identity/revision distinctions. Avoid redundant alternate content construction.

**Done:** a change only to measured memory or a coverage limit invalidates the
product fingerprint; identical inputs remain unchanged. A stale source revision
still cannot publish. Leave a focused regression check exercising those cases.

### S4 — Finish evaluation parity using the existing machinery

**Paths:** `src/evaluation/tasks.rs`, `src/bin/eval.rs`, the three aligned plugins'
preparation/options/parsers, `fixtures/quality/{narratives,vibe,rating}`.

Offline parts assembly works, but live `score_backend` still calls the separate
options API that Journalist and Scout refuse. Capture emits `parts: None` and
expects manual reconstruction. Reuse the prepared request in live evaluation and
capture, including the parts needed to reproduce it. Use the same plugin functions
as production; eliminate contradictory prompt/options paths as consumers migrate.
Carry parsing context from preparation rather than independently maintaining
report count in expectations or evaluating only an archived parser.

Distinguish production acceptance from optional diagnostic display: an evaluator
may show rejected prose, but may not report it as production-valid. It may not
claim that numeric containment proves semantic correctness. Required parts and
request parity must be checked in the execution path, not only by fixture tests.
No-call dispositions should skip inference, as they do in production.

**Done:** fixture and live modes can execute the aligned contracts; capture can
produce replayable parts without hand-reconstructing them. Cover a multi-report
Journalist request, Scout's request-specific validation, malformed/declined output
and a no-call case with focused existing-harness checks. Retain the byte-order and
package-drift checks; do not introduce another evaluation service or LLM judge.

### S5 — Delete inert bookkeeping, verify the product, record closure

Remove unused `Contract`/`SLOTS`/`SlotKind` machinery and its declaration-only tests
after checking callers. Keep live schema/decoder functions, `DecisionModel`, route
grants and manifest validation. Existing module locations can stay; no taxonomy
rewrite is required. Replace claims of structural prevention with direct checks
of the actual plugin request paths, including a source-triggered Scout request.

Run relevant unit checks and all-target compilation; run isolated DB checks for
changed disposition, invalidation or publication behavior. Replay representative
Scout worlds: strong/sparse/composite/missing profiles, zero versus unknown,
incompatible comparisons, measured memory, qualified/withdrawn reported claims
and a source trigger. Inspect world and output together. Record actual calls,
request size, retries and latency using existing tooling, not a metrics project.
Replay the current Influencer manual before calling its product verified; preserve
Journalist's retained replay and rerun affected cases if this closure changes its
request or parsing. An unavailable model or database is an explicit unrun check,
not passing evidence or a reason to add infrastructure.

**Exit:** S1–S4 closed, inert plumbing removed, actual calls and publication paths
verified, remaining legacy consumers assigned below. Record code, checks, replay
and deployment separately. No deployment is implied by this plan revision.

## Window 5 — Insider

**Purpose:** articulate sourced relationship states without upgrading rumors into
facts. Start with `insider/adapter/{mod,harvester,identity}.rs`,
`adapter/harvester/{identity,wrap}.rs`, and `cognition/{mod,inputs,brief,verification}.rs`.

1. Trace each live source, pair and scored-board operation to its actual model
   request, parser and writer, including the Graph inference route. Do not copy
   Scout's old dual preparation or slot declarations.
2. Resolve candidate pairs, direction, source claims, dates, negation, contradiction
   and admissible stage in plugin policy. Keep heat deterministic and its source
   independence rule explicit. Unsupported identity or stage remains unknown;
   generative extraction is not renamed articulation.
3. Select pair memory using pre-resolved article IDs through the shared reporting
   study where suitable. Enforce pair identity/name scope in Insider: the shared
   study no longer does it. Test the include-list replacement that previously had
   no production consumer. Use a local memory type only if reporting history does
   not express the actual data.
4. Prepare parts and a plugin manual for actual articulation operations; reuse
   request/evaluation mechanics from closure. Scores and canonical IDs stay out of
   generated prose fields. Retire this plugin's `compose` caller, packet fallbacks,
   duplicate verdict/correction paths and old preparation as callers migrate.
5. Preserve pair checkpoints, source dispositions, superseded wraps and durable
   identity-review obligations. Canonical promotion goes through Investigator's
   evidence gate; a quote or positive transfer claim cannot bypass it.

**Verify/exit:** rumor/agreement/confirmation, negation, stale/wrong-entity/co-mention,
duplicate/conflicting reports, identity refusal and resumed/superseded work. One
active preparation path per real operation; every remaining non-articulation model
job explicitly closed or recorded as an unresolved blocker. No framework expansion.

## Window 6 — Analyst

**Purpose:** express the supported relationship between measured form and observed
mood. Start with `analyst/{manifest.rs,adapter/mod.rs,cognition/mod.rs}` and its
trajectory/input/manual consumers. Production currently sends `momentum_palette`.

1. Select compatible Scout/Influencer products by identity, time and revision.
   Define one-input and no-input behavior. Unknown sentiment is not a neutral score;
   never recreate the Influencer's removed numeric sentiment from prose.
2. Compute direction and conviction from supported measurements and documented
   windows. Upstream prose is attributed interpretation, not new measurement or
   evidence of causation.
3. Replace palette selection and first-sentence/headline shortcuts with bounded,
   assembled inputs that retain their qualifications. Select fewer complete units
   if necessary; do not build a generic sentence-level meaning detector.
4. Retire unused builders/parsers, the `compose` caller and legacy memory selection.
   Move `MOMENTUM_BANNED_PHRASES` home only if its surviving policy is still needed.
   Preserve debounce, fingerprints, readiness and atomic completion events.

**Verify/exit:** agreement/divergence, one or both inputs absent, stale/incompatible
inputs, insufficient samples, unchanged work and delayed completion. One supported
synthesis path, with no lost qualifiers or new computed facts from the LLM.

## Window 7 — Oracle

**Purpose:** articulate the overall reading from five compatible finished character
products. Start with `oracle/{manifest.rs,adapter/mod.rs,cognition/mod.rs}` and the
readiness, score and provenance consumers. Production currently sends
`crown_palette`; `build_crown_prompt` is not the production model request.

1. Trace all five input contracts and the readiness barrier. Keep supported
   partial/no-product behavior and distinguish pending work from absent evidence.
2. Audit score, convergence, omen and their unknown/default semantics. Several
   products derived from one article are not independent corroboration. A prior
   crown is continuity, not a sixth evidence source.
3. Assemble compatible products and plugin-computed relationships with their
   qualifications and provenance. Replace the live palette for natural synthesis.
   Do not first repair the retired flat prompt: retire it when its callers migrate.
4. Bound inputs by selecting complete qualified units. A 700-character or enlarged
   1,200-character slice cannot guarantee preserved meaning. If a unit cannot fit,
   omit it explicitly or supply a genuinely complete structured alternative; record
   the selection and its limits in the fingerprint. Do not add an LLM summarizer
   or a keyword detector for supposedly safe truncation.
5. Remove old builders, score extraction, envelopes, `compose` and legacy memory
   paths. Preserve the barrier, changed-input handling and publication provenance.

**Verify/exit:** all/partial/no products, conflicting directions, duplicate source
lineage, stale/revised components and unresolved upstream work. One prepared overall
reading; scores and factual relationships established before articulation.

## Window 8 — Investigator

**Purpose:** persist canonical identity only from sufficient retained evidence.
Start with `investigator/adapter/{mod,discover,publish,factsweep}.rs`,
`cognition/{mod,gate,prompt}.rs`, `src/bin/factsweep.rs` and nomination consumers.

Trace nomination, retrieval, matching, adjudication and canonical promotion
separately. Retain source receipts, name/sport/team discriminators, temporal
identity evidence and conflict handling. Move supported classification to existing
evaluated mechanisms and deterministic policy. Missing extractor/classifier
capabilities stay explicit blockers; SmolLM3 cannot fill them by asserting facts.

No public prose or model call is required just to persist verified data. Scope web
tools, deduplicate attempts without manufacturing corroboration, and remove old
Editor/duplicate resolution paths. Preserve identity revisions, invalidation and
rating follow-ups.

**Verify/exit:** supported match, same-name/wrong-sport/ambiguous/conflicting cases,
stale affiliation, unsupported occupation, repeated nomination and failed fetch.
Canonical writes are grounded in retained evidence, not merely schema-valid output.

## Window 9 — Fixture Boxscore

**Purpose:** deterministic fixture acquisition and supported measurement boundaries.
Start with `fixture_boxscore/{manifest,adapter,mod}.rs`, source registry, parser and
promotion consumers. Separate implemented retrieval from future parser/discovery
features; this audit does not authorize building all declared future capabilities.

Check fixture identity, UTC date, source/domain, redirects, budgets, cached receipts,
units and implemented trust/reconciliation rules. Missing parser or failed fetch
means unavailable data, not a verified score. Keep acquisition receipts as memory;
no voice, generic parts wrapper or cognition call is required. Delete unused
scaffolding after checking consumers.

**Verify/exit:** cache/retry/fetch failure, unsupported source/parser, date boundary,
wrong fixture and incomplete/conflicting results where reconciliation exists.
Only supported measurements reach downstream consumers; remaining limitations are
stated plainly.

## Window 10 — Graph and final closure

**Purpose:** source-supported relationships, nominations and fixture evidence.
Start with `graph/{manifest.rs,adapter/mod.rs,adapter/fixture.rs,cognition/mod.rs}`,
source/candidate loaders, typed links, likelihood and nomination consumers.

1. Trace remaining extraction against exact publisher evidence. Establish candidate
   identity, predicate, direction, negation, time and uncertainty before persistence.
   Allowed IDs and quote containment alone do not prove a relation. Evaluate bounded
   System 1/source-span/deterministic capabilities where appropriate; unsupported
   extraction remains unavailable, not generated evidence with a new type name.
2. Keep unknown people as source-bound Investigator nominations; require fixture
   identity and supported source data for results. Preserve the provenance firewall:
   junction-authored continuity cannot enter evidence-count or measurement consumers.
3. Preserve or explicitly migrate `storyline_*` consumers. Journalist resolves its
   own groups through those tables; fallible storyline membership is a presentation
   aid, not event identity or independent corroboration. Rewriting Graph must not
   silently change Journalist's selected memory.
4. Close legacy Graph/Editor eligibility, packet and route dependencies after
   migrating all operational consumers and surviving side effects. Retire Editor
   registration, flags, jobs and operational references. Durable data removal needs
   its own migration/recovery checks, not incidental cleanup.
5. Close the retirement ledger and verify ingestion, durable assignments, six
   products, identity, fixtures, graph publication and downstream readiness together.
   Update README/fleet/configuration to the surviving runtime.

**Verify/exit:** supported/negated/unrelated/reversed/rumored relationships, wrong
entity, source mutation, duplicates, unknown person, ambiguous fixture, repeat and
stale work. No generative factual authority or unexplained legacy dependency remains.
Report any missing capability instead of weakening the evidence boundary.

## Retirement ledger

Delete each consumer with its replacement; delete a shared module only after its
last required caller migrates. Check Rust, Go, SQL, examples, fixtures, flags and
operational scripts as appropriate. Necessary temporary retention needs a concrete
consumer and removal condition, not "for later."

| Path / mechanism | Owner | Removal condition |
| --- | --- | --- |
| `plugins/cognition.rs` slot registry and unused `Contract` | Scout closure S5 | Caller check confirms only bookkeeping/tests; live decision/prose APIs preserved |
| Scout palette, ignored generated title, duplicate material formatting | Scout closure S2–S3 | Admission/provenance moved to parts; no surviving product or operational consumer |
| Split live-eval prompt/options and manual parts reconstruction | Scout closure S4; remaining tasks in their windows | Each mode consumes the plugin's prepared request and parsing context |
| `support/prompt.rs::compose` and schema content instructions | Insider → Analyst → Oracle | Each caller replaced by its actual plugin manual; then delete composer/constants |
| `support/prompt.rs` correction helpers | Each surviving caller | Retain generic bounded mechanics; plugin owns needed content policy; delete only after last caller migrates |
| `evidence/memories.rs` and `memories/{sources,identity,performance}.rs` | Scout, Insider, Analyst, Oracle, Graph/Editor and eval consumers | Migrate real data/policy/provenance needs; remove unused mission paths incrementally; Window 10 closes final imports |
| `MOMENTUM_BANNED_PHRASES` in shared guards | Analyst | Keep locally only if still required; delete retired policy |
| Remaining palette/flat-prompt alternatives | Each owning plugin | Production, replay and operational callers use the surviving contract |
| Harvester packet API and compatibility types | Owning consumers; final closure in Window 10 | All pending delivery and offline/operational consumers migrated; source-integrity receipts preserved |
| Editor registration and legacy side effects | Affected windows; Graph final closure | Every needed side effect has a surviving owner and no live caller needs Editor |
| `storyline_*`, `story_parts.rs`, `impact`/`card_score` | Graph with Journalist/Oracle consumers | Keep live contracts; any replacement needs explicit consumer migration. These are not dead fields. |
| SQL `card_score_prev` column | Separate schema migration if requested | Rust use already removed; verify remaining external readers and migration/recovery needs before dropping data |

## Procedure and acceptance gates

1. **Trace first.** Follow every entry point to actual model request, parser and
   writer, including overrides, flags and eval. Name the job of each model call.
2. **Prepare once.** Select/join/compute in the plugin using existing tools. Keep
   the model's job narrow and the world complete. Do not prescribe file names or
   type sketches before reading the actual consumers.
3. **Replace and delete together.** Move one real operation onto the surviving
   path, migrate its callers, remove the obsolete path. No new compatibility layer
   without a named live consumer and exit condition.
4. **Check the boundary.** Reuse meaningful tests and leave a focused regression
   check for changed nontrivial behavior. Verify request/parse parity, no-call
   behavior, source identity, unknown states, input invalidation and publication
   fencing where touched. Tests asserting an enum differs from another enum or
   passing on an empty fixture do not prove architecture.
5. **Observe the product.** Replay changed model requests on representative worlds;
   inspect qualifications, attribution, unsupported additions, useful synthesis
   and voice. Use retained requests/responses and existing timings. Passing a
   parser or snapshot alone is not product verification. Do not add a benchmark
   framework, LLM judge or repeated tuning campaign for every plugin.
6. **Record one current handoff.** Revision, actual calls, deleted paths, retained
   consumers, verification/unrun checks and release state. Update the status table
   and ledger; replace superseded instructions rather than append a competing plan.

Completion requires the active paths to satisfy ownership, useful articulation
where applicable, durable publication and honest missing-data behavior. A required
unsupported capability or unreplayed product change remains open. Deployment is
separate; no release or data destruction is implied by documentation work.

## Retained decisions and evidence

- **Harvester calibration stays accepted.** 90/96 synthetic development routes are
  not independent gold; further calibration is deferred by the user. See the
  [v7 frame](harvester-frame-2026-09-27.md),
  [integrity repairs](harvester-integrity-review-2026-09-27.md),
  [cleanup](harvester-cleanup-2026-09-27.md) and
  [calibration follow-up](harvester-calibration-follow-up-2026-09-27.md).
- **Journalist's history attaches per report.** F3 landed in `b386f3b8`; grouping
  uses the report's storyline, not an impossible fresh-ID match against historical
  `source_ids`. Mixed history needs no third manual. Keep current deterministic
  attachments and ambiguity behavior. Source IDs remain provenance.
- **Journalist's prose design stays accepted.** n94 deployed at `573d6a8e`; thinking
  remains disabled. Source-derived titles, stateless natural articulation and
  preserved source qualifications stay. See the
  [handoff](HANDOFF-journalist-finish-2026-09-28.md) and
  [memory evidence](journalist-memory-world-2026-09-28.md).
- **Useful foundation work stays.** Shared history types (`76065db6`), prose
  decoding (`b386f3b8`), ordered rendering (`fc3ef96b`), parts fixtures (`9fffc883`)
  and dead Rust continuity-field removal (`694b4947`) are implemented. Repair
  demonstrated gaps; do not redo them to impose uniformity. Historical F1b's
  structural-enforcement claim is withdrawn by this revision.
- **Influencer has no numeric sentiment without a measurement.** v3 remains local;
  see its [handoff](HANDOFF-influencer-2026-09-28.md). Fuller historical source
  passages are added only when a concrete continuity case needs them.
- **Scout's parts work stays.** `73374a72` introduced measured/reported memory,
  meaningful typed limits and a live `statistic::team_matches` consumer. Do not
  remove the statistic study as "unused." See [shared studies](memory-studies.md).
  Its retained tests do not close source eligibility, fingerprint or replay gaps.
- **Shared form is structural.** Body/paragraph policy remains as stated above.
  Do not restore shared writing direction, fixed phrase palettes or score invention.
  Preserve genuinely needed source integrity, error handling and publication checks.

The earlier detailed F1–F9 sketches and handoffs remain in Git history. This
section records only decisions the next implementation should carry forward.

## Next fresh context: finish Window 4

Read the ownership and parts contracts, the current status table, S1–S5 and the
retirement ledger. Implement the bounded closure, verify it and update this file.
Do not start a new foundation framework or expand into Insider's implementation.
If a required capability is missing, name it and keep the relevant item open.

After closure, start Window 5 in a fresh context using
[KICKOFF-window-5-insider.md](KICKOFF-window-5-insider.md). That file is a pointer,
not a second plan. Remaining windows apply the same ownership test and delete
obsolete paths as they migrate; they do not copy Scout's historical plumbing.
