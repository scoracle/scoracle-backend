# Harness and plugin cleanup

Planning date: October 5, 2026. [rust/README.md](../README.md) owns the current architecture contract. This plan records the Ponytail audit and agreed migration decisions; it does not claim the migration or deployment is complete.

**SQL owns the data. Rust owns the cognition.** The harness runs a plugin. The plugin calls tools, selects and assembles the world, invokes its model where needed, validates the response and publishes to Postgres. Models articulate or classify supplied input and never call tools.

## Status

| Step | State |
| --- | --- |
| 1. Direction and local voices | Done. |
| 2. Scout rendering and legacy memory package | Done; preparation ownership and directory cleanup remain. Render is `prompt.rs`. Measurements are `performance.rs`. Identity reads are `meta.rs`. Publication fences unchanged. |
| 3. Scout data | Trajectory query is the producer, parity-checked on Archbox. Notability stays beside selection. Reporting `strpos` prepared on Archbox. Event-based candidate selection not replaced. |
| 4. Influencer | Preparation and rendering are `prompt.rs`; execution is `mod.rs`; SQL/receipt helpers are `publish.rs`. `cognition/` and `adapter/` deleted with callers migrated. `memories.rs` retains real selection policy. Tool pilot and exclusive provider chat/XML path retired. |
| 5. Other plugins | Render done for Journalist, Insider, Analyst, and Oracle. `assembly.rs` deleted. SQL metric moves not started. |
| 6. Editor and old namespaces | Not started. |
| 7. Harness folder move | Not started. |

Deleted after a caller check: `studio/palette.rs`, `evidence/story_parts.rs`, `dedupe_i64`, `classify_delta`, `trajectory_label`, and the unused `RunDeadline` / `handler_budget` wiring. `DEFAULT_TRAJECTORY` stays. Worker `handler_timeout` stays; that one is enforced.

Do not replace reporting candidate selection, or add verified reporting or linked entities, before a before/after request.

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

Current code: [Harvester execution](../src/plugins/harvester/adapter.rs), [classification requests](../src/plugins/harvester/cognition.rs), [worker/replay context](../src/plugins/harvester/context.rs), [Oracle execution](../src/plugins/oracle/adapter/mod.rs), [Oracle input](../src/plugins/oracle/cognition/parts.rs).

## Context compilation: current and proposed

Production characters already prepare input in Rust and invoke inference without tools. The problem is locating that preparation, duplicated preparation paths and obsolete guidance promoting model-directed retrieval.

| Current Scout | Proposed Scout |
| --- | --- |
| Preparation is still spread across `adapter/mod.rs`, `adapter/harvester.rs` and `cognition/mod.rs`. Rendering is `prompt.rs`; measurements are `performance.rs`. | `prompt.rs` calls shared or local functions with Scout's performance scope. Do not fold adapter SQL into it. |
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
| `plugins/cognition.rs` and `plugins/cognition/` | Delete this namespace. Its only modules are form vocabulary and classification protocol, not an orchestration layer. Fold `Prose`/`Dimensions` into shared `form.rs`; keep `DecisionModel` and its request/response types with Harvester's classification tool. |
| `plugins/support/` | Dissolve the umbrella. Form/source/guard mechanisms become shared tools where compatible; resource constants and bounded execution mechanics belong to the harness. Delete unused helpers rather than moving them. |
| `plugins/memories.rs` and `plugins/memories/` | Keep one shared memory tool. Fold the small statistics adapter into it; retain the reporting SQL and integration checks alongside shared tools or tests. This subfolder is not required architecture. |
| Scout/Journalist/Influencer `memories.rs` | Put small scope/selection policies in `prompt.rs`; retain substantial specialized study or continuity logic as a local tool. Delete redundant wrappers; Influencer's current file carries selection policy and stays. |
| `evidence/memories.rs` and `evidence/memories/` | Retired. Identity reads are `plugins/meta.rs::load_identity_record`. |
| `studio/`, `application/`, `runtime/` | Consolidate real registration, scheduling, queue, publication, model/provider, configuration and DB infrastructure into `harness/` as callers are simplified. Keep real queue/provider grouping; delete unused capabilities/deadline scaffolding. |
| `evidence/` | Move surviving readers/fetch mechanisms into shared or specialized tools, then remove the namespace. Preserve actual acquisition caching, pacing, retry, budget and provenance behavior. |
| `plugins/editor/` | Delete in its isolated retirement slice after obsolete executable evaluations move; keep historical fixtures and stored data. |

Deleted after a caller check: `studio/palette.rs`, unused `RunDeadline` / `handler_budget` wiring, `evidence/story_parts.rs`, `dedupe_i64`, and the trajectory helpers that only the story helper used. `DEFAULT_TRAJECTORY` remains. Check references immediately before the next deletion.

Folder cleanup is part of each plugin slice, not a cosmetic rename at the end. Each slice must remove superseded imports, exports, forwarding modules, prompts and exclusive helpers. Track any live caller that prevents deletion and finish its migration before declaring the slice complete.

## Shared context tools

- **Meta:** add a reusable canonical loader to the existing type where it removes repeated reads. A plugin that already has the metadata can construct the type directly. Reuse SQL current-identity projections for team/league attributes.
- **Memories:** keep the existing bounded Postgres snapshot and Go DuckDB runner. The reporting title predicate calls `strpos` with two arguments and prepared on Archbox. Candidate selection is still event-based. Replace that before relying on new Harvester history, and show the before/after request first. Keep study selection with the plugin.
- **Fresh:** reuse `harvester::delivery::load_for_character` and `support::source` behind one shared reader where callers fit. Preserve plugin-specific assignment eligibility, source integrity and publication revalidation. Oracle's finished cards and Harvester's acquisition are different local tools.
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
| Separate later pass | Retired Editor and obsolete executable evaluation callers. | Not in the active fleet; deletion must preserve historical fixtures and data. |

The audit's provisional eventual reduction is about 6,000 lines with no new dependencies, including the separate Editor pass. This is an estimate, not a target that justifies removing safeguards.

## Migration sequence and acceptance

1. **Direction and local voice:** update the active README, add a short supersession banner to the completed alignment plan and preserve dated evidence. Keep each active voice local with identical text; remove the shared catalog. Record the folder/deletion map above.
2. **Scout preparation and pruning:** rendering and legacy-memory pruning are done; the preparation entry point still needs to move from `adapter/mod.rs` to `prompt.rs`, and the substantial parsing/measurement helpers still live in `cognition/`. Native serialization, parts-file deletion, and the legacy memory package are gone. Rendered requests and both publication paths were preserved. The debounce hash no longer includes the unused package fingerprint.
3. **Scout data and context:** trajectory arithmetic now runs in the load query, with Archbox parity. Notability stays beside selection. Replace event-based reporting selection only after a before/after request. Preserve no-stats outcomes and measurement guards. Do not add verified reporting or linked entities before that request.
4. **Influencer:** done. Read-only preparation and native rendering live in `prompt.rs`, execution in `mod.rs`, publication SQL/receipts in `publish.rs`. The old preparation directories, source forwarder, task alias and execution forwarder are gone. Worker, evaluation, replay and downstream completion callers use the new paths. `memories.rs` retains its selection policy.
5. **Other plugins:** the four remaining worlds now render with native serialization, and `assembly.rs` is deleted. Still to do: move Journalist activity, Insider grounded transfer heat, Analyst direction/conviction and Oracle score/convergence/deterministic omen into SQL with parity. Keep Harvester, Investigator, Boxscore and Graph's distinct jobs and model protocols.
6. **Legacy retirement:** remove Editor and its obsolete executable evaluations in an isolated pass, retaining fixtures and stored historical data. Finish dissolving the old support, cognition and evidence namespaces as their last callers move.
7. **Harness consolidation:** move the surviving Studio/application/runtime infrastructure into `harness/`, retaining its useful queue/provider subfolders. Production business logic belongs to plugins and tools. Remove old exports and paths together; keep operator/evaluation entry points working.

Metric migrations preserve formulas, selected inputs, null behavior, thresholds, ordering and rounding. Scores/categories must match exactly; raw regression slope allows a 1e-9 tolerance. Keep one active producer after cutover. Semantic extraction, quote/source validation, model instructions and acceptance stay in Rust. Keep current Go acquisition and analytical helpers; preserve Analyst/Oracle's existing product dependencies.

Before changing a model's input, retain an inspectable before/after request, its selected data and provenance. Reuse production preparation for replay. Run the smallest relevant checks for each slice; SQL changes require a real integration/parity check, because existing ignored DB tests do not verify them. Compilation and local tests do not establish deployment or model-output quality.

## Local checkpoint — October 5, 2026

Reviewed the current working tree against this plan. The Scout rendering/measurement split, legacy-memory removal, identity-reader migration, SQL trajectory cutover, reporting-predicate repair and Influencer pilot retirement are the checkpoint scope. Archbox parity above is evidence recorded by the prior work; this checkpoint does not repeat that remote run or establish deployment. Checkpoint committed as `df05d894`. The subsequent native-renderer/dead-code slice passed `cargo test --all-targets` (419 library tests and 14 eval tests; 63 DB-dependent tests ignored). Formatting was checked before its commit.

The native-renderer/dead-code checkpoint is `d7a59629`. The Influencer ownership slice follows it, preserving request bytes, source admission, memory policy and publication fences. Next: finish Scout preparation ownership and directory cleanup before further plugin metric or harness moves. Scout reporting and linked-entity input changes remain gated by an inspectable before/after request.

Influencer slice validation: `cargo test --all-targets` passed (419 library, 14 eval, 1 factsweep and 2 statcommentary tests; DB integration tests remained ignored). The 12 audit replay requests exported before and after the ownership move compare byte-for-byte, including provider options and schema. Preparation and claim-fenced publication bodies also match the checkpoint after path changes. A local no-DB admission check verifies rejected sources retain their receipt before memory access. No SQL or model input changed, and no deployment was performed.
