# Harvester and character plugin boundary

> **2026-10-06 governing update:** This dated record preserves its implementation and evaluation evidence. The [current contract](../../../scoracle-wiki/wiki/Architecture/Harness,%20Plugins,%20Tools,%20and%20LLM%20Contract.md) supersedes articulation-only ownership: cheap code owns collection; expensive compute owns discovery. Plugins assemble traceable clues through tools; the LLM reasons across them, discovers what they support and articulates the answer. Historical “plugin owns WHAT/model owns HOW” statements below do not govern new builds.

The September 27 target contract is: plugins define semantic eligibility and routing intent; the harness validates and dispatches that work; each receiving plugin selects permissible evidence, claims, and output form. Laya supplies bounded probability signals and SmolLM3 supplies articulation. The iOS `ArticulatorAnswerSkill` and `ArticulatorAnswerPlan` are the reference for selected material and explicit unknowns. Its strict selected-answer recipe is still opt-in, so this is a boundary to implement and measure, not a claim that every backend character already enforces semantic fidelity.

## Intake path

1. Google results retain query provenance and publisher identity. Shared `plugins/meta.rs` provides canonical subject name, type, sport and ID. Each predicate receives a natural-language subject reference. Laya scores explicit reference to that subject in the headline; Harvester applies admission policy. It does not ask the model to supply missing affiliations or indirect consequences.
2. Harvester fetches or reuses publisher text only for admitted headlines. Seven native boolean predicates score source evidence: developments, emotion, player moves, contracts, staffing, performance and availability. The exact first three paragraphs are covered by consecutive windows of at most 100 words / 1,200 bytes. More than eight windows is a visible coverage error. Metadata identifies the subject in the question; the evidence state contains only publisher text.
3. The plugin aggregates each predicate's maximum support across windows and applies its route table. Insider accepts player-move, contract or staffing support; Scout accepts performance or availability support. Journalist and Influencer each use their own predicate. The harness dispatches the resulting registered destinations. Receiving plugins own claim selection, output form and fidelity checks.

The local contracts are `harvest-context-v7`, `harvest-headline-v3`,
`harvest-headline-relevance-v3`, and `harvest-theme-routing-v6`. The
[v7 frame report](harvester-frame-2026-09-27.md) records implementation and model
evidence. Reading uses 0.25 (`explicit-headline-read-p025-v2`). Routing uses 0.50,
except performance at 0.70 (`source-window-predicates-v2`). These are provisional
policy values. The maximum score is a support signal, not a calibrated probability
that at least one window is positive.

Receipts bind the entire retained body, exact selected context, individual scoring
windows, canonical subject, attribution/date, checkpoint, scalar scores and policy.
Rejected headlines have no theme calls or source excerpt. The legacy SQL `choice`
column is a deterministic scalar projection, not a Laya-authored choice; the
`distributions` JSON column holds named scalar scores. There is one active scorer,
not a choice/boolean fallback. Deploy the full-question coverage adapter
`harvest-laya-v2` with this Rust version and the matching Go intake work key.

An already stored flat body or opening shorter than thirty words must be fetched
again to recover article prose. Publisher pages without usable paragraph markup
remain a visible acquisition limitation. Live intake supports team queries; the
shared metadata type does not itself implement player acquisition or relationships.

## Release gates

The user accepts the current v7 behavior based on 90/96 matched synthetic route
expectations. [Further calibration](harvester-calibration-follow-up-2026-09-27.md)
is deferred until experience warrants it and does not block the next plugin window.
Deployment remains a separate operational step; this decision changed no delivery
settings. A bounded release rehearsal should verify acquisition, exact source bytes
and hashes, negative short-circuiting and assignment integrity. Retain examples of
missed useful material and unwanted routes for a future calibration pass. Keep
Editor and packet consumers active only where their replacement has not yet been
verified, then retire their paths in coordinated steps.

The three-source v4 shadow replay matched all three retained body hashes, headlines, context byte slices, and Laya input byte slices. Each Laya input started at the first paragraph and ended within the character excerpt; shadow delivery created zero assignments. One entity relevance choice changed from v3. A 226-character opening still produced four positive theme routes. A later 20-domain v4 shadow sample acquired and classified 18 articles, held two fetch failures, and produced 54 query-entity classifications: 50 relevant, four irrelevant, and 17 all-four theme recommendations. These are contract and routing observations, not quality approval. The nightly report counts v7 classifications/work and matching v3 headline gates for current-contract readiness, and separately shows the latest version mix for historical audit.
