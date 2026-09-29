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

Planning date: September 27, 2026. Revised September 29, 2026.

**Revision 2026-09-29.** A read-only audit of the two aligned plugins against the
ownership contract found that the shared parts are not yet shared, that
"assemble" had two competing meanings and had produced one real defect, and that
the evaluation harness does not cover the deployed plugin. This revision adds
the binding assemble/articulate definition, the parts contract, Window 0
(Foundation, F1–F9), a decision register, a Window 0 handoff, and named defects
in Windows 4, 6, 7 and 10. Windows 1–3 keep their history; Window 2's exit claim
is explicitly reopened with the four reasons.

Harvester phase one is implemented and locally verified. The user accepts its current 90/96 development result and defers further calibration until experience warrants it; this does not block the next plugin window. Operational release remains separate. This is the governing alignment plan; older architecture and cutover documents remain evidence of earlier work, not competing target contracts.

## Ownership contract

**System 1 filters reality. Plugins frame reality. LLM articulates reality.**

The user's clarification is binding: **neither System 1 nor the LLM builds the decision world; plugins do. The LLM's job is articulation.** The plugin determines **WHAT can be said**; SmolLM3 determines **HOW it is said**. The model receives a bounded, prepared world. It must not determine what is true, discover relevant facts, reconstruct missing context, or decide what information exists.

Within that world, articulation includes natural expression, coherent synthesis, character voice, choice of emphasis, and compression that preserves important meaning. The plugin supplies factual boundaries and required qualifications; the model has expressive latitude inside them. This is broader than choosing a fixed sentence from a menu, and narrower than inventing an interpretation unsupported by the supplied facts.

The question for SmolLM3 is **“Given this reality, how should it sound?”**, never **“What is reality?”** Evaluate every inference with: **“Did the model express the supplied world well without adding anything that was not there?”** Unsupported facts, statistics, events, relationships, and assumptions are interface violations, not an accepted consequence of model size.

### Assemble versus articulate — the binding definition

The two words were being used for different things, and the looser reading has
already produced one defect. This is the only definition:

- **The plugin assembles the world.** It decides which parts exist, which part
  belongs with which, and what is attached to what. Every join, pairing,
  ranking, selection and attachment happens before inference.
- **The LLM composes the prose.** It decides how the parts it was given read
  together: wording, order, emphasis, compression, voice, paragraph breaks.

The question for SmolLM3 is **“Given these parts, how should this read?”** A
package that hands the model two parallel unordered arrays and asks it in prose
to work out which belongs with which has given the model an assembly job. The
manual may explain the shape of a part; it may never ask the model to resolve
one.

**The test for any package:** if you can delete the model and a reader can still
name what the product is about, the parts are assembled. If a reader cannot tell
which memory belongs to which claim without the model, the plugin has not
finished assembling. Journalist violated this until the September 29 audit
(Window 0, F3): its manual said *"combines the fresh item identified by its
report_key with the history that contextualizes that item"*, which is an
assignment the plugin had already made and the model was re-deriving.

Shared tools can supply presentation and decoding mechanics; plugins retain
evidence selection, admissibility, content instructions and publication policy.

The September 29 clarification is binding: the LLM is the cognition engine only.
It synthesizes and articulates the supplied world, with zero world-building
responsibility. `meta.rs` identifies the subject; fresh material, selected memory,
form and character supply the parts; `prompt.rs` explains their assembly. Noisy
fresh material belongs to Harvester calibration. Do not compensate by adding a
second eligibility task, content-policing guards, correction layers or elaborate
automated prose/disposition benchmarks inside the character. Preserve the simple
transport, requested form, provenance and publication mechanics.

- **System 1:** cheap probabilistic relevance, classification, routing, filtering, and downstream eligibility over plugin-selected inputs. Laya is the current Harvester classifier. It returns signals and uncertainty; plugin policy decides what those signals permit. Do not use the LLM for these decisions when a cheaper System 1 mechanism can perform them reliably. Deterministic eligibility checks stay in code. Unsupported classification capabilities must be measured or implemented explicitly; do not assume Laya already replaces every generative judgment.
- **Plugins:** own evidence selection, tool scope, product structure, memory selection, voice, permitted claims, scores, admission policy, and publication boundaries. A prompt located inside a plugin does not establish ownership if the model still chooses what is true, eligible, scored, or persisted.
- **SmolLM3:** articulates material already selected and bounded by the plugin. It must not serve as a second relevance filter, invent evidence, resolve canonical identities, compute scores, decide database facts, or promote its own prior prose into evidence.
- **Studio and the application:** provide generic inference, scoped capabilities, budgets, claim fencing, transactions, durable dispatch, and dependency assembly. Domain policy belongs to the owning plugin.
- **Persistence and studies:** Postgres retains evidence, provenance, continuity, products, and work. Analytical code and bounded studies compute measurements and comparisons. Prior interpretations may supply continuity; they cannot increase the evidence supporting a new measurement.

### Everything is a plugin

There is no character pipeline and no intake pipeline. There is one kind of
thing, a plugin, and eleven of them. The earlier framing — a pipeline whose
stages were progressively converted into plugins — was a description of the
migration, not of the target.

**Harvester is a plugin whose cognition slot is filled by a System 1 model.** It
is not a plugin-shaped exception. It has a `PluginManifest` with its own
`TaskKey`, `ClaimPolicy`, `inference_routes` and `tools: &[ToolGrant]`
(`harvester.rs:15-25`); an adapter; a cognition module with the same
prepare-then-enforce shape as every character plugin (`prepare_relevance()`,
`prepare_character_routing()`, `validate()`); parts; and publication. The
`ToolGrant` list is how a plugin holds tools no other plugin uses, which is
normal and is not a reason to be a different kind of thing.

Every plugin is the same shape:

| Slot | What it is | Who owns its content |
| --- | --- | --- |
| manifest | id, task, claim policy, declared inference routes, resources, tool grants | plugin |
| adapter | the production caller: reads DB state, prepares, calls, publishes | plugin |
| parts | the evidence, tools, memory, voice and form it assembles | plugin, sharing where sharing costs nobody anything |
| assembly | rendering the prepared world | shared mechanic, plugin-named parts |
| cognition | **one model call against a declared contract** | plugin declares the contract; the harness supplies the model |
| publication | provenance, claim fencing, atomic disposition | plugin, with harness mechanics |

The cognition slot is the only one that varies *in kind*, and it varies by
declared contract, not by exception:

- **Decision-shaped.** Harvester. Typed predicates in, bounded probabilities
  out, enforced by `validate()`. Laya.
- **Prose-shaped.** The nine character plugins. A prepared world in, prose out,
  enforced by a parser. SmolLM3.
- **Absent.** Fixture Boxscore. Deterministic, and says so.

The rule that used to read as a carve-out — "internal plugins may need no
articulation call" — is not a carve-out. It is what a plugin with no model in
its slot looks like from the outside. Making this uniform is what lets the plan
stop arguing about which plugins "have" an LLM.

**The contract type is the enforcement.** A character plugin cannot acquire a
second eligibility call, because it has no Decision slot to put one in. That
makes "no second eligibility task" structural rather than a rule to remember.
The mirror image is the finding: **a plugin with no declared, enforced response
contract is a plugin whose model role is undefined.** That is precisely the
condition the plan flags in Investigator's identity gate and Graph's extraction,
and it is more actionable than "stop using the LLM for facts" — it names the
missing artifact.

### A harness over a database, not a pipeline

What used to be a pipeline is now a harness that sits on a database and enriches
it through plugins. The operative property is that **a plugin is a total function
of database state**: given any state, running it produces a correct outcome,
which may be "insufficient evidence" or "nothing new." There is no partial
result that is only meaningful because some other stage ran first in this
process.

Two consequences, and both are already load-bearing in the plan:

- **Idempotency is correctness, not hygiene.** Running a plugin twice must
  produce no second effect. The claim fencing, input-revision checks and
  immutable receipts exist for this reason.
- **Dependency is data readiness, not execution order.** A plugin that needs
  another's output reads it, and if it is absent the plugin computes what it
  can from what is there — the plan already verifies Oracle against partial
  products and none, and Analyst against one rail and both absent. Nothing waits
  on a control-flow edge.

**One honest qualification.** `ClaimPolicy` carries `upstream_tasks` and
`pending_event_kinds` (`application/queue/work.rs:66-68`), which is queue-level
execution gating, not data readiness. That is legitimate as an *efficiency*
measure — do not wake Oracle until Analyst has work — and it is only sound
because the plugin above it is total. Where that ordering is load-bearing for
correctness rather than cost, a pipeline survives and the claim is false. Each
window should state which kind its dependency is. The test is short: can this
plugin be re-run against current state and produce a correct answer, with no
in-memory handoff from another plugin? The retained `Harvester::harvest` packet
API is pipeline residue on exactly this test — an in-process payload that the
context worker no longer needs.

### Four layers, not three

The vision is *harness dispatches → plugin frames and assembles → LLM
articulates*. It is correct, and the plan implements it. One correction, because
compressing it loses a boundary the system depends on:

**The harness does not decide the plugin. Harvester does; the harness executes.**
The plan is explicit — "The harness executes routing; plugins own semantic
eligibility" — and the distinction is load-bearing. A harness that chose
destinations would be a second semantic authority, and its thresholds could not
be versioned, measured or attributed to a plugin. So the runtime actually has
four layers, and the vision's "harness" is two of them:

| Layer | Decides | Does not decide |
| --- | --- | --- |
| **Harness** (Studio, application) | which plugin is enabled, budgets, deduplication, claim fencing, retry, transactions, durable dispatch | anything about sports content; any threshold |
| **System 1** (Laya) | probabilities for predicates the plugin supplied | eligibility, routing, or what a predicate means |
| **Plugin** | evidence, tools, parts, assembly, memory, voice, scores, admission, publication | whether the model understood its assignment |
| **LLM** (SmolLM3) | how the prepared world reads | what exists, what is eligible, what pairs with what |

A frequent way this architecture is broken is by moving a row upward or
downward for convenience. Moving System 1's job into the harness makes routing
unauditable. Moving a plugin's job into the LLM makes the world the model's.
Moving a plugin's job into System 1 makes it unowned when Laya is wrong. Every
finding in this plan is one of those three moves.

### Sharing is an optimization, not an architecture

The plugin is a toolkit, and a tool that serves two plugins is strictly better
than two tools. But the share must not cost the first consumer anything. A tool
made narrower, dumber or more restrictive so a second plugin can also use it has
not been shared — it has been diluted, and the damage is invisible because the
code is now in one place. Three rules follow, and they bind every shared part in
this plan:

1. **Share the mechanism, not the policy and not the constant.** One validator
   with a parameter is shared; one validator with one number is a shared policy.
2. **A plugin that cannot use a shared tool without being made worse keeps its
   own**, and that is a finding about the tool's shape, not a license to
   compromise it. Widen the tool, or let the plugin stand alone, and record which.
3. **Duplication that is visible beats sharing that is lossy.** Two honest tools
   are a smaller problem than one tool that is wrong for somebody.

This is why `meta.rs` is the model: it is shared, and it is shared without cost,
because every consumer wanted exactly the same thing. That is the test for any
part, not a stylistic preference.

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

Internal plugins need no articulation call, and that is not an exception.
Harvester's cognition slot holds a Decision contract and Fixture Boxscore's is
empty; both are plugins. Investigator and Graph must expose their remaining
generative extraction and adjudication as missing or unenforced response
contracts — not relabel them as articulation, and not as Harvester-shaped
classification either.

Existing finite phrasing palettes are an implementation to assess, not the definition of the target. Their structural guarantees are useful. Their claim selection, specificity, voice, repetition, and actual need for a model call still require review. Do not replace them with unconstrained generation or add another abstraction without representative evidence of a benefit.

Model evaluation priorities, in order: factual fidelity to supplied context; articulation quality; voice consistency; concise synthesis; speed and efficiency; general reasoning only where required. SmolLM3 is the selected articulation model. This plan does not reopen a general model contest: make the task narrow and the supplied world complete enough for the smaller model to express it well.

## One plugin per fresh context window

Use this single plan as the durable index. Start each plugin in a fresh context window with only its section, the shared ownership contract, and the immediately relevant handoff evidence. Do not run the ten audits in one accumulated conversation. The order is an audit order, not the worker's dispatch order.

| Window | Plugin / task | Scope | Status |
| --- | --- | --- | --- |
| 1 | Harvester / `harvester` | Intake, source extraction, System 1 filtering, delivery contract | V7 implemented and verified; current behavior accepted; calibration deferred; deployment separate |
| 2 | Journalist / `narratives` | Stateless articulation of fresh reporting and studied history | Deployed at `573d6a8e`; **exit claim reopened** — F3, F4b, F6, F7 open |
| 3 | Influencer / `vibe` | Emotional synthesis of supplied fresh context and memory | Parts/manual implemented locally; not deployed; migrates first in F2a/F4a |
| **0** | **Shared foundation** | **Memory, cognition contracts, form, assembly, manual, evaluation harness, guards, ledgers** | **In progress: F1, F1b, F2 complete. Blocks Window 4.** |
| 4 | Scout / `rating` | Measured performance, source triggers, statistical voice | Not started; carries F5, F8 and the first `statistic::team_matches` consumer |
| 5 | Insider / `transfers` | Relationship evidence, transfer state, heat, identity obligations | Not started; supplies pair scope as an include list under F1 |
| 6 | Analyst / `momentum` | Scout/Influencer synthesis and supported direction | Not started; carries its half of F8 |
| 7 | Oracle / `sigil` | Five-product synthesis, readiness, score, final reading | Not started; upstream body truncation is a named defect |
| 8 | Investigator / `investigate_entity`, `factsweep` entry point | Canonical identity and metadata evidence gates | Not started |
| 9 | Fixture Boxscore / `fixture_boxscore` | Deterministic acquisition and measured-data boundary | Not started |
| 10 | Graph / `graph` | Source-bound relationships; close final retirement dependencies | Not started; must not break Journalist's storyline grouping |

Window 0 is infrastructure and is worked in its own fresh context, like a plugin
window. It is inserted at position 0 rather than renumbering the rest because
Windows 1–3 are closed history and their numbering is cited across handoff
documents.

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

- Shared `src/plugins/support/form.rs` supplies reusable output shape and readability tools, without prescribing claims, supporting detail or conclusions. Plugins provide tools and instructions and may share tools; consuming a common form does not move domain policy out of the plugin. The 1,200-character body ceiling is shared and non-negotiable because it is a reader-facing product constraint. The 140-character paragraph rule is **per plugin**, applied through one shared validator that takes it as a parameter; Influencer enforces it today and Journalist does not, and forcing it on Journalist or dropping it from Influencer would make one of them worse in order to share. Content scope and factual qualifications belong to the plugin's prepared world and assembly instructions. As of the September 29 audit this gate is **half met**: one decoder and one dimension set is the target (Window 0, F4), and the six existing schemas are not yet one contract. Carry the target into the remaining character audits; their existing contracts are not migrated.
- Every published claim traces to selected evidence or an explicitly defined computation. An exact quote proves containment, not entity relevance or semantic support by itself.
- Articulation may change wording, emphasis, and compression, but must preserve material meaning, attribution, uncertainty, and required qualifications. Unsupported invention fails the interface. Test factual additions and meaning lost through omission alongside naturalness and voice.
- Scores, entity IDs, source IDs, dates, units, and uncertainty cannot be changed by articulation. Missing stays unknown. Absence, measured zero, abstention, acquisition failure, and classifier failure remain distinct.
- Memory has an explicit selection rule, budget, freshness policy, and provenance class. Retain only memory that affects the permitted reading; do not inject history merely because it is available.
- Tools are scoped to the plugin's needs. Model/network work stays outside publication transactions; claim and input-revision checks fence effects. Product, provenance, and required follow-up intent commit together.
- Production and evaluation use the same active preparation and contract. Historical replay tools cannot silently define production behavior. Remove tests that only enforce retired behavior; preserve meaningful invariants on the replacement path.
- Every deletion is checked for callers and operational consumers. Do not remove source provenance, idempotency, publication fencing, or recovery receipts as cosmetic cleanup.
- Record actual before/after model calls, request/output size, latency, retries, and file/path removals where relevant. Do not introduce a new metrics framework just for the audit.
- **The assemble/articulate test applies to every package.** No part is supplied as a parallel unordered array that the manual asks the model to pair up. Related items are nested or keyed by the plugin before inference. A manual that says "the history that contextualizes it" is a defect, not a style. Every world is rendered by the shared `plugins/assembly.rs` renderer, so this test is checkable on the rendered package rather than by reading plugin code.
- **One renderer, plugin-chosen parts.** `plugins/assembly.rs` owns key order, serialization and the world hash. It does not know which parts exist, what nests in what, or how anything is paired or selected. `form.rs:29` records that field order changes SmolLM3's output, so order is pinned by one test rather than by struct declaration order.
- **One shared decoder, plugin-chosen key names.** Every character product's prose is a map of plugin-named slots to prose, validated by one shared implementation of the paragraph and body ceilings. The *shape* is shared; the *keys* are the plugin's. A plugin that needs a deterministic non-prose field (Insider's and Oracle's score) owns that field outside the prose map.
- **One manual, in the plugin.** Content direction lives in `<plugin>/cognition/prompt.rs`. A shared module may hold decoding mechanics, mechanical prose guards and transport policy. It may not hold a shared stack of writing instructions that four character plugins compose from. `src/plugins/support/prompt.rs::compose` is the last such stack and is retired in Window 4.
- **Evaluation is a live gate, not a frozen string.** A quality fixture stores the plugin's *parts*, and the harness assembles them with the plugin's current assembler. A change to the assembled package must fail a test rather than silently invalidate a stored prompt.

## The parts contract

Every aligned plugin is six parts and one manual. This table is the whole
architecture; a plugin that adds a seventh part is a finding, not a design.

| Part | Module | Owner of content | Owner of policy | State at the September 29 audit |
| --- | --- | --- | --- | --- |
| identity | `src/plugins/meta.rs` | shared | none — canonical data | **Shared and correct.** 63 lines, no DB, every consumer takes `&EntityMeta`. Do not change. |
| fresh | `src/plugins/support/source.rs` presents; `<plugin>/cognition/fresh.rs` selects | shared presentation | plugin selection | Journalist wraps it in a keyed `Report`; Influencer embeds it directly. **Unify on the keyed form**, which F3 requires anyway to nest history per report. |
| memory | `src/plugins/memories.rs` studies; `<plugin>/memories.rs` requests, selects, presents | shared study | plugin request and presentation | **Diverged.** Influencer uses `ReportingHistory`; Journalist reimplements selection, budget, dedup and presentation in 233 lines. F2. |
| form | `src/plugins/support/form.rs` | shared | plugin names its own keys | **Not one contract.** Six schemas, two unrelated shapes, two dimension rules. F4. |
| voice | `<plugin>/cognition/<character>.rs` | plugin | plugin | **Correct.** Two or three lines each, plugin-owned. Do not share. |
| manual | `<plugin>/cognition/prompt.rs` | plugin | plugin | Correct for Journalist and Influencer. `support/prompt.rs::compose` still holds shared writing direction for four others. F5, in Window 4. |
| assembly | new `src/plugins/assembly.rs` | shared rendering: key order, serialization, world hash | plugin names its own parts and their nesting | **Duplicated.** Both plugins render the same five keys from a local struct; the load-bearing key order is pinned only by field declaration order. F4c. |

The reuse property to hold onto: **the mechanism is shared; the policy, the
constants and the decisions belong to the plugin.** A plugin adopts a shared
tool by supplying a request, wherever a request can express what it needs.

**Sharing is an optimization, not a requirement.** Where a request cannot
express what a plugin genuinely needs, the honest outcome is a second tool or a
wider tool — not a compromise that serves neither plugin well. A tool that is
made narrower, dumber or more restrictive so that a second plugin can also use
it has been made worse for its first consumer and has not been shared, it has
been diluted. When a shared tool cannot serve a plugin without degrading it,
that is a finding about the tool's shape, and the fix is to widen the tool or to
let the plugin keep its own. Record which, and why.

The same rule applies to sharing a *constant*. A mechanism may be shared while
its parameters stay with the plugin; a shared constant is a shared policy, and a
shared policy that one member needs changed is a diluted tool. See F4.

## Window 0 — Shared foundation

**Runs before Window 4.** The September 29 audit found that the shared parts are
not yet shared, and that the two aligned plugins demonstrate two different
contracts. Building the remaining eight windows on that base would propagate the
divergence into Scout, Analyst and Oracle, and Oracle consumes three character
products at once. Window 0 is infrastructure, not a plugin audit: it changes no
product, no voice, and no Harvester behavior.

Sequence by risk. Influencer is local and undeployed, so it migrates first.
Journalist is deployed, so every change to it is gated on a replay.

### F1 — Decouple the shared memory study from Graph

**Files:** `src/plugins/memories.rs:177-274`; `src/plugins/memories/reporting.sql:1-40`; `src/plugins/memories/tests.rs`.

The shared study is not plugin-neutral today. `memories.rs:198` validates
predicates against `crate::plugins::graph::cognition::PREDICATES`, and
`reporting.sql:34-37` joins `storyline_articles` and `storyline_entities` to
mint `storyline/...` and `requested_pair` topics. When Window 10 rewrites Graph
this file changes, and deployed Journalist breaks.

- Delete the `pair` and `predicates` parameters from `reporting_scope` and
  `Receipt`. **They have zero production callers** — only `memories/tests.rs`
  and `examples/memory_request.rs`. This removes the Graph import outright.
- Remove the storyline join from `reporting.sql`; the shared study emits
  `article/<canonical_id>` only.
- Add `plugins::memories::apply_topics(&mut Study, impl Fn(i64) -> Option<String>)`.
  Grouping becomes a plugin-supplied function over article IDs.
- A plugin needing a pair or predicate scope passes a pre-resolved `Vec<i64>` of
  article IDs it already holds. The Graph vocabulary never enters the shared
  module; Insider supplies it in Window 5.

**Done:** `rg "graph::" src/plugins/memories.rs` is empty.
**Verify:** existing memory study tests against the new signature; an
Insider-style pair request expressed as an include list returns the rows the old
predicate path returned, recorded in `docs/memory-studies.md`.

### F1b — One vocabulary for the cognition slot

**Files:** new `src/plugins/cognition.rs`; `src/plugins/mod.rs`;
`src/studio/decision.rs`; `src/plugins/harvester/cognition.rs:162-210`.

There are two contract shapes in the codebase today and no shared vocabulary for
them. `studio::decision::{DecisionRequest, DecisionResponse, PredicateQuestion}`
is used by Harvester and the System 1 provider and by nothing else; the nine
character plugins assemble a `String` and hand it to a free-standing parser.
Both do the same three things — prepare a bounded request from plugin-selected
facts, call once, then fail closed on a response that exceeds the contract —
which is why the plan's model rules currently live in prose rather than in code.

Add the shared shape, keep the two implementations separate:

```rust
// plugins/cognition.rs — the slot, not a plugin.
pub trait Contract {
    type Request;
    type Response;
    /// Turn plugin-selected facts into a bounded request.
    fn prepare(&self) -> Result<Self::Request>;
    /// Fail closed on anything outside the declared contract.
    fn enforce(&self, request: &Self::Request, response: Self::Response) -> Result<Self::Response>;
}
```

with `Decision` (moved out of `studio::decision` into the plugin layer, because
it is a plugin vocabulary and not a studio one) and `Prose` (F4's
`prose_map`, keys and dimensions supplied by the plugin). A third variant,
`None`, is the honest shape for Fixture Boxscore and makes its determinism a
declaration rather than an absence.

This is deliberately small. It is not a plugin framework, not a trait object the
worker dispatches through, and it does not move any policy. Its only job is that
"what may this model decide" becomes a declared, enforced artifact in the same
place for all eleven plugins.

**Done, in `b2577103`:** the module exists with the three kinds, the `Contract`
trait, the `SLOTS` table covering all eleven registered plugins, and
`cognition/decision.rs` moved out of `studio`. `cognition/prose.rs` declares the
Prose slot with plugin-chosen keys and the two dimensions separated.

Two corrections to the sketch above, made while implementing it:

- **`prepare()` is not on the trait.** What a plugin prepares from is
  plugin-specific — `&Article` for Harvester, `&Assignment` for the characters —
  so a uniform `prepare` would have been a fiction. Only `enforce` is genuinely
  shared, and the trait is correspondingly thin. The slot table, not a trait
  object, is what the worker reads; the worker dispatches through nothing here.
- **A slot carries whether it is enforced, and an unenforced slot must name the
  window that closes it.** Without that second rule a slot can sit in the table
  indefinitely while the declaration reads as coverage, which is the exact
  condition this task exists to detect. Investigator and Graph are recorded as
  unenforced today with the reason attached.

**`Prose::enforce` deliberately does not work yet and fails closed.** F4 supplies
that body. Wiring the declaration in ahead of the real validator would make a
contract the production path never consults — worse than none, because it reads
as coverage. The character plugins' existing parsers remain their enforcement
until F4 lands.

**Verified:** 534 tests pass (was 529). Both registry guards were mutation-checked
rather than assumed: deleting Graph from `SLOTS` fails with *"reaches no model and
declares no cognition slot"*, and stripping a gap fails with *"declares an
unenforced slot with no window to close it"*. A guard that was never seen to fail
is not a guard.

### F2 — One memory item, one presentation contract
**Files:** `src/plugins/memories.rs`; `src/plugins/influencer/memories.rs`;
`src/plugins/journalist/memories.rs`.

- Add to `plugins/memories.rs`:
  `pub struct HistoryItem { group: Option<String>, publisher: String, published_at: String, reported_headline: String }`
  and
  `pub struct GroupSummary { group: String, population: String, from: String, before: String, distinct_recorded_articles: usize, publisher_article_counts: Vec<PublisherCount>, source_ids: Vec<i64> }`.
  `group` is the stable key the study already computes as `topic`; carrying it as
  a field is what lets a plugin group (Journalist) or flatten (Influencer)
  without a different type, and it is the join key for F3.
- `ReportingHistory::select` returns `Vec<HistoryItem>` and gains a `grouped: bool`,
  so a plugin that does not group omits the field rather than presenting the
  study's one-article-per-observation default as a group.
- `journalist/memories.rs` deletes `MemoryReport`, `MemorySource`, `MemoryGroup`,
  `reporting_context` and `context`, and returns the shared types. Its `select`
  becomes index-aligned with the fresh reports (F3).
- `journalist/memories.rs::load` keeps its own `news_summaries` query — that is
  *self-memory*, what this plugin has already published — and the field is
  renamed `published_reports` so it is never confused with studied history. It
  stays a `CorpusItem`, not a `HistoryItem`: deduplication needs the article id
  and the full source text that the presentation type deliberately omits, and
  narrowing it to share a type would make it worse.
- The assembled `history` key has the same field names in both plugins.

**Done, in `76065db6`.** `rg "struct Memory(Group|Report|Source)" src/plugins/journalist/`
is empty; the Influencer's serialized item is unchanged; the Journalist carries
`group` and a parallel `history_groups`.

**F2 also completes F1.** Removing the storyline join from the shared SQL left the
Journalist with one group per article and no replacement, because nothing yet
supplied the `Topic` hook. F1 is therefore only behaviorally complete as of this
commit: the Journalist resolves its own storyline topics and passes them through
that hook, and the storyline tables remain a plugin dependency rather than shared
infrastructure again. Worth stating plainly, because F1's message claimed
behavior-preserving and that was true of the API and the test but not of the
production grouping until here.

**Verified:** 534 tests pass. The Influencer's serialized history item was
checked byte-for-byte against the pre-change shape. Two Journalist tests that
asserted the old nested shape were updated to the shared contract, not removed.
`GroupSummary` initially dropped `source_ids`; a failing provenance test caught it
and the field was restored to the type — a published memory claim with a count and
no resolvable article ids is a provenance regression.

**Unrun, and why:** `storyline_groups` is new SQL that nothing else exercises, so it
has an isolated database test covering scope, window bounds and a wrong subject.
It is `#[ignore]`d without `TEST_DATABASE_URL` and was not executed here. F2 and F3
both change the Journalist's assembled package and therefore its prompt, so they
share one replay gate; neither is released.

### F3 — The plugin assigns history to report slots

**Files:** `src/plugins/journalist/memories.rs:92-137`;
`src/plugins/journalist/cognition/mod.rs:158-160,182-215`;
`src/plugins/journalist/cognition/prompt.rs`; `src/plugins/support/form.rs:187`.

This is the assemble/articulate defect. Journalist supplies `MAX_REPORTS = 3`
fresh reports and `MAX_GROUPS = 3` history groups as two independent arrays and
tells the model to pair them. The n94 fixtures never exercise it because they are
1:1.

**Corrected after F2 landed.** The sketch above predates `76065db6` and is wrong
in two places. `select` now returns `Option<Selected>` where
`Selected { items, groups }`, not `Vec<Finding>`; and `GroupSummary` now carries
`source_ids`, which is a far better join key than the plan assumed. Use the
correction below, not the sketch.

- `select` becomes index-aligned with the fresh reports, one `Option<Selected>`
  per report, so `Assignment.memories` becomes `Vec<Option<Selected>>` rather
  than a single `Option<Selected>`. The budget-fitting `fits` callback currently
  measures the whole rendered context; per-report attachment changes what it must
  measure, and that is part of this task.
- **Join rule, deterministic, first match wins:**
  1. the report's canonical article id appears in the group's `source_ids` —
     an exact, already-retained link, no inference;
  2. otherwise exactly one group whose `before` is the nearest preceding boundary
     to the report's `published_at`;
  3. otherwise empty.

  More than one candidate under (2) yields empty. Missing stays unknown.
  Rule (1) is now the common case and is free: F2 put the article ids in the
  group summary, so a report indexed to a storyline already knows its history.
- The package nests history under its report:
  `fresh: [{report_key, publisher, published_at, publisher_excerpt, history: [...], history_groups: [...]}]`,
  and the top-level `history` and `history_groups` keys are removed.
- `HISTORY_TASK` becomes: *"Each `report_N` contains the fresh item at `report_N`
  and the history attached to it. Articulate that prepared set in the supplied
  voice and form. Add no history and no claim that is not in it."*
- `journalist_form` gains `history: "attached per report"`. The input hash
  covers the attachment so a changed pairing is a changed input.
- `prompt::task(has_history)` currently takes one bool; it must derive from the
  per-report vector, and a package where some reports have history and others do
  not needs a decision recorded rather than assumed.

**Done:** no manual sentence asks the model to resolve which memory belongs
where; the package has no top-level history array.
**Verify:** a 2-report × 3-group fixture asserts the deterministic attachment,
including one report that matches by `source_ids` and one that falls to rule (2);
a 3×3 adjacent case asserts rule (2)'s ambiguity resolves to empty; a case with
history on one report and none on another pins the mixed shape. Replay
`memory-nonredundant-n94` and `memory-direct-n94` and confirm the previously
correct outputs are unchanged. **This is a deployed behavior change: release
nothing before the replay passes.**

### F4 — One prose form

**Files:** `src/plugins/support/form.rs`.

- Add `prose_map_schema(keys)`, `prose_map_form(keys)` and
  `parse_prose_map(raw, keys, dims) -> ProseMap`. One implementation of: nonblank
  when present, every paragraph ≤ `dims.paragraph_max_chars` when the plugin
  supplies one, total across keys ≤ `dims.total_max_chars`, and
  `additionalProperties: false` over exactly the requested keys. `ProseMap`
  exposes `get`/`push` so callers stop re-parsing JSON.
- **Do not redefine `Dimensions`.** F1b already landed it at
  `src/plugins/cognition/prose.rs` along with `Prose::schema()` and `Prose::form()`.
  Reuse it. The schema and form generation F4 needs already exist there; what is
  missing is the validator body that `Prose::enforce` currently refuses to
  provide, and `form.rs` is where it should live so the parser and the validator
  stay beside the surface they enforce.
- **Share the validator, not the number.** `dims` is a plugin-supplied value, not
  a shared constant:

  ```rust
  pub struct Dimensions {
      /// Reader-facing ceiling across all keys. This one is shared and
      /// non-negotiable: a product constraint, not a writing preference.
      pub total_max_chars: usize,
      /// Readability policy. A plugin opts in and records why.
      pub paragraph_max_chars: Option<usize>,
  }
  ```

  The plan's gate names 140 characters per paragraph as if it were universal. It
  is not, and enforcing it universally makes Journalist worse than it is today:
  it currently validates only the body total (`parse_journalist:268-279`), and 900
  tokens across up to three keyed reports under a 140-character paragraph cap
  will raise `SurfaceError` at a rate nobody has measured. Softening Influencer's
  140 to accommodate Journalist would dilute Influencer instead. One validator
  with a parameter and a recorded per-plugin decision is the only form that
  serves neither plugin worse.
- **F4a, Influencer (safe):** `Prose::new(&["body"], Dimensions::new(BODY_MAX_CHARS, Some(140)))`.
  Behavior must be identical; the n94/retest replays prove it. A refactor, not a
  change. `plugins/cognition.rs::SLOTS` already marks the Influencer `enforced`,
  so this makes that declaration true rather than aspirational.
- **F4b, Journalist (deployed):** decided by replay, not by this plan. F2 changed
  the Journalist's package, so F4b's measurement is against a package F2 produced
  and F3 has not yet touched. Try `Some(140)`; if it fails materially, `Some(200)`;
  if that still fails, `None` with the paragraph rule recorded as a documented
  non-participation. Record the chosen value and the measured failure rate. Do not
  ship a number that neither plugin was measured against.
  **Recommendation: fold F4b into the same replay as F3.** Both change the
  Journalist's prompt, and measuring the paragraph rule against a package F3 is
  about to replace would produce a number nobody can use. They are one release.
- Leave `card_schema`, `oracle_format_schema` and `insider_score_format_schema`
  in place. Scout converts in Window 4; each other plugin in its own window.

**Done:** one implementation of the paragraph and body rules; `Prose::enforce`
calls it instead of bailing; `parse_observation` and `parse_journalist` delegate.
`SLOTS` moves Journalist from `pending` to `enforced` only when its parser
genuinely routes through the shared validator, not when the validator merely
exists.
**Verify:** existing `form.rs` tests, plus a test that a plugin-chosen key set
validates and a wrong key set fails closed, plus the F1b test that an
unenforced contract refuses to pass a response — that test must be inverted or
replaced once enforcement is real, and leaving it passing unchanged would be the
bug.

### F4c — One assembler, shared rendering only

**Files:** new `src/plugins/assembly.rs`; `src/plugins/mod.rs`;
`journalist/cognition/mod.rs:181-203`; `influencer/cognition/mod.rs:59-77`.

**Why this is its own task.** The September 29 audit unified the *parts* and
missed the thing that joins them. The assembler already exists twice, as two
near-identical local `#[derive(Serialize)]` structs rendering the same five keys
in the same order:

- `journalist::cognition::render_context` — `identity, history, fresh, voice, form`
- `influencer::cognition::assembled_prompt` — `identity, fresh, history, voice, form`

Two problems follow. First, `form.rs:29` records that field order *demonstrably*
changes SmolLM3's output, and that order is currently pinned by nothing but the
incidental declaration order of two structs in two files. Reordering one changes
behavior silently. Second, each plugin builds its own `input_hash` from a
different ad-hoc JSON object, so the two already hash different shapes for the
same conceptual world.

**The line, which is the whole task.** `assembly.rs` shares the *rendering*.
It must never share the *decisions*.

Shared, and only this:

- rendering in **insertion order, never sorted**, so the wire order is a stated
  choice rather than an accident of struct declaration;
- one `world_hash` over the rendered parts, so every plugin hashes a world the
  same way;
- the guarantee that a prepared world is rendered exactly once, in one place, by
  production, replay and the evaluation harness alike.

**The order is the plugin's, and the test pins it.** The two plugins currently
render different orders — Journalist `identity, history, fresh, voice, form`,
Influencer `identity, fresh, history, voice, form` — and `form.rs:29` records
that order changes SmolLM3's output. Forcing one canonical order on both would
change the behavior of whichever plugin had it wrong, which is making a plugin
worse in order to share a renderer. So: each plugin names its own order, and one
test asserts the rendered key order per plugin so it cannot drift silently. The
renderer guarantees *determinism*, not a *particular* order.

Never shared, and a change that puts any of these in `assembly.rs` is a defect:

- which parts exist, and in what order, for a given plugin;
- what nests inside what, and how many items a part has;
- pairing, budgets, selection and admission;
- anything about source identity, scope or a product's meaning.

Shape:

```rust
// plugins/assembly.rs
/// An ordered prepared world. Insertion order is the wire order and is
/// load-bearing; do not sort. This renders parts and nothing else.
pub struct World { parts: Vec<(&'static str, serde_json::Value)> }
impl World {
    pub fn new() -> Self;
    pub fn part(mut self, name: &'static str, value: serde_json::Value) -> Self;
    pub fn render(&self) -> String;
    pub fn hash(&self) -> String;
}
```

Note that `form` is a `serde_json::Value` the caller already builds, and after F4
the plugin can build it from `Prose::form()` rather than hand-writing it. The
renderer does not need to know that; it renders what it is given.

**The anti-pattern to avoid.** A `trait Assembler { fn parts(&self) -> Vec<...> }`
implemented once per plugin. That is indirection that removes no per-plugin
decision and adds a vtable; it is strictly worse than today's free functions. The
shared artifact is the renderer, and the per-plugin assembly function stays a
plain function that names its own parts.

**Dependency:** F4c lands with F3, not before it. F3 nests each report's history
under its own report key, which is what makes the two worlds the same shape —
after F3 both plugins emit `fresh: [{report_key, publisher, published_at,
publisher_excerpt, history: [...]}]` with a single fresh item for Influencer.
The shared assembler is only honest once that is true; before it, the assembler
would be papering over a real divergence with a common signature.

**Done:** one `World` renderer; both plugins assemble through it; one test pins
each plugin's chosen key order; both plugins hash through `World::hash`.
**Verify:** the n94 Journalist replays and the Influencer replays are unchanged
except for the intended F3 nesting. A test asserts that adding, removing or
reordering a part changes the hash and that rendering twice is byte-identical.
Reviewers can now check a plugin's entire world with one call and no reading of
plugin logic.

### F5 — Retire the shared manual stack (Window 4, Scout's first task)

**Files:** `src/plugins/support/prompt.rs`; `scout`, `analyst`, `insider`,
`oracle` `brief.rs`/`prompt.rs`; `src/plugins/support/prompt.rs:97` test.

`compose()`, `CardFormat` and the six content constants are the last shared
writing instructions. `compose` has exactly four callers —
`scout/cognition/brief.rs:3`, `analyst/cognition/prompt.rs:3`,
`insider/cognition/brief.rs:3`, `oracle/cognition/brief.rs:3` — and the test at
`support/prompt.rs:97` pins all four to it. Each plugin writes its own
`prompt.rs` from its own voice. `support/prompt.rs` retains only
`publishing_correction` and `structured_correction` until every plugin has
migrated, then is deleted. The shared test is replaced by a per-plugin assertion
that the plugin's own constants appear exactly once in its own system prompt.

### F6 — Make evaluation a live gate

**Files:** `src/evaluation/tasks.rs`; `src/bin/eval.rs`;
`fixtures/quality/narratives/`; `fixtures/quality/vibe/*.json`.

The plan's gate "production and evaluation use the same active preparation and
contract" is currently not met, and the September 29 audit is what established
it:

- `fixtures/quality/narratives/` is an **empty directory**.
- `src/evaluation/tasks.rs` has eight `LensTask`s — Vibe, Oracle, Transfer,
  Rating, Momentum, Graph, Editor, Investigator — and **no NarrativesTask**.
  `"narratives"` is absent from `all_task_names()`. The window marked deployed has
  no shared-harness coverage; only `examples/journalist_replay.rs`.
- Offline mode replays the fixture's frozen `user_prompt` string
  (`src/bin/eval.rs:558`), not live preparation. The v3 commit hand-edited four
  vibe fixtures to match. Every future window will rot its fixtures silently.

Changes:

- Add `NarrativesTask`; register `"narratives"` in `all_task_names()`.
- Change `Fixture` to store `parts: serde_json::Value` and add
  `LensTask::assemble(parts) -> String`. Keep `user_prompt` as a captured field,
  and add a test asserting `assemble(fixture.parts) == fixture.user_prompt`, so
  a package change fails a test instead of invalidating a stored string.
- Migrate `fixtures/quality/vibe/*.json`; populate `fixtures/quality/narratives/`
  from the cases the n94 replay already covers — direct-history update, adjacent
  non-redundant, rumour qualification, late correction, conflicting reports,
  no-call.

**Done:** every aligned plugin has a registered task; a deliberate change to
`assembled_prompt` fails `cargo test` in `evaluation`. This is the gate that
stops Windows 4 through 10 from drifting apart.

### F7 — Delete Journalist's dead continuity field

**Files:** `src/plugins/journalist/memories.rs:18,200-211,230`;
`journalist/cognition/mod.rs:74,177,278,332,350`;
`journalist/adapter/mod.rs:102,144`; `journalist/adapter/tests.rs:20`.

- **Delete** `Continuity::previous_score`, its query,
  `Assignment::card_score_prev`, `NarrativesProduct::card_score_prev` and the
  bind. The column has no reader in Rust, Go or SQL. Its own schema comment says
  it was fed to *"the n12 prompt's memory line"* — the retired palette. It runs a
  query on every Journalist assignment to populate a field nothing consumes.
- **Keep** `compute_news_impact`, `Narrative::impact` and `card_score`. The
  September 29 audit checked this and the initial read was wrong: `impact` is
  live in `src/evidence/story_parts.rs:147-164` (storyline trajectory),
  `src/plugins/oracle/adapter/mod.rs:163,173`,
  `src/plugins/oracle/cognition/mod.rs:727-729`, `go/internal/db/db.go:868,887,1164`,
  and the partial index `idx_news_summaries_sport_impact`. It is a deterministic
  computation with named consumers, which is exactly what the plan requires. Do
  not remove it.
- Leave `news_summaries.card_score_prev` in the schema. Drop the column in a
  separate migration with its own recovery check.

**Done:** no `card_score_prev` identifier remains in `rust/src/`.
**Verify:** `cargo test`; one isolated DB check that publication still writes
`impact` and `input_news_ids`.

### F8 — Split the prose guards (Window 4, Scout's second task)

**Files:** `src/plugins/support/guards.rs`; `scout/cognition/mod.rs:886-888`;
`analyst/cognition/mod.rs:190-192`.

`RATING_BODY_BANS` and `MOMENTUM_BANNED_PHRASES` are per-character content policy
defined in a shared module. They are already consumed only by their owning
plugin — only the definition is misplaced. Move each into its plugin.

Keep shared: `PRODUCT_NAME_BANS`/`first_product_name`, `has_bookkeeping_citation`,
`has_foreign_script`, `hook_violation`, `settle_title`, `title_names_entity`,
`clean_served_prose`, `count_sentences`, `strip_template_spans`, `contains_ci`,
`fold_for_match`, `has_ascii_digit`, `first_banned_phrase`. These are mechanical
invariants with genuine cross-plugin consumers.

**Done:** `rg "BANS" src/plugins/support/guards.rs` shows only
`PRODUCT_NAME_BANS`.

### F9 — Ledger for `evidence/memories.rs`

1,892 lines across `evidence/memories.rs` (1,272), `sources.rs` (406),
`identity.rs` (156) and `performance.rs` (58), with eight live consumers:
`evaluation/bin/eval.rs`, and the `analyst`, `editor`, `graph`, `insider`,
`oracle` and `scout` adapters. Seven windows will otherwise rediscover it.

Add a ledger table here — path, owning window, consumer list, removal condition.
Each window deletes its own consumer's path as it migrates; **Window 10 closes
the ledger.** Editor's consumer is removed with Editor. Do not delete the module
ahead of its consumers.

Starting ledger, to be filled in during Window 0 and closed in Window 10:

| Path | Lines | Live consumers | Owning window | Removal condition |
| --- | --- | --- | --- | --- |
| `src/evidence/memories.rs` | 1,272 | analyst, editor, graph, insider, oracle, scout, `bin/eval.rs` | each consumer's window | no consumer imports it |
| `src/evidence/memories/sources.rs` | 406 | as above | each consumer's window | as above |
| `src/evidence/memories/identity.rs` | 156 | as above | each consumer's window | as above |
| `src/evidence/memories/performance.rs` | 58 | as above | each consumer's window | as above |
| `src/evidence/story_parts.rs` | — | storyline trajectory; reads `news_summaries.impact` | Window 10 | keep; `impact` is a live product field |

### Window 0 exit

One memory item type; one prose decoder; one assembler; one manual home per
plugin; every aligned plugin in the evaluation harness with parts-based fixtures;
Journalist's dead field gone and its live one kept; guards split. No product
behavior changed except the four gated ones: F3 pairing, F4b paragraph ceiling,
F2/F4 wrapper identities, and F4c's move to a shared renderer. Each is replayed
before release and recorded separately from code-complete status.

**Ordering.** F4 and F4c land with F3, in that order: F4 fixes the decoder,
F3 nests history per report and thereby makes the two worlds the same shape,
F4c then has one honest renderer to share. F1, F1b, F2, F6, F7 are independent
and can be done in any order. F5, F8 and F9 hand off to Window 4 and Window 10.
F1b should precede any window that adds or moves a model call, because a contract
declared late is a contract declared to fit whatever the code already did.


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
The user accepts this behavior for now and defers further calibration, including
real-source and headline-negative evaluation, until experience warrants revisiting
it. The [calibration follow-up](harvester-calibration-follow-up-2026-09-27.md) records
options and evidence to retain. Calibration is not a blocker for the next plugin
window. Deploy the Python coverage adapter, Rust and Go intake producer together;
nothing in this work enabled production character delivery.

**Exit:** one active acquisition/classification/context path, no editorial summarization, explicit provisional/calibrated policy status, verified provenance, and a precise handoff for Journalist/Influencer/Scout/Insider and Graph. List remaining external consumers before any broader retirement.

## Shared memory work across character windows

**Postgres stores the world; DuckDB studies the world.** Memory is shared product
infrastructure, beginning with the Journalist integration. Reuse source-backed
history, bounded studies and provenance across plugins; each plugin's `memories.rs`
selects and presents the findings relevant to its fresh assignment. The model
articulates that prepared context without retrieving, calculating or judging it.

The [live matrix inspection and shared contract](journalist-memory-world-2026-09-28.md#shared-across-plugins)
record the existing substrate, initial DuckDB evidence and cross-plugin acceptance
properties. Carry this contract into every character window below. Verify useful
continuity, identity/time correctness, comparable measurements, source lineage,
correction/deletion invalidation and honest articulation as well as query speed.
Reuse the existing analytical snapshot infrastructure; do not build a parallel
memory store or copy the entire context package into each plugin. The shared
reporting study engine is implemented locally and Journalist is its first
integrated consumer; the match-statistic adapter has no consumer yet (see
Window 4). See [the implementation](memory-studies.md). Other character selectors
are not migrated. Nothing is deployed; discovered Graph binding and grouping
defects remain explicit preparation issues, and Window 0 item 6 in Window 10 now
also records that the grouping reaches a deployed plugin.

## Window 2 — Journalist

**Purpose:** articulate selected, attributed developments and their supported continuity.

**Locally complete checkpoint: n94 / fresh v7, September 28, 2026.** The exact
requests and responses are retained in `fixtures/journalist/context-n94-development.jsonl`
and the three `memory-*-n94.jsonl` records. The earlier
[fresh-window handoff](HANDOFF-journalist-finish-2026-09-28.md),
[context audit](journalist-context-trim-2026-09-28.md) and
[working history](journalist-alignment-2026-09-27.md) record the superseded
experiments; they are not the current contract.

**Binding design:** plugins prepare and govern WHAT; Laya scores; SmolLM3 articulates
HOW. Each call is stateless and memory-informed. The stored history supplies
continuity; the model has no obligation to evolve a story, invent significance,
prepare claims, retrieve, calculate, fact-check or judge the reporting.

- `meta.rs`: canonical identity.
- `fresh.rs`: newly fetched, complete attributed source reporting.
- `memories.rs`: requested scope, selection and presentation of studied history.
- `journalist.rs`: descriptive tone only.
- `form.rs`: structure only—fields, types, counts, limits, schema and parser. No
  direction about WHAT the output contains.
- `journalist/cognition/prompt.rs`: the sole task instruction and assembly manual.
  The data package is `identity`, `history`, `fresh`, `voice`, `form`.

**Implemented and pruned:** one natural articulation stage replaces the fixed phrase
palette and Editor packet/story corpus path. Fresh reports and output slots share
request-local `report_key` values, so source order is plugin-owned. The model returns
only report text. Titles come from the complete source opening and the edition
headline comes from the first plugin-selected title; the model no longer invents
either. Canonical storage retains league namespaces while the writing identity uses
the underlying sport (`NBA` becomes `basketball`), removing a demonstrated recap
genre cue without changing identity provenance. Explicit source attempts to override
the articulation contract fail closed before inference.

`prompt.rs` now acts as the instruction manual for the pieces actually present.
Fresh-only calls receive the compact report-key articulation instruction. When
`memories.rs` selects history, the system message identifies identity, history,
fresh, voice and form and states that historical reporting belongs in the report
text it contextualizes. This is still one stateless articulation call. `form.rs`
contains only the keyed JSON shape; it supplies no content direction. Temperature
is zero, thinking stays disabled and the output allowance remains 900 tokens.

The reasoning transport has since been repaired without changing this production
decision. When a route explicitly selects `think:true`, the Ollama boundary adds
SmolLM3's tagged-reasoning cue and the existing provider allowance becomes 1,500
tokens. A full n94 comparison verified nonempty separated reasoning, but it was
slower, exhausted the allowance once and reduced articulation fidelity. Keep
Journalist thinking disabled; the capability is available for explicit evaluation,
not promoted as a quality mechanism. See `journalist-thinking-2026-09-28.md`.

Memory requests still study a bounded read-only Postgres snapshot through the Go
DuckDB package. Complete source text, qualifications, UTC publication dates and
full provenance survive preparation. Historical sources remain distinct from fresh
evidence and do not inflate fresh source metadata or activity. Publication still
rechecks locked Harvester receipts and keeps disposition, publication and completion
intent atomic.

**Verification:** the final n94 no-thinking replay made seven fresh-report calls,
three memory-bearing calls and four correct no-call decisions. All ten calls parsed
and passed manual reporting-fidelity review. Rumour qualifications, late corrections,
conflicting reports, report order and NBA attribution were preserved; no reviewed
body added an unsupported fact. The direct-history case accurately articulated the
new 10:00 time against the previously reported 11:00 time. A merely adjacent history
case was not forced into the fresh report. Mean local inference time was 2.53 seconds.
The exact `think:false` requests and outputs are retained with the fixtures. Rust
verification passed with 555 tests and 77 environment-dependent tests ignored;
formatting and all-target compilation passed.

**Deployment:** commit `573d6a8e` was released on `archbox` with the standard atomic
release script on September 28, 2026. All four Go and three Rust binaries built
before placement. The production checkout and served API reported the exact commit,
`/health/db` passed, and the API, cognition worker and both binary path watchers were
active. No migration or configuration change was required.

**Known limits:** legacy Graph bindings and storyline membership remain fallible and
do not prove event identity or independent confirmation. Reporting-frequency counts
describe the stored article population only. The source-instruction admission check
recognizes explicit contract-override language; structural JSON isolation remains in
place for ordinary quoted source syntax. The suspected chat-template issue remains
unconfirmed and was not used to explain or mask the demonstrated failures.

**Preserve:** natural articulation, source integrity, provenance, claim fencing,
atomic publication, request-time memory scope and the existing single articulation
stage. No new generative claim preparation, mandatory LLM judge, blanket retries,
output-budget escalation, restored phrase palette or Harvester recalibration.

**Exit claimed met and deployed — reopened September 29.** The articulation
itself held: prepared fresh and historical reporting is articulated faithfully
with correct attribution, qualifications and source mapping, and no hidden
Editor dependency or fallback remains. The September 29 audit found four
corrections to the *surrounding* contract, none of which required touching the
prose behavior the n94 review passed:

1. **F3 — the package was not fully assembled.** `history` was supplied as a
   flat group array beside `fresh`, and `HISTORY_TASK` asked the model to combine
   each report with "the history that contextualizes that item." With
   `MAX_REPORTS = 3` and `MAX_GROUPS = 3` that is a pairing decision handed to
   the model. The n94 fixtures did not catch it because every case is 1:1. This
   is the assemble/articulate defect described in the ownership contract, and it
   is the reason the exit claim is reopened rather than amended.
2. **F4b — the paragraph rule was never applied here.** `parse_journalist`
   validates only the body total; Influencer enforces 140 characters per
   paragraph. The shared acceptance gate names one dimension set.
3. **F6 — the shared harness has no coverage of this plugin.** No
   `NarrativesTask`, `"narratives"` absent from `all_task_names()`, and
   `fixtures/quality/narratives/` is empty. Verification rests entirely on
   `examples/journalist_replay.rs`, which is a bespoke tool, not the gate the
   plan requires.
4. **F7 — `card_score_prev` is written and never read.** Its schema comment ties
   it to the retired n12 palette's memory line. `impact`/`card_score` is live and
   stays.

Window 0 closes all four. Do not reopen Harvester calibration.

## Window 3 — Influencer

**Purpose:** synthesize the supplied world and faithfully articulate its emotional charge around the identified entity.

**Parts and manual:** shared `plugins/meta.rs` supplies subject identity; `influencer/cognition/fresh.rs` presents intact publisher context with shared source tools; `influencer/memories.rs` selects dated historical context; shared `support/form.rs` supplies structure and dimensions; `influencer/cognition/influencer.rs` supplies voice; `influencer/cognition/prompt.rs` explains how to assemble the parts.

**Work:**

1. Keep one cognition call over the assembled world. Harvester selects incoming material; its remaining calibration noise does not create a second reaction/eligibility task for the Influencer model.
2. Keep the components complete and their roles clear. The manual asks for synthesis of fresh material and relevant memory, preserving supplied speakers, timing, scope and uncertainty. The model adds expression and synthesis inside that world.
3. Keep sentiment unknown without a supplied measurement. Select historical context in the plugin and preserve source lineage. Inspect missing parts before expanding instructions or adding guards.
4. Prune superseded packet, score, reaction-gate, content-guard and evaluation branches. Preserve one shared structural decoder, provenance, source freshness, publication fencing and atomic disposition.

**Review:** inspect the supplied parts and the resulting synthesis together. Noisy source context is an intake observation for Harvester. A nullable body remains a transport capability; there is no benchmark requiring the cognition engine to classify neutral sources as passes. Existing paragraph and body dimensions remain the designed form.

**Local checkpoint:** [the current handoff](HANDOFF-influencer-2026-09-28.md) records `vibe-frame-v3`, the simplified manual, shared form decoder and pruned semantic/disposition checks. Earlier model probes remain historical evidence, not the current architecture or an outstanding abstention decision. The new manual has not been live-replayed or deployed.

**Missing part to consider:** historical memory currently contains only headlines, publishers and dates. It cannot supply the speaker detail and qualifications found only in the source passage. If fuller emotional continuity is needed, supply bounded historical source passages through the memory component rather than ask the model to reconstruct them. Under the parts contract this is a **plugin selection choice, not a shared limit** — `HistoryItem` can carry a bounded passage, and Window 0 does not require Influencer to take one. Decide it here against a replayed case that shows the loss, not in the abstract.

**Migrates first, because it is safe to migrate first.** F2a moves Influencer
onto the shared `HistoryItem`/`GroupSummary`; F4a makes `observation_schema` and
`parse_observation` wrappers over the shared `prose_map` with keys `["body"]`.
Both are identity changes: the v3 replays must come back byte-identical, and if
they do not, F2a or F4a changed behavior and the plan is wrong. F6 then replaces
the four hand-edited `fixtures/quality/vibe/*.json` prompts with stored parts.

**Exit:** one stateless cognition call over plugin-supplied parts and assembly instructions; no world-building, second eligibility call, numeric sentiment invention or packet fallback; the shared memory item, shared prose decoder and parts-based evaluation fixture are all consumed. Code alignment, product observation and deployment remain separately recorded.

## Window 4 — Scout

**Purpose:** articulate prepared measurements and supported comparisons, with explicit limits.

**Start with:** `src/plugins/scout/adapter/{harvester,evidence,materials,mod}.rs`; `cognition/{mod,inputs,brief}.rs`; rating core/studies; `src/bin/statcommentary.rs`; rating evaluation callers. Delete `cognition/brief.rs` and `cognition/inputs.rs`; they are inputs for a prompt architecture this window replaces.

**Inherited foundation work, done first in this window:**

- **F5 — retire the shared manual stack.** Scout is the first of four callers of
  `support::prompt::compose`. It writes its own `cognition/prompt.rs` from its own
  voice and uses the shared `prose_map` form with keys `headline` and `body`.
  Replacing `cognition/brief.rs` with `cognition/prompt.rs` is the deliverable.
- **F8 — take `RATING_BODY_BANS` out of `support/guards.rs`.** It is consumed only
  by `scout/cognition/mod.rs:886-888`; only its definition is misplaced.

**The statistic adapter has no production consumer.** `plugins::memories::statistic::team_matches`
is called from `memories/tests.rs` and `examples/memory_request.rs` and nowhere
else. Scout is its intended consumer and the plan should say so explicitly rather
than let an unexercised shared adapter sit in the foundation. Either Scout adopts
it as its trend measurement, or Window 0 records it as unused and deletes it. Do
not leave a third option where a shared tool has no owner.

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

**Start with:** `src/plugins/analyst/{manifest.rs,adapter/mod.rs,cognition/mod.rs,cognition/inputs.rs,cognition/prompt.rs}`; trajectory studies; rating/vibe completion reactions and memory selectors. Delete `cognition/inputs.rs` and replace `cognition/prompt.rs`'s `compose` call with a plugin-owned manual.

**Inherited foundation work:** **F8 — take `MOMENTUM_BANNED_PHRASES` out of
`support/guards.rs`** into `analyst/cognition/mod.rs`, its only consumer. The
remaining shared guards stay shared.

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

**Start with:** `src/plugins/oracle/{manifest.rs,adapter/mod.rs,cognition/mod.rs,cognition/inputs.rs,cognition/brief.rs}`; barrier reactions; component provenance and readiness logic. Delete `cognition/inputs.rs` and `cognition/brief.rs`; replace the `compose` call with a plugin-owned `cognition/prompt.rs`.

**Named defect found September 29 — upstream bodies are truncated, not
summarized.** `cognition/inputs.rs:93-167` (`build_crown_prompt`) concatenates
heterogeneous upstream fields and caps each one:

- `narrative_title` and `body` for narratives,
- `v.prompt` for vibe,
- `mom.blurb` for momentum,
- `r.read` / `r.prompt` for other products,

each through `capped(text, body_cap)` under `CROWN_CARD_BODY_CAP: usize = 700`.
`descrub_z` and per-body division make the cut silent and unequal. A rumour
qualification, a source attribution or an uncertainty marker in the last
sentence of an upstream body disappears without any signal, and Oracle then
articulates a reading that overstates its inputs. This is a meaning-loss failure,
not a style issue, and it is the same class as the Journalist paragraph gap.

Work item 3 must additionally: derive the per-body cap from the shared
`BODY_MAX_CHARS` rather than a private 700; assert that no upstream sentence
carrying a qualification, attribution or uncertainty marker is dropped; and
record what is truncated in the input hash so a change in what was withheld
produces a new crown. Truncation must never silently change a claim.

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
2. Preserve and assess the existing name/sport/team discriminator gate. Identify generative extraction or factsweep decisions that still choose identity facts. Move classification to evaluated System 1 capabilities and deterministic policy where supported; unresolved cases remain ambiguous rather than being guessed by SmolLM3. The missing artifact is a **declared, enforced response contract** for whatever the gate's cognition slot actually is: under F1b, a plugin that reaches a model with no contract has an undefined model role, and that is the finding to name — not merely "the LLM is doing too much." Canonical promotion must fail closed when the contract is not satisfied.
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
2. Audit the remaining SmolLM3 relation/person/result extraction. Define source anchors, closed entity candidates, allowed predicates, direction, negation, time, and uncertainty before persistence. Evaluate System 1 classification and deterministic/source-span extraction for these bounded tasks; do not describe structured JSON generation as articulation. Graph's slot here is neither Harvester's Decision contract nor a character Prose contract — it emits records that become evidence. Name the contract it should have, state what its model may decide, and enforce it at the write boundary. A slot whose output becomes evidence and whose contract is only a parser is the specific gap.
3. Require semantic support as well as valid IDs and quote containment. An in-range candidate index or allowed predicate does not prove a relationship. Unknown people remain source-bound nominations for Investigator; fixture results require matching fixture identity and verified source text.
4. Audit memory and evidence aggregation. Keep extraction-origin measurement separate from junction-origin continuity so generated products cannot inflate typed links or transfer likelihood. Preserve the database provenance firewall and its consumer coverage.
5. Remove legacy Graph prompt/parser paths, Editor coupling, duplicate candidate representations, and the final recorded retirement dependencies once replacement behavior is verified. Close any remaining obsolete Editor registration, route, flag, job, packet/story consumer, and operational reference identified by earlier passes. Durable data removal requires its own migration/recovery checks. Graph needs no public voice unless it has a separately justified articulation product.
6. **Window 0 moved a dependency onto Graph.** `reporting.sql:34-37` joined
   `storyline_articles` and `storyline_entities` to group memory by storyline; F1
   removed that join from the shared study and replaced it with a
   plugin-supplied `apply_topics` function. Journalist is the plugin that supplies
   it, and it is deployed. Graph therefore still owns the storyline tables
   Journalist's memory grouping depends on. Rewriting them here changes a
   deployed plugin's memory selection. Keep the tables, or migrate Journalist's
   grouping onto a contract Graph does not own, and record which.
7. **Legacy Graph bindings and storyline membership are still known-fallible.**
   Window 2 recorded this and it was never closed. They do not prove event
   identity or independent confirmation, and they now also determine which memory
   a deployed plugin selects. Treat the grouping as a presentation aid with a
   recorded provenance class, not as evidence.
8. Run the final integration check across ingestion, source assignments, six products, identity work, fixtures, graph publication, and downstream barriers. Close all temporary-dependency entries, close the F9 `evidence/memories.rs` ledger, and update README/fleet/configuration to the actual surviving architecture.

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

The current baseline is commit `6b3ae88d`; see the [v7 frame](harvester-frame-2026-09-27.md)
for shared metadata, scalar predicates, complete bounded opening coverage, policy
and verification. The user accepts the 90/96 synthetic development result and
[defers further calibration](harvester-calibration-follow-up-2026-09-27.md) while
experience accumulates. The next plugin window can proceed. No deployment or
delivery release ran. The earlier [alignment report](harvester-alignment-2026-09-27.md)
and [v6 real-source smoke](harvester-alignment-smoke-2026-09-27.md) remain historical
evidence, not the current scoring contract or an outstanding calibration gate.

## Shared memory tool — September 29

`plugins/memories.rs` now owns the DuckDB runner, snapshot loading and provenance,
with reporting and match-statistic adapters in `plugins/memories/`. Plugins choose
the dataset, entity, window and bounds; local memory files retain request and
presentation policy. Influencer's compact dated-report selection is shared too.
Journalist, Influencer and replay callers use the shared API. Legacy
`evidence/memories.rs` consumers remain for their respective alignment windows;
F9 tracks them to a ledger closed in Window 10. Historical source passages remain
the next memory input priority: the extraction does not add missing passages or
assign reconstruction to cognition.

**Corrected by the September 29 audit.** The statement above was accurate about
the runner and the reporting adapter, and wrong about two things it did not
distinguish. The `pair` and `predicates` parameters of `reporting_scope` have no
production caller, and the only reason the shared module imports Graph is to
validate them; F1 removes both. And `statistic::team_matches` has no production
caller at all — "team-stat study engine" describes tested code, not a live
path. Scout either adopts it or Window 0 deletes it; see Window 4.

## Decision register

Recorded so a later window does not relitigate a settled question. Each entry
names the date, the decision, and the evidence it rests on.

| Date | Decision | Basis |
| --- | --- | --- |
| 2026-09-27 | Harvester v7 behavior accepted; calibration deferred. | 90/96 synthetic development routes, explicitly not gold. |
| 2026-09-29 | **Assemble = plugin. Compose prose = LLM.** The looser reading is retired. | The loose reading produced Journalist's history/report pairing defect. See the binding definition above. |
| 2026-09-29 | **`meta.rs` is correct as written and is not to be refactored.** | One 63-line type, no DB, every consumer takes `&EntityMeta`. It is the model for the other parts. |
| 2026-09-29 | **The shared memory tool must not depend on Graph.** `pair` and `predicates` are deleted, not parameterized. | Zero production callers; the dependency would break deployed Journalist when Window 10 rewrites Graph. |
| 2026-09-29 | **Grouping is a plugin-supplied function, not shared SQL.** | Keeps `storyline_*` out of the shared study while preserving Journalist's behavior. |
| 2026-09-29 | **One prose decoder with plugin-chosen keys, not one universal schema.** | Six incompatible schemas today. A universal schema would have to be the least specific plugin's, which is none of them. |
| 2026-09-29 | **One manual per plugin. `support/prompt.rs::compose` is retired, not extended.** | Four plugins compose shared writing direction from one stack. The plan says content instructions are the plugin's. |
| 2026-09-29 | **Quality fixtures store parts, not a serialized prompt.** | `fixtures/quality/vibe/*.json` were hand-edited when the package changed; `fixtures/quality/narratives/` is empty and Journalist has no `LensTask`. |
| 2026-09-29 | **`impact` / `card_score` stays. `card_score_prev` goes.** | `impact` is read by `evidence/story_parts.rs`, Oracle, `go/internal/db/db.go` and a partial index. `card_score_prev` has no reader in Rust, Go or SQL and is tied to the retired n12 palette. An earlier read of this in the audit was wrong; the corrected finding is F7. |
| 2026-09-29 | **Window 0 is numbered 0, not inserted between 1 and 2.** | Windows 1–3 are closed history cited by number across handoffs. |
| 2026-09-29 | **`statistic::team_matches` must be adopted by Scout or deleted.** | It has no production consumer. An unowned shared tool is the redundancy the plan exists to remove. |
| 2026-09-29 | **One assembler, `plugins/assembly.rs`, shares rendering and nothing else.** | The assembler existed twice with the same five keys. `form.rs:29` records that key order changes SmolLM3's output, and it was pinned only by struct field order in two files. |
| 2026-09-29 | **A `trait Assembler` per plugin is rejected.** | It adds a vtable and removes no per-plugin decision. The shared artifact is the renderer; the assembly function stays a plain function naming its own parts. |
| 2026-09-29 | **The assembler lands with F3, not before it.** | F3's per-report history nesting is what makes the two worlds the same shape. A shared renderer over two different shapes hides a divergence instead of removing it. |
| 2026-09-29 | **Key order is the plugin's, and a test pins it. There is no single canonical order.** | The two plugins already render different orders and `form.rs:29` shows order changes output. Forcing one order would change the behavior of whichever plugin had it wrong. The renderer guarantees determinism, not a particular order. Corrects an earlier draft of F4c. |
| 2026-09-29 | **The 1,200-character body ceiling is shared; the 140-character paragraph rule is per plugin.** | The body ceiling is a reader-facing product constraint. The paragraph rule is a writing policy, enforced today only by Influencer. Enforcing it on Journalist or dropping it from Influencer would make one worse to share. F4's `Dimensions` makes it a parameter. |
| 2026-09-29 | **Sharing is an optimization, not a requirement.** | A tool narrowed or restricted so a second plugin can use it is diluted, and the damage is invisible because the code now lives in one place. Where a request cannot express what a plugin needs, widen the tool or let the plugin keep its own, and record which. |
| 2026-09-29 | **The runtime has four layers, and the vision's "harness" is two of them.** | Harvester decides the destination; the harness executes it. A harness that chose destinations would be a second semantic authority with unversionable, unattributable thresholds. |
| 2026-09-29 | **F4b and F3 ship as one change to the Journalist's prompt.** All three of F2, F3 and F4b change the assembled package. Measuring the paragraph rule separately against a package F3 is about to replace produces a number nobody can use. |
| 2026-09-29 | **`Dimensions` and `Prose::schema` already exist in `plugins/cognition/prose.rs`.** F4 reuses them and supplies only the validator body. A fresh window that re-derives them has missed F1b. |
| 2026-09-29 | **F3's join rule uses `GroupSummary::source_ids`,** not inference. F2 put the article ids in the group summary, so a report indexed to a storyline already knows its own history. Rule 1 of F3 is now an exact lookup rather than a nearest-boundary guess. |
| 2026-09-29 | **`GroupSummary` carries `source_ids`.** A first draft dropped them and a provenance test caught it. A published memory claim with a count and no resolvable article ids cannot be traced to evidence, and "traceable" outranks "compact". |
| 2026-09-29 | **F1 was not behavior-preserving on its own.** Its API and test were, but removing the storyline join left the Journalist with one group per article until F2 supplied the `Topic` hook. The two had to land together; F1's own commit message overstated it and F2 records the correction. |
| 2026-09-29 | **The Journalist's self-memory stays a `CorpusItem`, not a `HistoryItem`.** It deduplicates fresh source by exact text and needs the article id and full source text the presentation type omits. Converting it to share a type would have narrowed a tool to look uniform. |
| 2026-09-29 | **F1 moved pair-name containment out of the shared study.** Accepted deliberately: it is caller policy and the shared study cannot know a caller's identity rules. The property is now opt-in, so every caller that resolves a pair must apply it. The plan notes the cost; it does not hide it. |
| 2026-09-29 | **Everything is a plugin; Harvester's cognition slot holds a System 1 model.** | Harvester already has a `PluginManifest` with its own task, claim policy, inference routes and `ToolGrant`s, plus the same prepare-then-enforce cognition shape as every character. The old "internal plugins may need no articulation call" was a description of the migration, not the target. |
| 2026-09-29 | **The contract type is the enforcement.** A character plugin cannot take a second eligibility call because it has no Decision slot. Converse: a plugin with no declared, enforced response contract has an undefined model role — which is the true finding in Investigator and Graph. |
| 2026-09-29 | **A plugin is a total function of database state.** Re-runnable against current state, correct with partial or absent upstream data, idempotent. `ClaimPolicy.upstream_tasks` is execution gating and is legitimate only as a cost measure; each window must say whether its dependency is data readiness or order. |
| 2026-09-29 | **`studio::decision` moves to the plugin layer under F1b.** | `DecisionRequest`/`PredicateQuestion` are a plugin vocabulary, not a studio facility, and their being in `studio` is why no character plugin found them. |

## Window 0 handoff

**Status: F1, F1b and F2 complete; F4 → F3 → F4c is the next window's entire
scope.** F1 is `020bc7a1`, F1b is `b2577103`, F2 is `76065db6`. Each is a separate
commit so the plan's "complete" claim is traceable to code that can be reverted
independently of the plan.

| Task | State | What landed |
| --- | --- | --- |
| F1 | complete | `reporting_scope` lost `pair`/`predicates`; `include` and `Topic` replace them; shared SQL reads no storyline table; the memory test omits those tables so it fails if the coupling returns |
| F1b | complete | `plugins/cognition.rs` names the three slot kinds and the `SLOTS` table; `studio/decision.rs` moved to `plugins/cognition/decision.rs`; `prose.rs` declares the slot with the two dimensions separated; `Prose::enforce` fails closed pending F4 |
| F2 | complete, `76065db6` | `HistoryItem`/`GroupSummary` shared; both plugins migrated; Journalist's storyline grouping restored as the first `Topic` consumer; self-memory renamed `published_reports` |
| F4, F3, F4c | next window | one prose validator; per-report history attachment; one assembler. **F4b and F3 ship together** — all three of F2, F3 and F4b change the Journalist's prompt and share one replay |
| F6, F7 | independent | evaluation harness; Journalist's dead `card_score_prev` |
| F5, F8, F9 | handed off | Window 4 and Window 10 |

Findings established, each reproduced against the working tree:

- `rg "plugins::meta" src/` — ten consumers, one definition, no per-plugin
  copies. `meta.rs` is the pattern the other parts should reach.
- `rg "ReportingHistory" src/` — Influencer only.
  `journalist/memories.rs` is 233 lines with its own SQL, budget, dedup and
  presentation, and borrows only `plugins::memories::reporting`.
- `rg "reporting_scope" .` — callers are `memories.rs` itself, `memories/tests.rs`
  and `examples/memory_request.rs`. No production caller passes `pair` or
  `predicates`.
- `rg "team_matches|memories::statistic" .` — `memories/tests.rs` and
  `examples/memory_request.rs` only.
- `rg "compose" src/` — `scout/cognition/brief.rs`, `analyst/cognition/prompt.rs`,
  `insider/cognition/brief.rs`, `oracle/cognition/brief.rs`. The six content
  constants have no consumer outside `support/prompt.rs` itself and its test.
- `rg "card_score_prev" go/ sql/ rust/src/` — Rust writer, schema column and
  comment, migration 186, one test. No reader. `rg "\bimpact\b" go/internal/
  sql/schema/schema.sql` — live in Go, Oracle and storyline trajectory.
- `all_task_names()` — eight entries, no `"narratives"`.
  `ls fixtures/quality/narratives/` — empty.
  `src/bin/eval.rs:558` generates from `fx.user_prompt`, a stored string.
- `ls src/plugins/*/manifest.rs` — ten plugins have one; Harvester's is inside
  `harvester.rs:15-25`. All eleven register through `fleet::ALL`, so the
  "everything is a plugin" shape is real in the code and not an aspiration.
- `rg "studio::decision" src/` — four modules, all of them either Harvester or
  the System 1 provider. No character plugin used the vocabulary that already
  existed for the other contract shape.
- `cargo build --all-targets` — clean at `4972fa7c`; 529 tests before F1b.

**Unrun checks and why:** no model replay was run for this audit; it changed no
behavior. Every replay named in F2, F3, F4a and F4b is a gate on the change that
follows it, not a pending item here. The two shape questions the audit could not
settle from code alone — whether Influencer wants bounded historical passages,
and what single paragraph ceiling Journalist should use — are decided in Window 0
F4b and Window 3 respectively, against replayed cases.

**Next interface:** Window 0 hands the remaining windows one memory item type, one
prose decoder, one manual per plugin, and a live evaluation harness. Windows 4
through 10 consume those; none of them introduces a second memory
implementation, a second parser or a second prompt composer.

## Next fresh context: the F4 → F3 → F4c chain

Read, in this order and nothing else: the ownership contract, the parts contract
table, the "Sharing is an optimization" rules, Window 0's F4/F3/F4c sections, and
the decision register. Then `HANDOFF-influencer-2026-09-28.md` for the
parts-and-manual pattern and the n94 fixtures under `fixtures/journalist/`.

**Where the work stands.** F1, F1b and F2 are complete: `020bc7a1`, `b2577103`,
`76065db6`, with plan updates in `860b284d`, `91492981` and `8a58f2f6`. 534 tests
pass and the tree was clean at `8a58f2f6`. F4, F3 and F4c are the entire scope of
this window. Do not start F6, F7, F5, F8 or F9 here.

**Why this order, and do not rearrange it.** F4 makes the shared prose contract
real, F3 makes the two character worlds the same shape, and F4c then has one
honest renderer to share. F4c before F3 would be a shared renderer over two
different shapes, which hides a divergence instead of removing it. F3 before F4
would nest history per report and then re-derive the surface the validator
enforces.

**The two gates that are not negotiable.**

1. **F4b and F3 are one release.** Both change the Journalist's assembled package
   and therefore its prompt. F2 already changed it. Measuring the paragraph rule
   against a package F3 is about to replace produces a number nobody can use.
   Land them together, replay once.
2. **The replay gate.** Nothing here ships until the n94 Journalist replays and
   the Influencer v3 replays are recorded. Influencer's must be byte-identical for
   F4a. Journalist's must be reviewed for fidelity as before — parser success is
   not sufficient and never was.

**Known landmines in this chain, all found the hard way.**

- `Prose::enforce` currently *fails closed* by design, and there is a passing test
  asserting that. F4 must invert or replace that test. If it still passes
  unchanged after F4, enforcement was never wired in and the declaration is a
  lie.
- `plugins/cognition.rs::SLOTS` marks Journalist and Influencer `enforced` today,
  meaning their *existing* parsers enforce. Do not move that to mean the shared
  validator is in use until it genuinely is.
- F3's `fits` budget callback currently measures the whole rendered context.
  Per-report attachment changes what it measures; carrying the old measurement
  forward silently changes the budget.
- A report can end up with history while another has none. That mixed shape needs
  a recorded decision in both the manual and the test, not a default.

**Carry forward as fixed.** Harvester remains v7 with calibration accepted.
Influencer v3 is local and undeployed, and its retired abstention and classifier
choice stay retired. Do not reopen Window 1, Window 2's prose behavior, or Window
3's architecture. Do not revisit F1 or F2; if something there is wrong, say so
rather than quietly reworking it inside this chain.
