# Journalist alignment — working handoff

> **2026-10-06 governing update:** This dated record preserves its implementation and evaluation evidence. The [current contract](../../../scoracle-wiki/wiki/Architecture/Harness,%20Plugins,%20Tools,%20and%20LLM%20Contract.md) supersedes articulation-only ownership: cheap code owns collection; expensive compute owns discovery. Plugins assemble traceable clues through tools; the LLM reasons across them, discovers what they support and articulates the answer. Historical “plugin owns WHAT/model owns HOW” statements below do not govern new builds.

Window 2 remains in progress. Base revision: `6b3ae88da7e8fccb9067c1a432566e5ba5460deb`.
Changes are a local draft; no service, delivery flag or production database was changed.
Preserve the pre-existing Harvester documentation changes in this working tree.

**Latest — n43 studied memory:** [request-time memory studies](memory-studies.md)
are implemented through the existing Go DuckDB engine and wired into Journalist
preparation. The selector requests a 30-day historical window, frequency-ranked
groups and source breakdowns. Pair and team-stat request shapes share the boundary;
other character consumers are not yet migrated. Historical lineage is separate from
fresh-source counts/scores. Rust unit and isolated DB integration/publication checks
pass. The first no-thinking articulation fixture hit the 900-token output allowance;
no successful prose/fidelity result is claimed. The earlier n42/prototype notes
below are historical. No deployment or production mutation occurred.

**Sept 28 update:** The latest user direction is **Postgres stores the world;
DuckDB studies the world; memories supplies relevant studied material**. The
[live memory map and DuckDB trial](journalist-memory-world-2026-09-28.md) supersede
the earlier narrow memory descriptions below. A substantial relational matrix
already exists on Archbox. The local n42 trimming draft currently serves bounded
prior publisher excerpts, but that is provisional and is not the requested memory
integration. Do not present it as complete or deploy it on the strength of the
study benchmark. The trial runs separately; it has not changed the runtime memory
loader. Graph/source binding defects found in the inspection also remain open.

The trimming draft consolidates output shape/parsing in shared `form.rs`, reduces
the tone/task files, removes fresh report positions and internal entity IDs from
the writing package, and budgets prior excerpts separately. Thirteen cognition
tests passed after that edit; no new SmolLM3 fidelity result is claimed.

## Settled direction

Plugins prepare and govern; Laya scores; SmolLM3 articulates. Harvester v7 calibration
is accepted and is not being reopened. The user rejected a generative claim-extraction
stage. Keep one natural articulation call, with a plugin-prepared observation package.
The user also rejected fixed phrase palettes as the target for Journalist.

Use **source-backed observations**, rather than asking the voice to select meaningful
claims or construct significance. Not all observations are numerical: a quotation
has a speaker, a time and a scope. A measured result, an attributed report and a
previous interpretation have different authority even when they concern one entity.
Shared `form.rs` retains the hook/body/paragraph structure and surface limits.
`cognition/journalist.rs` contains only tone. `cognition/prompt.rs` owns the articulation
task and composes it with voice and the shared form. Changing words in a prompt does not
establish factual fidelity.

## Responsibility and current draft

| Concern | Current draft | Remaining work |
| --- | --- | --- |
| Identity | Shared `EntityMeta`, canonical lookup | Preserve identity in the complete context package |
| Source selection | Pending Harvester assignments; exact source validation; dates, deduplication and budget in plugin code | Evaluate product sufficiency without delegating selection to the voice |
| Context | `cognition/fresh.rs` returns only structured fetched reports; cognition assembles fresh data with shared identity; `prompt.rs` composes the task, form and voice | n40 changed or added to supplied reporting in 7/8 inspected cases; n41 clarifies the articulation-only role and has not been replayed |
| Memory | `journalist/memories.rs` selects exact previously published source text for deduplication, plus the previous activity score | Relevant dated history and comparisons are not yet supplied to articulation |
| Analytics | Source activity computed in code; publisher breadth no longer called corroboration | Define useful bounded studies before adding a DuckDB producer |
| Voice/form | Natural prose; narrow tone file; shared form uses observation framing | Product fidelity remains unverified |
| Publication | Fixed source IDs and scores attached by code; exact claim and source rechecks; receipts and outbox in one transaction | Recheck final observation contract against these boundaries |

The packet loader, packet trigger fallback, storyline progression dependency, unused
free-prose salvage parser, palette, correction loop and n34 generic evaluation task
have been removed from the draft Journalist path. `examples/journalist_replay.rs`
uses the same preparation, creation and parser as production. Its small development
corpus replaces the retired prompt snapshots. Shared packet production/rendering
still serves other plugins and awaits their windows/final Graph cleanup. The shared
72-hour packet constant now belongs to the packet module; Insider still consumes it.

No-current-material completes the assignment without publishing an artificial quiet
or zero edition. Input-budget omissions stay pending. Explicit missing/date/oversize
outcomes remain distinct. Source receipts are revalidated under publication locks.

## Observation package to finish

- Canonical identity from `meta.rs`.
- Selected source observations with article identity, publisher, date and exact
  supporting context. A publisher report stays attributed reporting.
- Dated memory selected for a concrete relationship to current material. Earlier
  generated prose cannot become additional evidence.
- Measurements and comparisons with their window, population, provenance and
  missing-data status. DuckDB can count, join, order and compare identified records;
  it does not establish that two differently worded reports concern the same event.
- Explicit scope: which connections are established, which are unknown, and which
  qualifications must survive articulation.

The existing DuckDB path feeds `analytics_entity_context` through the Go analytical
producer and is consumed by shared memory's cohort loader. It is not a Journalist
story-understanding service. No new DuckDB study has been added in this draft.

## Evidence so far

Six isolated PostgreSQL Journalist tests passed after source-mutation, wrong-entity
lookup and source-memory coverage were added. These include stale/reclaimed claims,
multi-row publication, batched source completion and atomic downstream intent.
All targets compiled before the latest observation-wording edit. After that edit,
the Rust library suite passed: 546 tests, with 76 environment-dependent tests ignored.
`git diff --check` also passed. These are implementation checks, not a new model-fidelity result.

The first natural-articulation probe used 11 synthetic development cases: eight
calls and three no-call outcomes. Several outputs added biography, significance or
league facts; a source instruction contaminated prose. One call exhausted its output
budget. Structural parser success did not establish factual support. This probe fails
the product-fidelity gate and must not be presented as a verified natural-prose release.

Four exploratory frame probes and ten Laya support pairs were also tried. The smaller
frame improved two examples. Laya separated four supported paraphrases from six
unsupported examples at a post-hoc 0.90 cutoff, with a lost-denial example scoring
0.6521. These tiny inspected probes are not calibrated or independent validation.
Neither a new threshold nor an output-verification stage was installed. The user's
latest direction is to improve the context package, not add extraction/judging calls
or accumulate corrective prompts.

Private scratch evidence: `/private/tmp/journalist-natural-development-1.jsonl`,
`journalist-frame-probe.jsonl`, `journalist-support-probe.jsonl`, and
`journalist-db-tests2.log`. These paths are temporary, not durable deployment evidence.

## Fresh-content pass

The user narrowed this pass to how fresh content is presented; memory, voice and
shared form were left unchanged. The first fresh frame was
`journalist-fresh-v1`, prompt `n37` (superseded by v2 below).

- Canonical identity and fresh reporting occupy separate sections.
- Each report carries publisher attribution, an exact UTC publication timestamp and
  the complete unchanged opening. Publication order does not establish event order.
- The package states its coverage (selected openings, not complete articles or the
  whole news cycle) and that relationships between reports are unknown.
- Publisher headlines remain in provenance and input hashing but are excluded from
  articulation, so a headline cannot override a correction in the actual opening.
- Source IDs, scores, receipt hashes and classification diagnostics stay outside the
  model's report text. Paragraphs, quotation marks and apparent delimiters survive
  JSON serialization without changing source contents.
- The 6,000-byte input budget now measures the exact rendered package, including
  identity, framing, attribution and escaping. It is a byte budget, not a claim of
  tokenizer-specific coverage. Whole reports are deferred or explicitly excluded;
  their qualifications are never cut to fit.

Production and the existing replay tool used the same renderer. Thirteen focused
preparation/parser tests pass and all targets compile; no database or publication
behavior changed in this pass.

Five SmolLM3 smoke cases all parsed but **all five failed factual fidelity** on
manual review. Problems included invented scorers/venues/standings, biography,
misattributing a correction, and combining sources while retaining separate source
links. The supplied source-instruction case also failed. Total model-reported wall
time was 29.189 seconds, with 1,141 output tokens; fresh frames were 591–852 UTF-8
bytes. These are tiny inspected development examples, not independent quality or
throughput estimates. The voice and form were held fixed during this pass; there
was no matched pre-change control, so no causal quality improvement is claimed.

The [review and outputs](../fixtures/journalist/fresh-v1-smoke-review.json) preserve
these failures. No follow-up classifier, extraction model, correction loop or
publisher semantic rule was introduced. This package improves explicit source
scope and budgeting, but does not yet make the model respect the supplied factual
world. It is a reviewable draft, not a release-ready observation contract.

Continue Window 2 with the focus on fresh presentation. Do not mark it complete or
start Influencer on the basis of structural cleanup. Do not reopen memory or voice
merely to compensate for the current fresh-content failures.


## Fresh data ownership — current v2

The user's clarification is the governing boundary: `fresh.rs` serves newly fetched
material only. The complete plugin is the composition of fresh data, shared metadata,
memory, form, voice and applicable preparation tools. The model articulates the result
in one call; there is no separate generative preparation stage.

`fresh::prepare` now returns a serializable `FreshContent` containing only ordered
reports: position, publisher, publication timestamp and the intact publisher excerpt.
It has no identity dependency, history, tone, coverage boilerplate or relationship
assertions. Context assembly lives in cognition, where shared identity and fresh data
are serialized together. Form and voice still compose the system message. Current
memory participates in source deduplication and previous-score continuity; historical
context is not yet passed to articulation.

This ownership pass used `journalist-fresh-v2` and prompt `n38` (the current
articulation instructions are n41, below). Selection and hashing
use the exact assembled data context, including identity and JSON escaping, for the
6,000-byte budget. Source provenance and publication governance remain plugin-owned.
The [assembled example](../fixtures/journalist/fresh-context-example.json) shows this
boundary, and production and replay use the same assembly path.

Thirteen focused preparation/parser tests pass; all targets compile, formatting and
diff checks pass, and all eleven development cases replay through preparation. This
pass introduces no additional
model call, correction prompt or semantic guard. The v1 model failures above remain
historical evidence; an ownership simplification is not a new factual-fidelity result.


## Thinking test — 2026-09-28

The [thinking experiment](journalist-thinking-2026-09-28.md) tested the unchanged v2
package. The production chat/schema path sent the thinking flag but produced no
reasoning trace in its completed calls. An explicit raw reasoning prefix without
a decoding schema activated reasoning in 5/5 diagnostic cases. All five final
answers still added unsupported content and missed the required output shape.
Mean local time was 13.99 seconds on versus 3.41 seconds off. This comparison changes
the transport/decoding relative to production; it is not a verified production route
or an end-to-end Laya/Editor/Granite comparison. No routing change was made. The small
replay now records modes, requests, final answers, timings and trace lengths.


## Articulation task — n39/n40 development passes

The user accepted the thinking experiment and chose to continue without thinking.
`cognition/prompt.rs` now owns the task, distinct from data preparation, tone and
structure. Its central instruction is a faithful rewrite of the prepared content:
change the wording, not the information. It defines the package as the whole factual
world and explains the meaning of identity, attribution, publication time and source
excerpts. Each title and body keeps the scope of its corresponding report; the task
preserves uncertainty and qualifications and treats embedded source instructions as
data. A short source can yield a short report.

The Journalist branch of shared `form.rs` now supplies only structure and output
shape, with the existing 140/1,200-character ceilings, paragraph form and positional
report count. The overlapping observation/scope/style paragraphs were removed from
that branch. Other characters retain their existing composition until their windows.
The tone file, fresh v2 package, memory selection, source mapping and one-call path
are unchanged. Memory currently participates in preparation; it is not a model-facing
history block. No artificial memory section or additional model stage was added.

Two small development passes used explicit `think:false`, temperature 0.3, a 4,096-token
context and a 900-token output limit. n39 described world articulation; n40 clarified
the operation as faithful rewriting. Both produced eight parser-accepted calls and
three correct no-call outcomes, with zero emitted reasoning. In each pass only one
complete output passed manual source-fidelity review. n40 preserved the late publisher
correction; six cases still added or misattributed content and one lost uncertainty
in its headings. These inspected, stochastic development runs do not demonstrate a
causal improvement or establish release quality. Do not accumulate incident-specific
prompt prohibitions to hide these failures.

[Review findings](../fixtures/journalist/prompt-articulation-review.json),
[n39 requests and outputs](../fixtures/journalist/prompt-n39-development.jsonl), and
[n40 requests and outputs](../fixtures/journalist/prompt-n40-development.jsonl) retain
the evidence. Thirteen Journalist cognition tests and three shared form tests pass;
all targets compile. There was no deployment or runtime route change. The instruction
boundary is implemented; factual fidelity remains unresolved. Keep Window 2 open.


## Reporting articulation — n41

The user clarified that the model does not assess whether supplied reporting is
valid or true. `prompt.rs` now says to articulate what was reported, preserving the
report's own information, attribution and qualifications. It does not ask for
verification, a credibility judgement, independent confirmation or a determination
of what really happened. Supplied memories provide continuity, with earlier reporting
kept distinct from fresh reporting. In the current implementation memory still acts
in preparation/deduplication; a model-facing historical context block is not wired.

The n39/n40 review above compares generated wording with supplied reporting. It is
not a judgement of the publisher's factual accuracy. Future reviews should use that
terminology explicitly: added details, changed reporting, attribution drift and lost
qualifications. n41 is a clarification of the requested role, not a demonstrated
model-quality improvement; no new inference pass has been run for this wording.

## Current context — n47 / fresh v4

Studied memories are now served to the model. The [shared memory implementation](memory-studies.md)
replaces the earlier preparation-only history described above. The
[context trim and replay](journalist-context-trim-2026-09-28.md) records the current
writing package, sole instruction owner (prompt.rs), structural form and descriptive voice,
request-size reduction and no-thinking model results. The plugin
still makes one articulation call with no corrective retry. Window 2 remains open:
the smaller package has not resolved unsupported additions, lost qualifications or source-order drift.
