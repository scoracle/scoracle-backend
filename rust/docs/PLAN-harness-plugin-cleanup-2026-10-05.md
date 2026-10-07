# Harness and plugin cleanup — completed

> **2026-10-06 governing update:** This dated record preserves its implementation and evaluation evidence. The [current contract](../../../scoracle-wiki/wiki/Architecture/Harness,%20Plugins,%20Tools,%20and%20LLM%20Contract.md) supersedes articulation-only ownership: cheap code owns collection; expensive compute owns discovery. Plugins assemble traceable clues through tools; the LLM reasons across them, discovers what they support and articulates the answer. Historical “plugin owns WHAT/model owns HOW” statements below do not govern new builds.

Completed October 5, 2026. [rust/README.md](../README.md) owns the current architecture contract. All seven cleanup steps are complete, committed and pushed; implementation checkpoint `0098df4e` is deployed on Archbox. Validation and rollout evidence are recorded below and in the [deployment record](harness-cleanup-deployment-2026-10-05.md).

**SQL owns the data. Rust owns the cognition.** The harness runs a plugin. The plugin calls tools, selects and assembles the world, invokes its model where needed, validates the response and publishes to Postgres. Models articulate or classify supplied input and never call tools.

## Status

| Step | State |
| --- | --- |
| 1. Direction and local voices | Done. |
| 2. Scout preparation and legacy memory package | Done. Preparation and render are `prompt.rs`; measurements and SQL reads are `performance.rs`; window-study policy is `memories.rs`. Execution and product creation are `mod.rs`; response guards are `parser.rs`; product SQL and claim-fenced publication are `publish.rs`; source admission/delivery is `delivery.rs`. `adapter/` and `cognition/` are deleted with all callers migrated. Identity reads are `tools/meta.rs`. Publication fences unchanged. |
| 3. Scout data | Done. Trajectory query is the producer, parity-checked on Archbox. Notability stays beside selection. Reporting `strpos` prepared on Archbox. Event-based candidate selection not replaced. |
| 4. Influencer | Done. Preparation and rendering are `prompt.rs`; execution is `mod.rs`; SQL/receipt helpers are `publish.rs`. `cognition/` and `adapter/` deleted with callers migrated. `memories.rs` retains real selection policy. Tool pilot and exclusive provider chat/XML path retired. |
| 5. Other plugins | Done. Rendering and preparation/execution ownership done for Journalist, Insider, Analyst and Oracle. Their obsolete directories and `assembly.rs` are deleted. Journalist source activity and Insider transfer heat/card scores are SQL-owned with isolated PostgreSQL parity. Analyst direction/conviction are also SQL-owned with isolated PostgreSQL parity. Oracle score/convergence/omen are SQL-owned with isolated PostgreSQL 18 parity. All four planned metric migrations are complete. |
| 6. Editor and old namespaces | Done. Editor implementation, executable evaluation task and obsolete eval fixtures removed. Top-level cognition namespace removed; form vocabulary and decoding are consolidated in `tools/form.rs`, and Harvester owns its decision protocol. Support namespace removed; surviving tools, correction policies and slot constants have direct owners. Evidence namespace removed: shared fetch/identity/memories are `tools/`; Scout source readers and contested reports and Graph quote/result helpers have local owners. The reporting forwarder is deleted. |
| 7. Harness folder move | Done; deployed on Archbox. Registration, execution, model contracts, routing, configuration, DB, diagnostics and capability broker are `harness/`; queue and providers retain substantial subfolders. Old namespaces/exports removed; operator, evaluation, example and crash-rehearsal callers migrated. |

Deleted after a caller check: `studio/palette.rs`, `evidence/story_parts.rs`, `dedupe_i64`, `classify_delta`, `trajectory_label`, and the unused `RunDeadline` / `handler_budget` wiring. `DEFAULT_TRAJECTORY` stays. Worker `handler_timeout` stays; that one is enforced.

## Separate follow-ups

- **Context work:** reporting candidate selection, verified Scout reporting and linked entities are deferred to a separate session at the user's request. Prepare and review an inspectable before/after request before changing model input.
- **Mac operations:** the update is staged but blocked by the new binary's network failure. The Mac worker is paused with explicit user approval; Archbox runs the updated fleet. Recovery is tracked in the [deployment record](harness-cleanup-deployment-2026-10-05.md).

These follow-ups do not reopen the completed cleanup scope.

## The simplification rule

Share tools when sharing reduces complexity or fragility. Keep tools plugin-specific when that reduces complexity or fragility. Use plain functions and existing types. A tool needs no registry, trait or configuration language.

The useful object-oriented concepts are already present: plugin handler structs encapsulate dependencies; composition assembles each plugin; `StudioPlugin` lets one worker dispatch different implementations. Retain that execution seam, `PluginManifest`, real provider interfaces and typed parsers. Each plugin owns its workflow and product; a common execution contract does not require a common context shape or model protocol.

## File responsibilities

| Location | Responsibility |
| --- | --- |
| Each plugin's `prompt.rs` | Readable Rust entry point for tool calls, scopes, model-input assembly and task instructions. Calls local helpers when clearer; does not absorb every query or parser. |
| Each plugin's `manifest.rs` | Existing identity, task, claim, resource and capability policy. |
| Each plugin's `mod.rs` | Execution wiring, preparation, optional inference, validation and publication. |
| Each articulation plugin's `voice.rs` | One small tone constant owned by that plugin. Harvester and other non-expressive jobs need no voice. |
| Shared or local tools | Ordinary typed functions, located where they are simplest to understand and maintain. |

The six active voices now live at their plugin roots, with their existing wording unchanged. The shared voice catalog has been removed. The other file responsibilities are migration targets. Retain typed helpers and publication modules where they carry real work; remove layers that only forward to another layer.

## Harvester and Oracle

| | Harvester | Oracle |
| --- | --- | --- |
| Job | Acquire source, classify relevance and deliver exact attributed context. | Articulate the current picture from five finished character products. |
| Shared identity | Uses `EntityMeta`; reuse metadata already loaded with article/query provenance. | Uses `EntityMeta`; load canonical identity with the selected product scope. |
| Local tools | Acquisition, exact paragraph/window selection, predicate preparation, classification and routing. | Finished-card queries, card selection, availability and product acceptance. |
| `prompt.rs` | Constructs target-bound relevance/theme questions and controls the classification recipe. | Calls the card reader, assembles the selected cards and provides synthesis instructions. |
| Voice | None: bounded classification. | Uses Oracle's local `voice::VOICE`. |
| Model input | Existing typed `DecisionRequest`, with source text and predicate questions. | Existing ordered world, with selected finished cards, tone and form. |
| Publication | Source receipts, assignments, links, progress and downstream obligations. | Claim-fenced terminal synthesis product. |

Keep these workflows distinct. Harvester may make multiple bounded classification calls for its windows. Character articulation has one generation stage with the existing bounded structural correction policy. Deterministic jobs and insufficient-evidence paths need no inference.

Current code: [Harvester execution](../src/plugins/harvester/adapter.rs), [classification requests](../src/plugins/harvester/cognition.rs), [worker/replay context](../src/plugins/harvester/context.rs), [Oracle execution](../src/plugins/oracle/mod.rs), [Oracle input](../src/plugins/oracle/prompt.rs).

## Context compilation: current and proposed

Production characters already prepare input in Rust and invoke inference without tools. The problem is locating that preparation, duplicated preparation paths and obsolete guidance promoting model-directed retrieval.

| Current Scout | Proposed Scout |
| --- | --- |
| Shared preparation and rendering are `prompt.rs`; ordinary and Harvester delivery call that entry. Measurements and SQL reads are `performance.rs`; window-study policy is `memories.rs`. | `prompt.rs` calls shared or local functions with Scout's performance scope. Do not fold adapter SQL into it. |
| The legacy package is gone. The fingerprint covers the rendered world and attached provenance. | One selected SQL/DuckDB study path, with fingerprints covering the input actually used, contract versions and required provenance. |
| Statistical profile is rendered as `fresh`; optional memory contains measured history, reported context and coverage limits. | Preserve that input in the first refactor. Then deliberately add verified reporting and linked entities, showing the exact proposed JSON before changing inference input. |
| Harvester-triggered Scout work marks source as `trigger_only` and disables ordinary enrichment. | Source remains attributed context alongside performance, with explicit coverage. Reports cannot manufacture a measurement or ranking. |
| Rust computes several stored metrics alongside preparation and publication. | SQL/DuckDB computes factual metrics; Rust selects context and controls inference and acceptance. |
| Ordinary and Harvester delivery have distinct commit paths. | Keep both sets of claim fences, source revalidation, dispositions, progress and follow-up effects. Simplify preparation first. |

Current Scout wire shape, reduced to show the actual part names:

```text
system: Scout task instructions
input:
  meta: name, entity_type, sport
  fresh: selected statistical profile and its sample/comparison limits
  memories: measured, reported, coverage_limits   [when present]
  voice: Scout tone
  form: body structure and limits
```

Proposed preparation and eventual input, schematic rather than an accepted JSON change:

```text
Scout prompt.rs
  → canonical metadata
  → performance query and scoped memories study
  → verified fresh reporting for Scout's assignment
  → linked_entities query when useful to this task
  → Scout's local voice::VOICE and form
  → render the selected world

system: Scout task instructions
input:
  meta: subject and relevant canonical attributes
  fresh: performance measurements + attributed reporting
  memories: selected performance findings and relevant reported context
  linked_entities: entity metadata, co-mention counts and sources [when used]
  voice: Scout tone
  form: body structure and limits
```

Tool selection is code in `prompt.rs`; the model receives only the resulting input. Plugin scopes differ: Journalist's memories concern reporting/narrative continuity, while Scout's concern performance. Share the existing study runner and compatible studies; local queries and presentation remain valid when simpler.

`prompt.rs` owns context construction and serialization; there is no separate assembly layer in the target. Replace the current `assembly::World` with plugin-local `Serialize` context structs and one `serde_json::to_string` call. All six current production shapes have fixed field order and optional memories; native struct serialization preserves that order. Keep typed nested data unchanged, including existing nested JSON map ordering. Replace the two direct world-hash uses with the existing hash helper over the rendered input; retain version/provenance components. Request-byte and fixture comparisons must pass before deleting the renderer. Harvester keeps its typed requests. Full provenance stays available for inspection outside compact model context.

## Folder cleanup

The production target is **harness → plugins → shared and specialized tools**. Three source homes express that directly:

```text
src/
  harness/                 # register, claim, run, model transport, commit/recover
    queue/                 # existing substantial claim/publication/outbox code
    providers/             # existing real model transports
  plugins/
    scout/
      mod.rs
      manifest.rs
      prompt.rs            # all model-context preparation
      voice.rs
      performance.rs       # example specialized tool, only when needed
      publish.rs           # product SQL and acceptance helpers when substantial
    harvester/             # its own tools and classification recipe; no voice
    oracle/                # its own card reader, recipe and voice
    ...other active plugins...
  tools/
    meta.rs
    fresh.rs
    memories.rs
    linked_entities.rs
    form.rs
```

This is a responsibility map, not scaffolding to create in advance. Keep executable entry points, tests, fixtures, evaluation utilities and documentation outside the production flow. A folder remains only when it groups substantial concrete work; a namespace that only forwards to the next namespace goes away.

| Current location | Planned disposition |
| --- | --- |
| Each plugin's `cognition/parts.rs` | Move the recipe, model-input projection and rendering into `prompt.rs`, then delete the file. Scout's measurement types, sample limits and guards remain in a local performance tool; retained fixture inputs must still replay through production preparation. Scout: done. |
| `plugins/assembly.rs` | Deleted. All six recipes render with `serde_json::to_string`. |
| Each plugin's `cognition/` | Move task/context into `prompt.rs`, tone into local `voice.rs`, and real parsing/validation into local helpers; remove the obsolete directory and exports. Avoid moving Scout's entire 1,500-line module into one prompt file. |
| Each plugin's `adapter/` | Put the readable execution entry in `mod.rs`; keep substantial local data/publication tools in named files. Remove forwarding modules as callers migrate. Harvester retains its distinct acquisition and classification workflow. |
| `plugins/cognition.rs` and `plugins/cognition/` | Deleted. `Prose`/`Dimensions` and shared decoding live together in `tools/form.rs`; `DecisionModel` and its request/response types live in `plugins/harvester/decision.rs`. All callers migrated without a compatibility facade. |
| `plugins/support/` | Deleted. Shared form/source/guards live in `tools/`; correction policies live in `harness/session.rs`; slot constants live in `harness/fleet.rs`. Graph owns its sole-use identity framing. Unused digit helper removed. |
| `plugins/memories.rs` and `plugins/memories/` | Moved to `tools/memories.rs` with the statistic adapter folded into it; reporting SQL and integration checks remain alongside it. |
| Scout/Journalist/Influencer `memories.rs` | Put small scope/selection policies in `prompt.rs`; retain substantial specialized study or continuity logic as a local tool. Delete redundant wrappers; Influencer's current file carries selection policy and stays. |
| `evidence/memories.rs` and `evidence/memories/` | Retired. Identity reads are `tools/meta.rs::load_identity_record`. |
| `studio/`, `application/`, `runtime/` | Removed. Infrastructure lives directly in `harness/`, with substantial queue/provider subfolders retained. Registration is `registration.rs`; tool policy and the capability broker share `tools.rs`. Existing `StudioPlugin`, manifests, provider interfaces and creation session remain. |
| `evidence/` | Removed. Acquisition is `tools/fetch.rs`; display-name lookup joins `tools/meta.rs`; Scout reads/reports and Graph quote/result helpers are local. `DEFAULT_TRAJECTORY` joins `tools/source.rs`. Caching, pacing, retry, budget, provenance and source integrity are preserved. |
| `plugins/editor/` | Deleted with its executable evaluation task and, at the user’s request, obsolete eval fixtures/rubrics. Historical database tables and stored data remain. |

Deleted after a caller check: `studio/palette.rs`, unused `RunDeadline` / `handler_budget` wiring, `evidence/story_parts.rs`, `dedupe_i64`, and the trajectory helpers that only the story helper used. `DEFAULT_TRAJECTORY` remains. Check references immediately before the next deletion.

Folder cleanup is part of each plugin slice, not a cosmetic rename at the end. Each slice must remove superseded imports, exports, forwarding modules, prompts and exclusive helpers. Track any live caller that prevents deletion and finish its migration before declaring the slice complete.

## Shared context tools

- **Meta:** add a reusable canonical loader to the existing type where it removes repeated reads. A plugin that already has the metadata can construct the type directly. Reuse SQL current-identity projections for team/league attributes.
- **Memories:** keep the existing bounded Postgres snapshot and Go DuckDB runner. The reporting title predicate calls `strpos` with two arguments and prepared on Archbox. Candidate selection is still event-based. Replace that before relying on new Harvester history, and show the before/after request first. Keep study selection with the plugin.
- **Fresh:** reuse `harvester::delivery::load_for_character` and `tools::source` behind one shared reader where callers fit. Preserve plugin-specific assignment eligibility, source integrity and publication revalidation. Oracle's finished cards and Harvester's acquisition are different local tools.
- **Linked entities:** one thin context reader returning entity metadata, co-mention frequency and sources. SQL uses complete fetched articles and existing name/alias matching. Both entities must occur in the same article; count once per canonical article and retain dates and source references. Preserve uncertainty for ambiguous/fuzzy identity and missing affiliation. The plugin selects the scope; 90 days is an initial recommendation, not a global requirement.
- **Voice:** local to each articulation plugin. One short tone constant; no shared catalog or lookup machinery.
- **Form:** share compatible rendering/decoding; output fields and acceptance rules remain plugin-specific.
- **Web search:** add only for a plugin's concrete live-retrieval job. Scout reads Harvester's retained source.

The linked-entity reader is not a separate workflow. Reuse SQL normalization, aliases, stored full text and canonical metadata. The existing co-mention projection lacks source references and full-body coverage, so a scoped SQL query may be needed. Preserve existing Graph/Insider delivery semantics when adding full-article context reads. Review the exact returned JSON before serving it to a model; measure query cost before adding persisted projections or caching.

## Keep, simplify and retire

| Priority | Decision | Reason |
| --- | --- | --- |
| Keep | Static plugin registration, `StudioPlugin`, manifests and concrete dependency binding. | Already provide the harness and heterogeneous execution boundary. |
| Keep | Queue leases/revision fences, atomic product/outbox/completion, partial progress, recovery and source receipts. | Real durability and integrity requirements. |
| Keep behavior, simplify mechanism | Explicit input field order, provider seams, typed parsers and active factual guards. | Preserve model behavior with native serialization while removing assembly layers. |
| Simplify first | Scout preparation and its duplicate legacy memory package. | Largest active obstacle to understanding what the model sees. |
| Done | Influencer tool pilot, its example, and the exclusive provider chat/XML path. | No production caller. Dated evaluation remains evidence. |
| Simplify as callers migrate | Per-plugin source wrappers, redundant route maps, unused deadline/palette helpers and prompt aliases. | Reuse working functions and delete forwarding layers. |
| Done | Retired Editor, obsolete executable evaluation and fixtures. | Removed from source and eval selection; historical database records remain. |

The audit's provisional eventual reduction is about 6,000 lines with no new dependencies, including the separate Editor pass. This is an estimate, not a target that justifies removing safeguards.

## Migration sequence and acceptance

1. **Direction and local voice:** update the active README, add a short supersession banner to the completed alignment plan and preserve dated evidence. Keep each active voice local with identical text; remove the shared catalog. Record the folder/deletion map above.
2. **Scout preparation and pruning:** read-only preparation now lives in `prompt.rs`, SQL profile/trajectory reads join `performance.rs`, and window-study selection joins `memories.rs`. Both delivery paths and operator/evaluation callers use one entry; its forwarding wrapper is deleted. Measurement selection now joins `performance.rs`, response guards live in `parser.rs`, and execution/creation live in `mod.rs`. Publication and source delivery retain their own substantial tools. The old directories and source execution forwarder are deleted. Native serialization, parts-file deletion, and the legacy memory package are gone. Rendered requests and both publication paths were preserved. The debounce hash no longer includes the unused package fingerprint.
3. **Scout data and context:** trajectory arithmetic now runs in the load query, with Archbox parity. Notability stays beside selection. Replace event-based reporting selection only after a before/after request. Preserve no-stats outcomes and measurement guards. Do not add verified reporting or linked entities before that request.
4. **Influencer:** done. Read-only preparation and native rendering live in `prompt.rs`, execution in `mod.rs`, publication SQL/receipts in `publish.rs`. The old preparation directories, source forwarder, task alias and execution forwarder are gone. Worker, evaluation, replay and downstream completion callers use the new paths. `memories.rs` retains its selection policy.
5. **Other plugins:** all six articulation worlds render with native serialization, and `assembly.rs` is deleted. Journalist, Insider, Analyst and Oracle ownership cleanup is done: preparation/rendering/options live in `prompt.rs`, execution in `mod.rs`, acceptance in `parser.rs`, and substantial SQL/ledger publication in `publish.rs`. Journalist retains its substantial continuity tool; Insider retains team and player/coach delivery semantics; Analyst retains finished-card dependencies and queue reactions. Their obsolete directories and forwarding layers are deleted. Oracle retains its five-stage completion barrier, partial-card policy and empty-card marker. Journalist source activity is now computed by its local SQL query after source selection, with isolated PostgreSQL parity; its legacy formula survives only as a test oracle. Insider grounded transfer heat and card scores now use its local SQL query inside the claim-fenced publication transaction; exact source/counterparty validation and rumor selection remain in Rust. Analyst direction/conviction now come from its local SQL query before inference, using the selected raw snapshot score. Oracle score/convergence/deterministic omen now come from its local SQL query before inference, using the selected finished cards. All four metric migrations are complete with separate live parity evidence. Keep Harvester, Investigator, Boxscore and Graph's distinct jobs and model protocols.
6. **Legacy retirement:** Editor implementation, obsolete executable evaluation and eval fixtures are removed. The user explicitly requested removing the old evals too, superseding the initial fixture-retention decision; stored historical data remains. The top-level cognition namespace is removed and form vocabulary/decoding are consolidated. Support is removed: shared tools are in `tools/`, while correction policies and slot constants stay with execution/fleet mechanics. Evidence is removed with its callers migrated to direct shared/local owners.
7. **Harness consolidation:** complete; deployed on Archbox. Studio/application/runtime infrastructure lives directly in `harness/`, retaining useful queue/provider subfolders. Production business logic stays in plugins and tools. Old exports and paths are removed together; operator/evaluation/example entry points and subprocess rehearsal filters use the new owners.

Metric migrations preserve formulas, selected inputs, null behavior, thresholds, ordering and rounding. Scores/categories must match exactly; raw regression slope allows a 1e-9 tolerance. Keep one active producer after cutover. Semantic extraction, quote/source validation, model instructions and acceptance stay in Rust. Keep current Go acquisition and analytical helpers; preserve Analyst/Oracle's existing product dependencies.

Before changing a model's input, retain an inspectable before/after request, its selected data and provenance. Reuse production preparation for replay. Run the smallest relevant checks for each slice; SQL changes require a real integration/parity check, because existing ignored DB tests do not verify them. Compilation and local tests do not establish deployment or model-output quality.

## Historical checkpoints and validation — October 5, 2026

The following records describe each slice at its checkpoint. Their original source paths and deployment limits are historical; the completion status and deployment record above are current.

Reviewed the current working tree against this plan. The Scout rendering/measurement split, legacy-memory removal, identity-reader migration, SQL trajectory cutover, reporting-predicate repair and Influencer pilot retirement are the checkpoint scope. Archbox parity above is evidence recorded by the prior work; this checkpoint does not repeat that remote run or establish deployment. Checkpoint committed as `df05d894`. The subsequent native-renderer/dead-code slice passed `cargo test --all-targets` (419 library tests and 14 eval tests; 63 DB-dependent tests ignored). Formatting was checked before its commit.

The native-renderer/dead-code checkpoint is `d7a59629`. The Influencer ownership slice follows it, preserving request bytes, source admission, memory policy and publication fences. Influencer ownership is committed as `99ef4c15`. Scout preparation ownership now follows it. Scout parsing and execution cleanup follows the preparation ownership checkpoint `eb99f08a`. Scout directory cleanup is committed as `f8b0385b`. Journalist ownership cleanup is committed as `14c9b17d`. Insider ownership cleanup is committed as `4ca9af42`. Analyst ownership cleanup is committed as `8e585b75`. Oracle ownership cleanup is committed as `2c7028ad`. Journalist source-activity SQL migration is committed as `60b92efe`. Insider grounded transfer heat SQL is committed as `af342280`. Analyst direction/conviction SQL is committed as `b36f7ede`. The branch was refreshed against origin and verified current at `b5f04a91`, including the pushed CI/helper-prerequisite fixes. Oracle score/convergence/deterministic omen SQL is committed as `124e5cef`. Editor retirement is committed as `65f2ef38`, including obsolete evals at the user’s request. Shared form vocabulary/decoding consolidation and top-level cognition removal are committed as `8fad7dc3`. Support dissolution is committed as `b39a37d2`. Evidence dissolution and harness consolidation are committed as `0098df4e`; validation is recorded below. The four SQL metric moves have their separate integration/parity evidence above. Scout reporting and linked-entity input changes remain gated by an inspectable before/after request.

Influencer slice validation: `cargo test --all-targets` passed (419 library, 14 eval, 1 factsweep and 2 statcommentary tests; DB integration tests remained ignored). The 12 audit replay requests exported before and after the ownership move compare byte-for-byte, including provider options and schema. Preparation and claim-fenced publication bodies also match the checkpoint after path changes. A local no-DB admission check verifies rejected sources retain their receipt before memory access. No SQL or model input changed, and no deployment was performed.

Scout preparation slice: the preparation body, performance SQL reads and measured-window policy match the checkpoint after path changes; the rendered world, debounce material, ordinary/source publication paths and source-admission semantics are unchanged. `adapter/evidence.rs` and the redundant preparation wrapper are deleted. Verification: `cargo check --all-targets` passed; `cargo test --lib plugins::scout` passed 73 tests with 5 DB-dependent tests ignored, including rendered-request fixtures and measurement guards; formatting and diff checks passed. That checkpoint left `adapter/` and `cognition/` intact; the following directory slice removes them.

Scout directory slice: performance selection/types were moved intact into `performance.rs`, response validation and prose schema into `parser.rs`, assignment/options into `prompt.rs`, creation and queue policy into `mod.rs`, and substantial publication/source workflows into `publish.rs` and `delivery.rs`. Existing offline and DB integration checks move with their owners. No compatibility facade, new dependency, metric formula, SQL statement, model input or publication policy is introduced. Validation passed: `cargo check --all-targets`, `cargo test --all-targets` (437 passed, 64 DB-dependent tests ignored), formatting and diff checks. Seven frozen-part replay requests compare byte-for-byte before/after, including system instructions, schema and provider options. One older capture still fails parts deserialization with `missing field previous`, exactly as before. Creation, parser, publication and source-delivery bodies also match the checkpoint after visibility/path changes. No deployment is implied.

Journalist ownership slice: source admission, continuity attachment, parts and request options now live in `prompt.rs`; execution and product creation live in `mod.rs`; strict edition parsing and unchanged source-activity arithmetic live in `parser.rs`; claim-fenced SQL publication, dispositions, partial progress, outbox and ledger live in `publish.rs`. The substantial `memories.rs` policy remains local. Worker, evaluation, replay and Oracle completion callers use the new paths. The old `adapter/` and `cognition/` directories, rendering forwarder and task alias are deleted. Validation: `cargo check --all-targets`, `cargo test --all-targets` (437 passed, 64 DB-dependent tests ignored), formatting and diff checks passed. Fourteen offline replay captures compare byte-for-byte, including rendered requests, system instructions, provider options/schema, hashes, dispositions and deferrals. Admission, parser, activity formula, source metadata, creation, execution and publication bodies match the prior checkpoint after path/visibility changes. No SQL, model input, metric formula or deployment changed; SQL metric migration still requires its own live parity evidence.

Insider ownership slice: source loading, canonical co-mentions, prior reporting scope, measured publisher records and native requests now live in `prompt.rs`; response types, creation, backend binding and claim checks live in `mod.rs`; exact-source-span and served-prose guards live in `parser.rs`; rumor/score publication, unique counterparty resolution, denial ordering, source dispositions, outbox and ledger live in `publish.rs`. The old `adapter/` and `cognition/` directories, renamed export facade and source execution forwarder are deleted. Worker, evaluation, Oracle completion, outbox recovery and Harvester integration callers use the new paths. Validation: `cargo check --all-targets`, `cargo test --all-targets` (437 passed, 64 DB-dependent tests ignored), formatting and diff checks passed. Nine offline request captures (three retained transfer cases rendered as player/team/coach worlds) compare byte-for-byte, including system instructions, schema and provider options. Preparation, rendering, parser, activity/denial policy, creation, publication and event bodies match the checkpoint after path/visibility changes. Team and player/coach delivery paths, exact quotes, ambiguous-counterparty rejection and claim fences remain intact. Heat arithmetic and SQL are unchanged; SQL migration still needs separate live parity evidence. No deployment or model-quality claim is implied.

Analyst ownership slice: finished Scout/Influencer reads, dated snapshot retrieval, input components/hash, native rendering and provider options now live in `prompt.rs`; product creation, unchanged direction/conviction calculations, enqueue policy and completion reactions live in `mod.rs`; prose decoding and production guards live in `parser.rs`; claim-fenced product SQL and diagnostic ledger live in `publish.rs`. The old `adapter/` and `cognition/` directories are deleted. Creation's existing optional output now reaches publication directly, removing the redundant prepared-product enum, boxing and forwarding helper. The unused ledger capability argument is removed. Worker, Scout enqueue, evaluation, Oracle completion, queue recovery and shared-form checks use the new paths; the subprocess crash-rehearsal filter follows its relocated test. Validation: `cargo check --all-targets`, `cargo test --all-targets` (437 passed, 64 DB-dependent tests ignored), formatting and diff checks passed. Eight offline captures cover every combination of Scout, Influencer and trajectory availability and compare byte-for-byte, including requests, system/schema/options, fingerprints, input components and no-material decisions. SQL reads, rendering, fingerprint construction, formulas, parser, creation, queue policy and publication bodies match the checkpoint after ownership changes and the equivalent optional-output representation. SQL, model input and metric formulas remain unchanged; live SQL parity remains a separate gate. No deployment or model-quality claim is implied.

Oracle ownership slice: finished-card SQL reads, card/readiness types, native projection, input components/hash and provider options now live in `prompt.rs`; its read-only assignment loader replaces duplicate empty/nonempty assignment construction. Backend binding, debounce, five-stage completion barrier, product creation and unchanged score/convergence/omen arithmetic live in `mod.rs`; served-reading guards live in `parser.rs`; crown/marker SQL and claim-fenced completion/ledger live in `publish.rs`. Worker, evaluation and Analyst season-resolution callers use the new paths. The old `adapter/` and `cognition/` directories, render/export facade, sentence-counter forwarder and unused duplicate output-reservation/omen constants are deleted. The real prepared publication state remains because it carries debounce and previous-score policy. Validation: `cargo check --all-targets`, `cargo test --all-targets` (437 passed, 64 DB-dependent tests ignored), formatting and diff checks passed. Thirty-two offline captures cover all five-card availability combinations and compare byte-for-byte, including requests, system/schema/options, readiness and input components; the narrative cases also exercise highest-impact ordering and the three-report limit. SQL readers, completion barrier, input types/provenance, rendering, parser, creation, formulas and publication bodies match the checkpoint after paths/visibility and unused-constant removal. Empty cards retain their uncalled NULL marker; nonempty unchanged cards debounce before inference; failed upstream stages retain their settled policy. No SQL, metric formula, model input or deployment changed. All six articulation ownership moves are now complete; the following Journalist source-activity SQL migration satisfies its integration/parity gate.

Journalist activity SQL slice: `activity.sql` is the sole production producer of per-report source activity and aggregate card score, operating only on the already admitted and selected reports. `activity.rs` preserves Rust publisher casing at the input boundary and existing component JSON types; the old formula remains only as a test oracle. Creation loads activity before inference; an empty selection skips both, and query failure prevents a model call. Selected order, dates, source metadata, request bytes, input hashes, parser guards and publication policy are preserved. Validation: `cargo check --all-targets`, `cargo test --all-targets` (438 passed, 65 DB-dependent tests ignored), formatting and diff checks passed. The new ignored parity test was explicitly run against isolated local PostgreSQL 17.11: 1,089 cases matched exact scores and component JSON across source counts, case/Unicode/whitespace identity, null/future dates, recency boundaries and extreme i64 epochs. Mixed dates verified newest-source selection; one served-product check verified score, components, provenance and exactly one model call. Fourteen preparation/request captures remain byte-for-byte identical. Preparation replay remains offline; optional articulation replay now requires `JOURNALIST_REPLAY_DATABASE_URL` for the pure SQL calculation. The temporary PostgreSQL server was stopped after verification. This is local parity evidence, not an Archbox rerun, deployment or model-quality claim. The following Insider grounded transfer heat SQL slice satisfies its separate live parity gate.


Insider activity SQL slice: local `activity.sql` is the sole production producer of transfer heat and card activity scores. It selects each exact-name counterparty's current reported run in delivered report order, excludes everything after its first intervening denial, and applies unchanged stage bands, lowercase-only publisher identity, publisher bonus and cap. Rust retains exact source/counterparty validation, rumor grouping and receipt metadata, direction resolution and explicit denial heat zero. SQL runs inside the existing publication transaction after claim/source validation; query failure cannot leave partial products. The legacy Rust score formula remains only as a test oracle, verified unchanged against the prior checkpoint; `current_run` and all preparation, request, parser and execution files are unchanged. Validation: `cargo check --all-targets`, `cargo test --all-targets` (438 passed, 66 DB-dependent tests ignored), formatting and diff checks passed. The new ignored parity test was explicitly run on isolated local PostgreSQL 17.11: 2,036 exact-score comparisons covered every report/denial pattern through six reports, finding-order permutations, separate case/whitespace counterparty identities, Unicode/case/whitespace publisher identity, all stage bands, selected rumor runs and saturation; missing source indices are rejected. The existing claimed Harvester/Insider integration smoke also passed against the isolated schema-bearing database, now asserting reported rumor heat 72, denied heat zero, and exact team/player card scores as well as source receipts, outbox completion and stale-claim/changed-source rejection. The temporary server was stopped afterward. No model input, source-selection policy or deployment changed. The following Analyst direction/conviction SQL slice satisfies its separate live parity gate.


Analyst metrics SQL slice: local `metrics.sql` is the sole production producer of direction and conviction from the selected snapshot's raw float8 momentum score. SQL preserves the ±10 direction boundaries, half-steady-band lean, conviction ladder and ±5 saturation without rounding before classification; null stays steady/zero and nonfinite float behavior matches the Rust oracle. Creation skips SQL and inference for empty material; a SQL failure prevents the model call. The old formulas remain test-only oracles. A private articulation function consumes SQL values, keeping parser/model/provenance checks offline through the test-only oracle; production creation always uses SQL. Preparation, finished-card and snapshot selection, request projection/options, parser, hashes/components, queue policy, publication and ledger bodies are preserved. Validation: `cargo check --all-targets`, `cargo test --all-targets` (439 passed, 67 DB-dependent tests ignored), formatting and diff checks passed. Explicit isolated PostgreSQL 17.11 checks passed 2,046 exact direction/conviction comparisons covering a full tenths sweep, immediate float neighbors at each positive/negative threshold, null, signed zero, subnormal/extreme values, infinities and NaN. Fifty-six creation/request comparisons covered all eight finished-card/trajectory combinations, retaining material decisions, prompt/options, product fields and actual-call provenance. All five publication lifecycle checks passed serially on a fresh schema-only copy of the isolated migrated fixture (excluding leftover Harvester outbox events), including exact stored SQL-derived fields, new-revision/stale-worker fencing, no-material completion and subprocess crashes before/after commit. The temporary server was stopped afterward. No model input or deployment changed. The following Oracle metrics SQL slice satisfies its separate live parity gate.


Oracle metrics SQL slice: local `metrics.sql` is the sole production producer of crown score, convergence and deterministic omen. It consumes the selected finished-card values, preserving raw sign thresholds, available-pair selection, agreement rounding/floor, Momentum-only omen direction, crossroads priority, per-signal clamps and rounded score averaging (or the existing 50 when only Momentum is present). Rust retains strongest-narrative selection with `total_cmp`, including nonfinite/signed-zero behavior, and all model projection/ordering, reading guards, provenance and publication. Empty cards skip SQL and inference and retain the uncalled NULL marker; SQL failure prevents inference. The unchanged legacy formulas now live only in the test module; private articulation consumes SQL metrics in production and oracle values only in offline tests. Validation: `cargo check --all-targets`, `cargo test --all-targets` (440 passed, 68 DB-dependent tests ignored), formatting and diff checks passed. A fresh isolated PostgreSQL 18.6 database was built from the checked-in CI baseline; the migration runner reported zero pending migrations. Explicit live parity passed 14,336 exact metric comparisons across all 32 card combinations, direction variants, profile/mood thresholds, narrative rounding, extreme/nonfinite values and clamps. Another 128 creation/request comparisons retained prompt/options, narrative ordering/three-report limit, material decisions, product fields, components/hashes and actual-call provenance. All six Oracle publication/readiness integration checks passed serially, including SQL-derived stored score/convergence/omen/voiced score, previous score, empty marker, stale/reclaimed claims, debounce and failed-pillar partial-read policy. The temporary PostgreSQL server was stopped afterward. The pushed CI fixes remain intact; no model input or deployment changed. All four planned metric migrations are complete. The following Editor retirement slice removes its obsolete evals as explicitly requested, retaining historical database data; old namespace cleanup follows.


Editor retirement slice: removed all 12 files under `plugins/editor/` (4,220 lines), its module export, executable `EditorTask`, task/menu/lens registration and obsolete extraction/resolver evaluation code. The user’s follow-up authorized removing old evals as noise: deleted the 12 Editor quality fixtures, their preservation-only test, 17 exclusive expectation fields, the resolver fixture type and CLI assertion-count wiring. Active plugin fixtures and evaluations are unchanged. Shared fleet/grant/queue/router checks now use Harvester or Oracle while retaining deny-before-reach, ranked ingestion, backend sharing and stable route coverage; explicit checks reject Editor from production configuration and eval selection. Production source workflows, current model instructions, publication fences and historical SQL tables/data are preserved. Validation: `cargo check --all-targets`, `cargo test --all-targets` (387 passed, 62 DB-dependent tests ignored), formatting and diff checks passed. The lower test count is the removal of 52 Editor-only ordinary tests, six Editor database tests and the obsolete fixture-preservation test. Explicit isolated PostgreSQL 18.6 integration checks passed for Harvester claimed routing/publication, Graph’s rejection of historical Editor material and Investigator atomic candidate publication/stale-claim rejection. No Rust caller/export or Editor rubric reference remains; no SQL/schema or deployment changed. The temporary server was stopped afterward.

Shared form/cognition cleanup slice: combined `Prose`/`Dimensions`, schema rendering, decoding and structural validation in `plugins/form.rs`; moved the typed decision protocol beside Harvester classification in `plugins/harvester/decision.rs`. Deleted the top-level cognition files/directory and the former support form file/export. All plugin, provider, evaluation and session callers use the direct owners; no compatibility wrapper remains. Exact before/after source comparisons pass after only path substitutions and rustfmt for 23 callers, the merged form/types/tests and the decision protocol. Existing schemas, model instructions, validation policies and request/response bodies are unchanged. Validation: `cargo check --all-targets`, `cargo test --all-targets` (387 passed, 62 DB-dependent tests ignored), formatting and diff checks passed. No SQL, model input or deployment changed.

Support retirement slice: shared form types/decoder, served-prose guards and intact publisher reporting now live directly in `tools/form.rs`, `tools/guards.rs` and `tools/source.rs`. Correction policies moved beside the bounded rewrite loop in `studio/session.rs`; model-host slot constants moved beside fleet registration in `application/fleet.rs`, pending the harness move. Graph’s sole-use identity framing is inline in its local prompt, with 12 before/after captures matching byte-for-byte including unchanged system instructions and version. Deleted the support directory/export and the unused `has_ascii_digit` helper and its exclusive test. Exact source comparisons pass after path substitutions and rustfmt for 28 callers, all three tools, the correction policies/rewrite loop and fleet constants. Source admission, typography, schema/parsing rules, slot budgets and bounded correction behavior are unchanged. Validation: `cargo check --all-targets`, `cargo test --all-targets` (386 passed, 62 DB-dependent tests ignored), formatting and diff checks passed. The one-test reduction is the unused digit-scan helper’s test. No SQL, model input or deployment changed.


Evidence and harness consolidation slice: removed `evidence/` and migrated every source/evaluation/example caller to direct owners. Shared fetching is `tools/fetch.rs`; canonical identity and the unchanged display-name lookup are `tools/meta.rs`; shared reporting/statistic studies are `tools/memories.rs`, with the statistic adapter folded into it and SQL/integration checks retained alongside it. Scout owns structured personnel/availability and attributed-source reads in `sources.rs`, with contested-report types/marking in `reports.rs`; the redundant reporting forwarder is deleted. Graph owns quote windows and deterministic result parsing. The shared default trajectory stays in `tools/source.rs`. Acquisition caching, domain pacing, Retry-After/circuits, call budgets, provenance and exact-source validation are preserved.

Consolidated `studio/`, `application/` and `runtime/` directly into `harness/`, keeping substantial queue/provider subfolders. `registration.rs` binds the fleet; `tools.rs` combines grant vocabulary/refusals with the scoped broker. Existing `Studio`, `StudioPlugin`, manifests, inference interfaces, routing, slot limits, correction policies, claim fences, publication/outbox/recovery and diagnostics remain. Old module exports are removed without compatibility facades. Worker/operator/evaluation/example callers and subprocess crash-rehearsal filters use the new paths. README and live source links reflect the new layout; historical validation paragraphs retain their checkpoint paths.

Validation: `cargo check --all-targets`, `cargo test --all-targets` (386 passed, 62 DB-dependent tests ignored), formatting and diff checks passed. Exact source comparisons against `b39a37d2` pass for 136 files plus the creation-session body after owner/path substitutions, the explicit reporting-wrapper deletion, module consolidation and rustfmt; the moved reporting SQL also matches byte-for-byte. Existing request/fixture, source-integrity, parser, broker-budget, scheduling and correction checks pass with the same test counts. No SQL semantics, model input, publication policy, dependency or deployment changed; DB-dependent checks were not rerun for these ownership-only moves. Evidence dissolution and harness consolidation are complete locally. Reporting candidate selection and new Scout reporting/linked-entity input remain separately gated by an inspectable before/after request.
