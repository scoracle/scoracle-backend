# Harvester and character plugin boundary

The September 27 target contract is: the harness selects the work and plugin; the plugin selects permissible evidence, claims, and output form; the local model supplies probabilistic judgments and wording within that boundary. The iOS `ArticulatorAnswerSkill` and `ArticulatorAnswerPlan` are the reference for selected material and explicit unknowns. Its strict selected-answer recipe is still opt-in, so this is a boundary to implement and measure, not a claim that every backend character already enforces semantic fidelity.

## Intake path

1. Google results retain query provenance and publisher identity. Harvester fetches publisher text with paragraph breaks.
2. Laya receives the target entity, headline, and bounded publisher opening. Harvester validates its entity relevance choice. An irrelevant result stops after this pass; no theme call or character assignment is made.
3. For a relevant result, Harvester asks four separate questions about narrative developments, emotional charge, transfers or organizational change, and performance or availability including injuries and suspensions. Each choice maps to a named character plugin. The labels are routing evidence, not facts for a published card.
4. Harvester stores the exact headline and first three available publisher paragraphs, byte offsets, body hash, model input, distributions, versions, and provenance. Character plugins receive only selected source contexts through their assignment ledger. Each character owns its claim selection, output form, and fidelity checks.

`harvest-context-v4` and `harvest-theme-routing-v5` distinguish this path from the three-sentence, broad-routing v1 classifications and the v2/v3 shadow paragraph probes. Both Laya passes see only a bounded prefix of the same first three paragraphs supplied to characters. An already stored flat body or an opening shorter than thirty words must be fetched again to recover article prose. Publisher pages without usable paragraph markup remain a visible acquisition limitation.

## Release gates

Production delivery is held in shadow mode while the new route is evaluated. Before reopening it, check on a bounded fresh-source cohort that paragraph boundaries survive acquisition, byte ranges and hashes match, irrelevant results produce no theme calls or assignments, theme assignments are selective, and every delivered character product stays within its plugin-selected evidence. Measure missed useful items and per-character routing volume against reviewed sources; the old checkpoint's probabilities are uncalibrated and its broad routing is not a quality benchmark. Keep Editor and packet consumers active only where their replacement has been verified, then retire their worker, API, UI, trigger, and data paths in coordinated steps.
