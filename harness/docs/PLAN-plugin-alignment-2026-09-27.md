# Plugin alignment plan — complete

> **2026-10-06 governing update:** This dated record preserves its implementation and evaluation evidence. The [current contract](../../../scoracle-wiki/wiki/Architecture/Harness,%20Plugins,%20Tools,%20and%20LLM%20Contract.md) supersedes articulation-only ownership: cheap code owns collection; expensive compute owns discovery. Plugins assemble traceable clues through tools; the LLM reasons across them, discovers what they support and articulates the answer. Historical “plugin owns WHAT/model owns HOW” statements below do not govern new builds.

> **Historical record, superseded October 5, 2026:** [rust/README.md](../README.md)
> is the current architecture authority. Plugins call shared or local tools in Rust
> and prepare the model input; models never call tools. See the [cleanup plan](PLAN-harness-plugin-cleanup-2026-10-05.md).
> The completed plan and its dated evidence below remain unchanged.

**Status: COMPLETE — October 5, 2026.** All alignment windows are closed and
release `fd981fd2a19c` is deployed on Archbox. Schema restore, migrations, API
health and the surviving plugin fleet are verified. Product fidelity/calibration,
unavailable extraction capabilities and retained historical-data retirement remain
separate work; they do not reopen this alignment plan.

## Governing direction

**EVERYTHING IS A PLUGIN.**

**Harness determines runnable work → plugin supplies data, its relationships,
and task/tone instructions → cognition model synthesizes and expresses it →
plugin validates and publishes.**

Harvester follows the same plugin boundary. It uses Laya, a System 1 model,
instead of a traditional LLM. That model choice does not create a separate System 1
layer, a privileged pre-plugin stage or a second architecture. Harvester owns its
source acquisition, DB reads, assembled model inputs, interpretation and durable
outputs just as the other plugins own theirs. Laya returns scores for Harvester's
bounded classification job; character LLMs return articulation. Both are model
operations owned by their plugin.

Planning date: September 27, 2026. Completed October 5, 2026 after the
Ponytail audit, six-part migration, evidence-boundary retirement and verified
Archbox release. The September 30 six-part/base-toolkit
revision below supersedes the earlier flexible file-role sketch.
This direction supersedes the earlier
“System 1 filters → plugins frame” shorthand, which incorrectly suggested that
System 1 sat outside the plugins.

This is the completed alignment record. Git history and linked evidence retain earlier
sketches and handoffs; do not implement a superseded sketch. Keep the implementation
lean by deleting redundant paths as their consumers migrate. Harvester's accepted
calibration and release state are separate from this ownership clarification.

## October 5 execution scope

The user closed Scout's alignment window and directed work to move on. This plan
is for plugin alignment, not model tuning. Ownership, context assembly, structural
acceptance, evidence-write boundaries and durable execution remain alignment
requirements. Reader-facing fidelity replays are retained as quality/release work;
they do not block moving between alignment windows. No failed replay is reclassified
as passing, and no deployment follows from alignment closure. This decision
supersedes the earlier requirement to close Scout's product gate before Insider.

## Ownership contract

**Harness determines runnable work → plugin reads, selects, computes and assembles
from durable evidence → model performs its bounded job → plugin publishes.**

The product is a living database expressed through plugins. Harvester, Scout,
Journalist and every other work-producing capability are peers. A plugin's model
choice and output shape do not change who owns its work.

| Owner | Responsibility |
| --- | --- |
| Harness / application / Studio | Registered capabilities, work claims, scheduling, budgets, model transport, bounded recovery, transactions and durable dispatch |
| Every plugin | DB reads, evidence selection, joins, memory, arithmetic, eligibility, assembly, model request, tools, voice where applicable, response acceptance and publication policy |
| Postgres / analytical studies | Durable evidence, revisions, receipts and products; reusable measurements called by the owning plugin |

Models are dependencies invoked **inside** the plugin's work. Harvester supplies
Laya's predicates and interprets its scores. Character plugins supply the assembled
world and ask their LLM to articulate it. The harness does not supply a separate
semantic policy layer, and downstream plugins read Harvester's durable products
rather than consulting an independent “System 1 layer.” Fixture Boxscore's current
deterministic work still has a plugin owner; it needs no gratuitous model call.

### Data, relationships and instructions — the binding target state

The plugin supplies three things:

1. **Data:** relevant measurements, reports, history, identities, dates, provenance,
   missing values and uncertainty.
2. **Relationships:** how those datapoints connect. Attach a correction to its
   report, history to the relevant development, and a compatible comparison to
   its measurement. Supply deterministic arithmetic and its scope where needed.
3. **Instructions:** what to do with the supplied material, how the output should
   feel, and the required output structure and limits.

**Tone without an invented character is the target.** Describe how the output
should feel; do not give the model a character or persona to play, a backstory,
or a motive it must invent or fulfill. Extract the useful writing qualities from
the template characters and supply those qualities directly. “Observant, direct
and restrained” is tone; “you are The Scout” is a role assignment. The product
may retain its character name without asking the model to become that character.

The cognition model owns synthesis and expression: it determines what matters
within the supplied evidence and how to communicate it. The plugin does not
perform that interpretive work in advance, prescribe a story, manufacture a
claim to be supported, or curate evidence toward a preferred conclusion.
“Unbiased” is the design constraint: relevance, comparison and coverage rules
must be explicit and reproducible, not an editorial position disguised as data.

Assembly means preparing coherent evidence, not prewriting the answer. Identity
resolution, source joins, compatible windows and deterministic calculations
remain necessary. A computed movement in percentile is a relationship between
measurements; “this makes them a formidable team” is an interpretation and must
not be inserted by the plugin. The model must not retrieve missing facts, pair
unjoined evidence, invent arithmetic, resolve canonical identity or manufacture
support for its synthesis. Harvester's model-owned classification job remains
inside its peer plugin; this clarification does not move Laya or change its
accepted calibration.

**Delete the model mentally: a reader must still know what this product is about,
which evidence supports it and which limits apply.** That is the assembly test.
Natural synthesis is the model's job; fixed phrasing menus and predetermined
conclusions are not the target for the reader-facing plugins. No new
claim-preparation LLM, judge or correction
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
service layer or registry. The unused `Contract`, `SLOTS` and slot-kind bookkeeping were removed in Scout
closure; retain the live `DecisionModel`,
`Prose`, `Dimensions`, parsers and capability grants. Do not replace the table
with the same assertions copied into manifests.

A JSON schema constrains response shape, not factual truth. Graph and Investigator
remain unaligned until their evidence write boundaries stop depending on generated
factual authority; renaming their response type does not close that work.

### Routing ownership

The harness determines what can run operationally. The Harvester plugin owns semantic source
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

## The six-part contract and base toolkit

**September 30 decision:** adopt a lean task and subject plus plugin-selected,
model-callable database tools. The six responsibilities below remain; research
and fresh evidence can arrive on demand as tool results instead of being eagerly
assembled into one world. The application executes scoped reads; the model chooses
when to use the tools. Reuse existing SQL/DuckDB work without adding a registry or
DSL. Unused parts remain absent.

**Confirmed character-plugin context standard:** model-facing JSON contains only
`meta`, `fresh`, `memories`, `voice` and `form`, with unused parts omitted;
`prompt.rs` supplies the sixth responsibility as the system instruction. Keep
provenance and internal bookkeeping outside the model package. Before adding any
new content to a character plugin's context package, show the user the exact
proposed contents and obtain confirmation. This applies to later migrations too.
The local Insider, Analyst and Oracle requests now follow this shape. Their
database and product replays remain open; a JSON wrapper around an old flat
prompt or palette does not satisfy the standard.

The [input-path audit](scout-closure-2026-09-29.md#september-30-input-path-audit-and-tool-access-reconsideration)
records the legacy request contents and synthetic replay limitations. The first
[read-only Influencer tool slice](scout-closure-2026-09-29.md#september-30-first-model-directed-tool-slice)
adds native tool-chat transport and one scoped source read. A provider-level SmolLM3
XML adapter now makes the schema visible and preserves call/result history in the
active runner; the pilot-shaped model probe still answered without calling the
tool. A read-only Archbox pilot now confirms the unavailable-data round trip with
Granite 3B; SmolLM3 remains unable to make a valid first read in this pilot.
Granite also called the tool and received verified PostgreSQL excerpts in two
historical used-source replays, but neither final answer passed product review.
Those replays bypass fresh-source eligibility. Production cutover is gated on an
eligible available-source database-backed transcript and factual review;
the existing workers remain on their prior path until then. Subsequent cleanup
must remove that superseded path when the replacement is verified.

**September 30 acquisition comparison:** the user is evaluating native calling
against a plugin-performed DB read. The
[matched Granite comparison](scout-closure-2026-09-29.md#september-30-acquisition-cost-and-garbled-output-diagnosis)
uses identical historical evidence in both routes. For this mandatory single read,
native calling adds a median 1.19-second decision call and 2.36 times the input
tokens in matched evidence-received pairs, with no product-quality improvement.
Both routes still fail factual/form review. Keep lean plugin preparation as the
lower-cost control; native calling must demonstrate useful query selection or
avoided reads before it becomes an efficiency requirement. No worker cutover is
authorized by these historical replays, and adaptive DuckDB research is unmeasured.

| Part | Responsibility |
| --- | --- |
| `meta.rs` | Canonical subject identity and relevant entity attributes. |
| `voice.rs` | Tone and writing qualities only; no persona, job or prescribed interpretation. |
| `memories.rs` | DuckDB-powered research over durable evidence, with selected history, compatible comparisons, dates, units, coverage and provenance. |
| `fresh.rs` | New content from the daily sweep and relevant newly available records/measurements, with source qualifications and missing-data states. |
| `form.rs` | Output fields, types, paragraph structure, dimensions, serialization and structural decoding. |
| `prompt.rs` | How the supplied elements relate and what to do with them: the concrete task, factual boundaries and valid partial/unknown outcomes. |

**Harvester selects exactly `meta.rs`, `fresh.rs`, `form.rs` and `prompt.rs`.**
Its model operation is Laya classification, so form defines its bounded result and
prompt defines the classification task. Harvester needs neither voice nor memories.
This subset does not change its accepted calibration or release status.

These are the target module names and ownership boundaries. Reuse shared modules;
do not duplicate six files per plugin or introduce six operations per model call.
Rename surviving character-tone modules to `voice.rs` during their migration and
remove the old aliases. Preparation, parsers, adapters and publication code remain
ordinary implementation mechanisms outside the six model-facing parts.

### Tools are capabilities; parts organize their results

Build the base toolkit for direct database and evidence interactions. Start with
existing canonical-identity loaders, source acquisition/presentation, Postgres
reads, DuckDB studies, structural decoding and durable publication mechanisms.
Plugins select tools and define their query scope, selection and publication policy.
The application provides scoped capabilities and operational budgets through the
existing plugin boundary. A tool's availability does not expose it to every plugin
or require a model to call it; deterministic preparation can call plain functions.

The toolkit can grow as plugins need new capabilities. **Investigator needs a
`browser.rs` tool** for browser retrieval and inspection. Reuse working browser
capabilities where present; retain source receipts for its evidence gate. Results
can feed fresh content or durable records, but browser retrieval alone does not
verify an identity. Other plugins select that tool only if their work needs it.
Browser and future tools extend capabilities; the six model-facing roles remain
selective rather than mandatory for every plugin.

The immediate target is a usable base toolkit, not a framework. Consolidate actual
consumers and fill demonstrated gaps; add no speculative tools, universal assembler,
new registry, plugin DSL or parallel compatibility architecture.

### A clean instruction boundary

`prompt.rs` is the sole owner of task and relationship instructions. `voice.rs`
provides tone; `form.rs` provides structure. Evidence-bearing parts carry facts,
scope, uncertainty and relationships, not editorial commands. Their rendered
content must remain distinguishable from writing instructions. Rust filenames,
loader names and implementation commentary are not model evidence.

An empty selected history is not proof of no history. A withdrawal identifies the
claim it retracts; a comparison identifies its measurements and scope. Resolve
these relationships in preparation, then explain their use in `prompt.rs`.
Cognition still owns synthesis and expression. A short, ordinary supported answer
is valid; do not require a main finding, emotional charge or interpretive flourish
that the evidence cannot support.

Remove legacy instructions rather than carrying them into the six parts. Existing
snapshot assertions and phrase checks are not product requirements. Preserve
explicit product limits and factual protections, but delete obsolete persona,
palette, generic editorial correction and shared-composer rules with their callers.
Do not preserve a conflicting instruction merely to keep an old eval green.

The current cleanup entry points are `support/prompt.rs` (shared composition,
schema descriptions and editorial correction), `support/form.rs` (prose guidance
mixed into structural form), character briefs, and the newer plugin manuals.
This is accumulated policy, not solely old code: Scout's instruction to avoid
repeating stat lines was added in the uncommitted S5 follow-up. Source history
identifies wording changes; it does not establish which coding agent authored
them or that passing an eval was the author's motive. Audit instructions by their
actual product purpose and effect, regardless of age or authorship.

### Assembly and verification mechanics

- Reuse `meta.rs` for canonical identity and `assembly::World` for deterministic,
  ordered rendering. Preserve plugin-chosen wire order; converting ordered structs
  through `serde_json::Value` can sort keys and change model behavior.
- Reuse source presentation and memory studies where they fit. Reporting history,
  arithmetic over fixtures and canonical identity records need not share a type.
  Plugins own selection, budgets, time windows and presentation. Do not load unused
  memory just because a shared loader can supply it.
- Keep computed boundaries in data: sample limits, comparison eligibility,
  provenance class and unknown states. Do not encode those decisions only in prose.
- Keep the job and factual-boundary instructions in the owning plugin's manual.
  Voice supplies writing qualities—such as observant, direct, emotionally attentive
  or restrained—not a role to play, backstory, motive, or obligation to invent a
  characteristic story. Character names are product identities, not model personas.
  Do not duplicate the job in the voice or require a claim followed by supporting
  material. A sparse world may support a short or partial synthesis.
- Shared form supplies keys, types, dimensions, serialization and decoding only.
  Existing writing
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
Use the six target names when migrating their responsibilities; reuse shared
implementations instead of relocating unrelated working modules for symmetry.

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
| 0 | Base toolkit | **Complete and released.** Native model conversation transport, scoped database tools and shared DuckDB studies survive. Available-source tool fidelity remains separate quality work; failed pilot evidence is retained. |
| 1 | Harvester / `harvester` | **Complete and released.** Accepted Laya policy retained; source-wide five-player fanout and durable delivery verified. Calibration remains separate. |
| 2 | Journalist / `narratives` | **Complete and released.** Six-part n96 context, prepared evaluation and request-local parsing aligned; independent storyline grouping preserved and database-verified. Product fidelity replay remains separate. |
| 3 | Influencer / `vibe` | **Complete and released.** Six-part v6 context, structural form and scoped source tool aligned. Available-source fidelity review remains separate. |
| 4 | Scout / `rating` | **Complete and released.** s66 measured-memory windows, source-bound context and parser repairs verified. Failed fidelity replays and unavailable measured-window evidence remain recorded. |
| 5 | Insider / `transfers` | **Complete and released.** One synthesis call per entity uses verified publisher reports and measured source records. Five-player fanout, denial, exact-name/quote and publication checks pass. |
| 6 | Analyst / `momentum` | **Complete and released.** Six-part source package and one synthesis call replace the palette; five database publication/recovery checks pass. |
| 7 | Oracle / `sigil` | **Complete and released.** Six-part reading consumes the five finished cards; six database publication/readiness checks pass. |
| 8 | Investigator / `investigate_entity`, `factsweep` | **Complete and released.** Structured identity discovery remains; runtime inference and generated canonical facts are retired. Unsupported prose/factsweep extraction is unavailable. Six database checks pass. |
| 9 | Fixture Boxscore / `fixture_boxscore` | **Complete and released.** Deterministic retrieval retains receipts. Unsupported parser families, discovery and canonical promotion remain unavailable. Unit and database publication checks pass. |
| 10 | Graph / `graph` and Editor retirement | **Complete and released.** Source-bound nominations/review receipts survive; generated facts and Editor runtime are retired. Nine Graph database checks and Journalist grouping check pass. Historical data and one pending Editor assignment are retained. |

Statuses above include the October 5 production release verification recorded below.
Earlier test totals in this record are dated evidence. The September 29 audit ran
`cargo test --offline --lib` (558 passed, 73 ignored). After the October 4 local
migrations, `cargo test --offline --all-targets --quiet` passed (466 library tests
passed, 62 ignored, plus other target tests). The Influencer comparison protocol
test also passed. These checks do not replace database smoke or model/product replay.

## Window 4 — Scout alignment closed

This is a bounded completion pass over demonstrated gaps, including the base
toolkit and instruction cleanup needed by these consumers. Keep the working measured/reported memory separation, typed `Limit` states,
selected comparisons and natural articulation. Do not reopen Harvester calibration
or Journalist's accepted prose design to make Scout look uniform.

### S1 — Source eligibility: implemented

`scout/adapter/harvester.rs` consumes the Harvester plugin's durable current
`performance` predicate and independently resolved entity link. Applied,
source-linked roster/availability records remain valid triggers. Scout no longer
asks an LLM to classify eligibility. Its only model operation is articulation of
a measured profile; the news receipt is trigger provenance, not a measurement.

Insufficient evidence is `relevant_but_unused` with a durable reason. The existing
identity/availability rating obligations and later statistical updates own follow-up;
no new classifier or routing policy was introduced. Old receipts lacking current
scores cannot supply the performance decision. Harvester calibration is unchanged.
Source, attribution, identity and applied-record eligibility are rechecked inside
the fenced publication transaction after articulation.

### S2 — One product construction path: implemented; fidelity checked in S5

Removed the unused phrase palette, duplicate flat material renderers, unused model
headline, duplicate measurement maps and redundant body checks. Scout requests only
`body`; the plugin supplies the title. Admission checks selected measurements or a
finite composite directly. Numeric, comparison, source-coverage and identity
boundaries remain. Existing coverage guards now read the actual prepared fields;
the obsolete 800-character thin-sample rule is removed.

Scout owns its bounded correction instruction. Shared recovery no longer imposes
another character's “one paragraph, one supporting detail” content policy on it.
Legacy memory loading remains for historical-season policy and provenance; raw
structured records preserve the obligations formerly hidden in unused prose.

### S3 — Completed-input fingerprint: implemented

The fingerprint includes the fully assembled world, prompt/output versions and
retained source provenance. Measured-memory-only and coverage-limit-only changes
invalidate it; identical assembled inputs do not. Source-trigger provenance joins
the same components before hashing. The publication test mutates the source during
articulation and verifies that no product is written.

The existing rolling measurement window still depends on its time bounds. A new
bound is a changed input; this check does not claim that reads at different times
are byte-identical.

### S4 — Prepared evaluation and capture: implemented

Live evaluation, current fixtures and capture carry the plugin's prepared request
and parts. Required parts and package drift are enforced at runtime. Journalist's
report count comes from its reports; Scout's parser receives its selected measures,
bands and compatible comparisons. No-call cases skip inference. Rejected prose may
be displayed diagnostically but cannot pass production acceptance. An unreviewed
Scout abstention is not a vacuous quality pass.

Scout assignment captures are version 2 and replay through its own options, parser
and bounded correction; old captures without parts require recapture. Remaining
unaligned tasks retain their existing adapters until their windows. No second
evaluation service or LLM judge was added.

### S5 — Plumbing removed; fidelity work deferred

Removed unused `Contract`/`SLOTS`/`SlotKind` declarations and their declaration-only
tests. Retained live `DecisionModel`, prose/schema/decoder APIs, route grants and
manifest checks. All-target tests and isolated database checks cover source routing,
wrong-entity/no-profile handling, applied records, durable follow-ups and publication
fencing. See [Scout closure evidence](scout-closure-2026-09-29.md) for exact counts,
requests, outputs, calls, retries and limitations.

The ongoing S5 investigation applies the data/relationships/instructions target
above. Tone-only controls remove Scout's role assignment without changing the
world, manual, form or model. This is a target-state correction, not proof that
voice alone explains or resolves hallucinations. Retain separate assembly,
instruction, budget and model comparisons and their full failed responses.

Local SmolLM3 replay is **not passing product evidence**. Scout can exceed the body
ceiling, turn sparse source coverage into actual early-season participation, and
infer ability or tactics from measurements. Influencer's v4 four-case replay failed
its paragraph ceiling; v5 cleanup results are recorded separately below. A parser accepting a response does not establish its factual
fidelity. Keep these failures visible; do not add a classifier, judge, phrase
palette, generic prose-policing layer or broader framework to hide them.

**Next:** use the retained controlled comparisons and current production replay in
the closure evidence. The tone/relationship corrections are implemented, but
accepted replies still invent facts. The September 30 transport follow-up confirms
an unclosed system turn in the running SmolLM3 template. Restoring only that
delimiter in paired raw requests still fails all four diagnostic worlds; it is
not a demonstrated S5 cure. Exact rendered prompts, twelve complete responses,
production-parser results and manual findings are retained in
[the transport probe](../fixtures/scout/s5-template-probe.jsonl) and explained in
[closure evidence](scout-closure-2026-09-29.md#september-30-transport-follow-up--confirmed-framing-defect-no-product-cure).
Keep this upstream framing defect separate from the unresolved synthesis gate;
do not repeat the template hypothesis or promote a shared transport change from
these results. The subsequent [instruction trace](scout-closure-2026-09-29.md#september-30-instruction-trace--establishing-a-working-control)
establishes a faithful conversational/rewrite control on the same model, schema
and quiet-news world. Appending a concrete restatement task to the otherwise
unchanged request is sufficient for that case. This is not a full synthesis fix.
Continue from that working control under the six-part contract: remove legacy
instruction paths, separate evidence from writing controls, and make measurement
scope and retraction targets unambiguous before testing richer synthesis. Audit
all six responsibilities actually used by Scout and Influencer, including tone
module names and shared form/correction leakage; delete superseded callers rather
than retaining prompt variants. Harvester's four-part subset and accepted behavior
remain explicit constraints on any shared-tool change. Do not substitute a rewrite
task for the intended product or infer model incapacity from the current failures.
The first bounded cleanup is implemented in s64 / `vibe-frame-v5`: Scout's
`brief.rs` and Influencer's `influencer.rs` are replaced by tone-only `voice.rs`;
shared observation form no longer prescribes content; both manuals distinguish
writing controls from evidence. Scout no longer asks for findings while forbidding
repetition of the measurements. The redundant test pinning manual phrases is
removed; factual guards, parser limits and fixture review criteria remain.
This is instruction ownership cleanup, not full six-part completion. The remaining
assembly work must clarify source coverage and retraction targets before richer
synthesis. The former Insider hold is superseded by the October 5 scope decision. See the
[cleanup replay](scout-closure-2026-09-29.md#september-30-six-part-instruction-cleanup).

Seven damaged converted fixtures are retired from the active suite; their original
versions and failed captures remain as historical evidence. Eight coherent current
prepared worlds now supply the Scout cases. They remain known product failures.
Superseded Studio/palette plans, duplicate exports and the one-off replay exporter
are pruned; this plan is the design authority.
Do not treat prompt adjustments or another model swap as a demonstrated cure. Do not change the product limits or
claim completion merely to make a mechanical gate green. Preserve Journalist's
accepted prose design and Harvester's accepted calibration.

**Alignment exit:** closed October 5. The local implementation and mechanical
checks are complete for this window. Retain the failed replay evidence and
capability limits for separate quality/release work. Do not reopen Scout for
prompt experiments or model tuning while completing the remaining alignment
windows. Code, test, product quality and deployment statuses remain distinct.

## Window 5 — Insider

**Purpose:** give every entity named by a transfer-flagged Harvester source one
Insider reading. Harvester performs the cheap normalized name search and enqueues
each player or coach; Insider receives exact publisher text, co-mentions, selected
reporting history and measured source records. The one model response carries
`body` and source-linked `findings`, as confirmed by the user. Each finding has
`status` (`reported` or `denied`); a denial requires an explicit source quote
and has no stage. The plugin validates exact quotes, resolves counterparties,
writes positive or clearing transfer rows, and stores the reading. Direction and
activity score are deterministic; speculative reporting never changes canonical
identity.

The old pair verdict, identity adjudication and wrap model calls are retired.
The ambiguous `single.rs` modules became `cognition/form.rs` and
`adapter/source.rs`.
Source attribution stays in each finding and published transfer row. Oracle reads
the five finished cards only and has no memory study. Migration
`287_source_performance_player_scope` keeps denials and coach reports out of
the existing player-outcome source accuracy study.

**Alignment exit: closed October 5.** The existing Harvester database smoke now
publishes one source naming five players across two query-team classifications.
It verifies all seven subject work items, four positive player rumors, one explicit
denial clearing row, retained source IDs, source-wide pair obligations, no canonical
identity writes, and exclusion of denials from player source accuracy. Wrong claim
tokens, an in-flight headline mutation and a superseding input revision publish no
partial effects. Seven Insider unit checks cover exact quotes, denial form and
unique counterparties, including wrong, ambiguous and wrong-type names. Existing
report-order selection retains denial/report boundaries. Model/product replay is
unrun and remains separate quality/release work; this is not a deployment claim.

## Window 6 — Analyst

**Local implementation:** one model call synthesizes the complete selected Scout
and Influencer readings with a dated trajectory study. The model-facing world has
`meta`, `fresh`, `voice` and `form`; `prompt.rs` supplies the task. `fresh` includes
available cards and computed slope, sample count and window dates, omitting missing
rails. Internal score and fingerprint fields stay outside the request. Direction
and conviction remain deterministic product fields. The palette and first-sentence
prompt path were removed; the existing work fingerprint, readiness and completion
behavior remain. `MOMENTUM_BANNED_PHRASES` still has a live parser consumer.

**Verify/exit:** run database smoke and model/product replay for agreement,
divergence, one or both inputs absent, stale/incompatible inputs, insufficient
samples, unchanged work and delayed completion. Check that card qualifications
survive synthesis and that the model adds no unsupported facts. Local unit tests
pass; this path has not been released.

**October 5 alignment exit: closed.** All five existing isolated database tests
pass: current publication with provenance/event/completion, revision and lease
supersession, no-material completion, and process crashes before/after commit with
outbox recovery. The test backend's obsolete palette `choices` response was replaced
with the current `blurb` contract. Structural context and parser tests remain the
checks for selected readings and unavailable rails. Product fidelity is deferred.

## Window 7 — Oracle

**Local implementation:** one model call reads the five finished character cards:
Journalist, Scout, Influencer, Analyst and Insider. The model-facing world has
`meta`, `fresh`, `voice` and `form`; `prompt.rs` supplies the task. Oracle has no
`memories.rs` or source research. Its `fresh` part includes up to three complete
Journalist narratives and each available finished card, omitting internal scores.
The old crown palette, flat prompt and input builders were removed. Deterministic
score, convergence, omen, readiness barrier, changed-input handling and publication
provenance remain outside the model request. Empty input still produces a null
marker without a model call; partial input stays explicit.

**Verify/exit:** run database smoke and model/product replay for all, partial and no
cards, conflicting directions, duplicate source lineage, stale/revised components
and unresolved upstream work. Check that the overall reading preserves the cards'
qualifications and adds no unsupported facts. Local unit tests pass; this path has
not been released.

**October 5 alignment exit: closed.** All six existing isolated database tests
pass: current publication, empty-card marker without inference, revision and lease
supersession, unchanged completion, and pending/retryable/terminal upstream
readiness. Unit checks cover complete five-card input, per-card hash changes,
deterministic direction/score and the absence of a memory study. Product fidelity
is deferred.

## Window 8 — Investigator

**Purpose:** persist canonical identity only from sufficient retained evidence.
Start with `investigator/adapter/{mod,discover,publish,factsweep}.rs`,
`cognition/{mod,gate,prompt}.rs`, `src/bin/factsweep.rs` and nomination consumers.

Trace nomination, retrieval, matching, adjudication and canonical promotion
separately. Retain source receipts, name/sport/team discriminators, temporal
identity evidence and conflict handling. Move supported classification to existing
evaluated mechanisms and deterministic policy. Missing extractor/classifier
capabilities stay explicit blockers; SmolLM3 cannot fill them by asserting facts.

Select `browser.rs` from the base toolkit for retrieval and inspection; extend
existing browser capabilities only where this operation needs them. Its receipts
feed the plugin's fresh evidence and retained provenance. No voice is needed for
identity persistence. No public prose or model call is required just to persist
verified data. Scope web tools, deduplicate attempts without manufacturing corroboration, and remove old
Editor/duplicate resolution paths. Preserve identity revisions, invalidation and
rating follow-ups.

**October 5 alignment exit: closed.** Candidate prose fallback and player-team
prose corroboration were deleted. The runtime handler no longer receives model
capabilities; the manifest has no inference grant or inference routes. Candidate
insufficiency records a refusal explaining that prose extraction is unavailable;
ambiguous player enrichment remains unchanged. Structured Wikimedia retrieval,
name/sport/team screens, retained source receipts, canonical revision policy and
claim-fenced publication remain. Historical prose cognition/evaluation is offline
only and has no canonical-write consumer.

The factsweep model verdict, citation and canonical-write implementation was
removed. Its operator command now refuses before configuration, database access,
inference or cooldown stamping, in both commit and dry-run modes. An evaluated
extractor for current reporting is a missing capability, not an alignment blocker.
An existing person with a conflicting known team now refuses a same-name merge;
the previous check covered players only. All six isolated publication tests pass,
including this conflict, source-write rollback, refusals, stale revisions/leases,
player cooldown and distinct team provenance. The identity adversarial fixtures
and structured parser tests continue to pass. Existing current-tenure parsing
still drops ended claims; this work adds no temporal extraction capability.

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

**October 5 alignment audit: closed.** The surviving path loads fixture facts,
renders dates in UTC, selects eligible registry sources, enforces the shared fetch
budget/cache path, and records terminal acquisition outcomes under an exact claim.
There are no implemented parser families: even a successful fetch produces
`not_supported`, without score or player measurements. This branch now retains
the source-document ID, cache disposition and registry attribution in `raw_labels`.
Unused normalization, numeric-stat, minutes and player-name parser scaffolding was
deleted after checking callers. Retrieval remains supported; search discovery,
parsing, reconciliation and canonical promotion remain unavailable. No new parser
or source capability was built. Ten unit checks and the isolated terminal
acquisition/ledger publication check pass. Wrong/incomplete/conflicting measurements
cannot reach downstream consumers through the unavailable parser branch.

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
   plugin-owned Laya/source-span/deterministic capabilities where appropriate; unsupported
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

**October 5 alignment exit: closed locally.** Graph now consumes verified Harvester
context or publisher text, never Editor blurbs. The g7 contract can nominate exact
source names and score lines for review; parser output discards generated relations,
roles and affiliations. No Graph path writes canonical events, persons or fixture
results. Relation and fixture extraction remain explicitly unavailable. Publication
locks and rechecks the source hash before effects; nine isolated database checks pass.

Editor is absent from fleet, routes and plugin registration. Its runtime handler,
packet compiler, default Scout packet fallback and maintenance job are removed.
Harvester owns week sealing, deduplication and storyline dormancy. Go intake requires
Harvester enabled with shadow mode off, and enqueues only Harvester work. Migration
288 records unavailable fixture extraction; 289 removes packet-driven scheduling and
blocks legacy Editor-based canonical identity eligibility. Historical rows and pending
Editor assignments are retained for operator review, never silently completed.

Journalist retains its own storyline grouping. Stored packet/storyline data remains
readable through the Go stories API; remove it only with a separate consumer migration
and recovery check. Historical Editor prompt inspection and regression code remain
offline/test-only; they confer no runtime publication authority.

## Retirement ledger

Delete each consumer with its replacement; delete a shared module only after its
last required caller migrates. Check Rust, Go, SQL, examples, fixtures, flags and
operational scripts as appropriate. Necessary temporary retention needs a concrete
consumer and removal condition, not "for later."

| Path / mechanism | Owner | Removal condition |
| --- | --- | --- |
| `plugins/cognition.rs` slot registry and unused `Contract` | Removed in Scout closure | Live decision/prose APIs preserved |
| Scout palette, ignored generated title, duplicate material formatting | Removed in Scout closure | Admission/provenance preserved; selected parts supply the model |
| Split live-eval prompt/options and manual parts reconstruction | Aligned tasks repaired in S4; remaining tasks in their windows | Each mode consumes the plugin's prepared request and parsing context |
| Character-named tone modules and mixed `brief.rs` files | Each consuming plugin | Tone moves to `voice.rs`, necessary task instructions to `prompt.rs`; delete personas, duplicate rules and obsolete aliases |
| Shared form prose recipes and snapshot-driven instruction requirements | Each consuming plugin, beginning with S5 | Keep structural form and explicit product limits; remove editorial demands and tests that preserve them without a product requirement |
| `support/prompt.rs::compose` and schema content instructions | Removed with Insider, Analyst and Oracle migration | Each caller now uses its plugin prompt and structural form |
| `support/prompt.rs` correction helpers | Each surviving caller | Retain generic bounded mechanics; remove generic editorial rewrites, keep only necessary plugin-owned correction instructions in `prompt.rs`; delete helper after last caller migrates |
| `evidence/memories.rs` and `memories/{sources,identity,performance}.rs` | Scout, Insider, Analyst, Oracle, Graph/Editor and eval consumers | Scout still uses historical-season state and a provenance fingerprint. Migrate those consumers before removal; other mission paths retire in their owning windows |
| `MOMENTUM_BANNED_PHRASES` in shared guards | Analyst | Keep locally only if still required; delete retired policy |
| Remaining palette/flat-prompt alternatives | Each owning plugin | Production, replay and operational callers use the surviving contract |
| Harvester packet API and compatibility types | Removed in Window 10 | Verified publisher report loaders and source-integrity receipts survive |
| Editor registration and legacy side effects | Retired in Window 10 | Harvester maintenance survives; offline Editor inspection/tests and Go historical stories reads remain. Data removal requires their migration and recovery checks |
| `storyline_*`, `story_parts.rs`, `impact`/`card_score` | Graph with Journalist/Oracle consumers | Keep live contracts; any replacement needs explicit consumer migration. These are not dead fields. |
| SQL `card_score_prev` column | Separate schema migration if requested | Rust use already removed; verify remaining external readers and migration/recovery needs before dropping data |

## Procedure and acceptance gates

1. **Trace first.** Follow every entry point to actual model request, parser and
   writer, including overrides, flags and eval. Name the job of each model call.
2. **Prepare once.** Select/join/compute in the plugin using existing tools. Keep
   the model's task concrete and the evidence complete for that task. Map the
   operation to the six named parts, select only its required tools, and record
   missing base-toolkit capabilities. Do not add placeholder modules.
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

Alignment completion requires active paths to satisfy ownership, context and
evidence boundaries, durable publication and honest missing-data behavior. Required
unsupported capabilities remain explicit; reader-facing prose tuning and unreplayed
product quality are tracked separately from alignment completion. Every model
operation, including Laya calls, must have a plugin owner. Deployment is
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
- **Influencer has no numeric sentiment without a measurement.** v4 remains local and its current paragraph-limit replay fails;
  see the [full replay evidence](scout-closure-2026-09-29.md). Fuller historical source
  passages are added only when a concrete continuity case needs them.
- **Scout's parts work stays.** `73374a72` introduced measured/reported memory,
  meaningful typed limits and a live `statistic::team_matches` consumer. Do not
  remove the statistic study as "unused." See [shared studies](memory-studies.md).
  S1–S4 close the mechanical gaps; the S5 product replay remains open.
- **Shared form is structural.** Body/paragraph policy remains as stated above.
  Do not restore shared writing direction, fixed phrase palettes or score invention.
  Preserve genuinely needed source integrity, error handling and publication checks.

The earlier detailed F1–F9 sketches and handoffs remain in Git history. This
section records only decisions the next implementation should carry forward.

## Completed implementation verification

The local plugin alignment implementation is closed through Window 10. Missing
relation, reporting-based identity and fixture parser capabilities are recorded as
unavailable; product fidelity/calibration stays separate from alignment.

Release schema capture, restore checks and the coordinated rollout are complete;
see the production receipt below. No alignment implementation or release step remains.

October 5 final verification used disposable PostgreSQL 17 loaded from the checked-in
baseline plus migrations 287–289. Graph's nine publication checks and Journalist's preserved grouping check pass. The
historical grouping fixture now supplies required source/membership fields and correct
article identifiers. Rust all-target
checks pass: 448 library tests (63 ignored), plus 14, one and two in other targets.
Go thirdparty and durable work tests pass with the disposable database. Earlier
Scout-through-Fixture evidence remains recorded in each owning window above.
Run database checks serially; Analyst's crash rehearsal needs an empty pending outbox.
The subsequent PostgreSQL 18 release capture and restore supersede this local
pre-release baseline check; no generated snapshot was hand-edited.

## October 5 release completed

Alignment implementation commit `5cf7df9d` and verified PostgreSQL 18 baseline
commit `fd981fd2` were pushed to `codex/harvester-cutover`. Archbox released
`fd981fd2a19c` at 15:29 UTC using `scripts/hosting/release.sh` (four Go and three
Rust binaries). Two disposable PostgreSQL 18 databases verified baseline restore,
migrations 287–289, captured-schema restore and migration no-op before cutover.
Checksums pass; reference rows equal the prior baseline apart from dump guard tokens.

Production migrations 287–289 applied successfully. API `/health/db` is healthy
and `/` reports `fd981fd2a19c`; cognition logs the same commit, ten surviving plugins,
reachable Postgres/model host and Harvester-owned maintenance. Both services and
rebuild watchers are active; initial service restart counts are zero.

Archbox now uses the surviving ten-stage list, Harvester intake enabled, shadow
mode off and delivery to Journalist, Influencer, Insider and Scout. Analyst and
Oracle remain downstream products. The obsolete packet-compile setting is removed.
The packet scheduling trigger is absent and legacy Editor identity eligibility is
false. One pending Editor assignment is retained for explicit operator review;
no retired worker claims it. Historical product and storyline rows were preserved.

Recovery copies of the previous binaries, environment and schema remain on Archbox
under `~/.local/state/scoracle/releases/20261005-plugin-alignment/`. Disposable
release/restore databases and scratch build binaries are removed after verification.

The earlier release-preparation list above is now complete. Next work is operational
observation of source delivery and durable downstream progress; this release does not
claim new product fidelity acceptance or implement the unavailable extractors.
