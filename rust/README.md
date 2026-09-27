# Studio: the harness contract

**The harness provides the studio. The plugin provides the paint, brushes, and easel. The model provides the expression.**

Studio supplies inference, validation, and publication boundaries. A plugin selects the facts and claims that can appear in its product. The model chooses expression within that plugin-owned vocabulary. A model response is never authoritative evidence by itself.

For the migrated Scout path, the plugin prepares measured statements and approved phrasings. The model returns one phrasing choice per fact. Studio assembles those statements and rejects invalid choices, so a model cannot add a statistic or omit a selected limitation in served prose. Other character products still need their own output plans before this guarantee applies to them.

This is the governing contract for harness work. Propose changes to it explicitly; do not expand the architecture by inference. Implementation gaps and historical plans do not redefine these principles.

## Ownership

- **Postgres remembers:** facts, entity metadata, relationships, source history, products and durable work.
- **DuckDB studies:** bounded data populations, statistical comparisons, cohorts and trends. Return computed observations with their meaning and limitations.
- **Plugin adapters prepare and publish:** select relevant evidence, assemble assignments, route calls and persist validated results. Application assembly binds their concrete dependencies.
- **Studio runs the session:** provide inference, enforce the plugin's finite output plan, and retain provenance. Storage, analytical computation and queue coordination stay outside the core.

Rich upstream context enables precise selection. Prompt size is not a measure of context quality.

## The model call

**Shared form + character voice + relevant identity + selected evidence.**

[`form.rs`](src/plugins/support/form.rs) owns the shared publishing format. Each character has one active brief. Metadata supplies who the entity is, its role and the relevant time/season. Evidence supplies what the character can responsibly interpret.

| Character | Evidence |
|---|---|
| Scout / Rating | Prepared statistics, compatible comparisons, trends and relevant history. |
| Influencer / Vibe | Attributed stories, emotional evidence and relevant memories. |
| Journalist | Sourced developments and story continuity. |
| Insider | Transfer evidence, relationship status and relevant history. |
| Analyst / Momentum | Scout and Influencer outputs and their relevant memories. |
| Oracle | The other five finished outputs. |

Analyst and Oracle synthesize their supplied readings. Identity, dates and missing-output status travel with them in a small envelope. Extraction tasks use their own structured schemas rather than the publishing form.

Each publishing character tells the part of the story its evidence supports. A partial profile can be a complete reading. Measured zero and observed absence can be findings; missing measurements or reports remain unknown. Direction requires a supported comparison. Gaps limit the claims rather than obliging the model to fill a complete profile, explain a cause, or invent a trend. Scout's current palette path publishes a no-stats marker when no measured claim can be selected. Its archived open-prose parser still accepts JSON `null` for historical evaluation; production palette compositions must choose every selected fact. Other voices retain their existing output contracts pending migration.

## Evidence and efficiency

- Supply units, season/competition, comparison population, sample coverage and uncertainty when they affect interpretation. Compute arithmetic and trends upstream. Withhold unsupported comparisons. Missing stays unknown; prior prose is not a new fact.
- Retain full provenance and debugging detail outside the prompt. Send the evidence needed for this reading, once.
- Budget the complete request and reserved output before calling the model. Keep corrections bounded. Validate format and factual boundaries while leaving expression to the character.
- Use the same preparation path for production and evaluation. Frozen prompts are replay artifacts, never production defaults. Update or remove tests that enforce retired behavior.
- Every added input, rule, abstraction or model call must demonstrate a benefit on representative frozen cases. Prefer removing duplication. Measure groundedness, voice, useful specificity, tokens, latency and retries; passing parsers alone is insufficient.

For implementation and operations, use [development guidance](../run_docs/DEVELOPMENT.md), the [runbook](../run_docs/RUNBOOK.md) and [analytical acceptance](../run_docs/RECOVERY_ANALYTICS_ACCEPTANCE.md). The [September 20 findings](../run_docs/quality-2026-09-20/findings.md) record current gaps separately from this contract.

The [plugin architecture execution plan](docs/plugin-architecture-plan.md) tracks the transition to a durable, domain-independent host with plugin-owned capabilities. It explicitly proposes the ownership changes, preserves publication invariants, and records completed milestones separately from the target architecture.

## Source map

`src/studio/` holds the inference session, generation envelope, and plugin/tool contracts. `src/plugins/<name>/` owns each plugin's manifest, prepared cognition, and preparation/publication adapter. `src/plugins/support/` supplies shared form, guards, and resource profiles. The deterministic Boxscore plugin has an adapter and manifest without an inference module.

`src/application/` assembles the fleet and supplies shared capability brokers, with generic durable work transport under `application/queue/`. Plugin manifests and reactions own domain scheduling policy and fan-out. `src/evidence/` contains shared concrete loaders used by typed plugin preparation. `src/runtime/` holds configuration, database connections, routing and providers. `src/evaluation/` and the `eval` binary invoke the same plugin code for offline checks, inspection and replay. `statcommentary` and `factsweep` are thin operator entry points into explicit plugin-owned non-queue invocation contexts.

Keep only active [contract data and quality cases](fixtures/README.md) in `fixtures/`. Historical prompts, generators and captured experiments live in the wiki archive, outside the build.

## Adding a plugin

Add one package under `src/plugins/<name>/` with its manifest and adapter (plus cognition when it
uses inference). Add the manifest to `application/fleet.rs`, and bind the adapter's concrete
dependencies in `application/plugins.rs`; the registry then validates task ownership, route/grant
consistency, and resource declarations at boot. Add a database migration only when the plugin
introduces genuinely new persisted domain data—not merely to register code, routes, providers, or
scheduling policy.
