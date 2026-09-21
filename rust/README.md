# Studio: the harness contract

**The harness provides the studio. The model provides the art.**

Studio is an LLM-empowering platform. We provide trustworthy materials, useful tools, character direction and a shared canvas. The model brings judgment, interpretation and expression. As better painters arrive, they can use the same studio for more sophisticated work. Build capabilities that help them create; keep the machinery small and let the quality of the art grow with the model.

Form gives the work a usable surface. Evidence gives it a foundation. Validation protects factual integrity. These supports should leave the model room to discover what matters and express it beautifully. Do not prescribe its conclusions, paragraph plan or wording, or encode one model's limitations as permanent creative constraints.

This is the governing contract for harness work. Propose changes to it explicitly; do not expand the architecture by inference. Implementation gaps and historical plans do not redefine these principles.

## Ownership

- **Postgres remembers:** facts, entity metadata, relationships, source history, products and durable work.
- **DuckDB studies:** bounded data populations, statistical comparisons, cohorts and trends. Return computed observations with their meaning and limitations.
- **Application adapters prepare and publish:** select relevant evidence, assemble assignments, route calls and persist validated results.
- **Studio equips the model:** compose the assignment, provide capabilities, call the model and validate the output. The model creates the reading. Storage, analytical computation and queue coordination stay outside the core.

Rich upstream context enables precise selection. Prompt size is not a measure of context quality.

## The model call

**Shared form + character voice + relevant identity + selected evidence.**

[`form.rs`](src/studio/form.rs) owns the shared publishing format. Each character has one active brief. Metadata supplies who the entity is, its role and the relevant time/season. Evidence supplies what the character can responsibly interpret.

| Character | Evidence |
|---|---|
| Scout / Rating | Prepared statistics, compatible comparisons, trends and relevant history. |
| Influencer / Vibe | Attributed stories, emotional evidence and relevant memories. |
| Journalist | Sourced developments and story continuity. |
| Insider | Transfer evidence, relationship status and relevant history. |
| Analyst / Momentum | Scout and Influencer outputs and their relevant memories. |
| Oracle | The other five finished outputs. |

Analyst and Oracle synthesize their supplied readings. Identity, dates and missing-output status travel with them in a small envelope. Extraction tasks use their own structured schemas rather than the publishing form.

Each publishing character tells the part of the story its evidence supports. A partial profile can be a complete reading. Measured zero and observed absence can be findings; missing measurements or reports remain unknown. Direction requires a supported comparison. Gaps limit the claims rather than obliging the model to fill a complete profile, explain a cause, or invent a trend. Shared form retains claim selection and story structure. Scout may return JSON `null` when the evidence supports no meaningful claim in its scope; this is a completed, called abstention with a null-body publication marker, not a failed generation or a pre-call no-stats result. Other voices retain their existing output contracts until their publication paths support abstention.

## Evidence and efficiency

- Supply units, season/competition, comparison population, sample coverage and uncertainty when they affect interpretation. Compute arithmetic and trends upstream. Withhold unsupported comparisons. Missing stays unknown; prior prose is not a new fact.
- Retain full provenance and debugging detail outside the prompt. Send the evidence needed for this reading, once.
- Budget the complete request and reserved output before calling the model. Keep corrections bounded. Validate format and factual boundaries while leaving expression to the character.
- Use the same preparation path for production and evaluation. Frozen prompts are replay artifacts, never production defaults. Update or remove tests that enforce retired behavior.
- Every added input, rule, abstraction or model call must demonstrate a benefit on representative frozen cases. Prefer removing duplication. Measure groundedness, voice, useful specificity, tokens, latency and retries; passing parsers alone is insufficient.

For implementation and operations, use [development guidance](../run_docs/DEVELOPMENT.md), the [runbook](../run_docs/RUNBOOK.md) and [analytical acceptance](../run_docs/RECOVERY_ANALYTICS_ACCEPTANCE.md). The [September 20 findings](../run_docs/quality-2026-09-20/findings.md) record current gaps separately from this contract.

## Source map

`src/studio/` is the model-facing core. `src/application/` prepares and publishes, with durable work under `application/queue/`. `src/evidence/` retrieves and shapes context, including source fetching. `src/runtime/` holds configuration, database connections, routing and providers. `src/evaluation/` and the `eval` binary provide offline checks, inspection and replay.

Keep only active [contract data and quality cases](fixtures/README.md) in `fixtures/`. Historical prompts, generators and captured experiments live in the wiki archive, outside the build.
