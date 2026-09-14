# Card canvas and identity repair — local, not deployed

Follow-up: [editorial investigation](2026-09-13-editorial-quality-investigation.md)
found that unranked fallback scores are mislabeled as percentiles. The measurement
identity change below does not yet fix that separate issue; hold release pending repair.

## Observed failures

- Morgan Rogers, FOOTBALL player 4592198: stat summary 33148 had the hook
  “Morgan Rogers is a Midfield”. Its ledger recorded 700 generated tokens against
  a 700-token reservation. Provider termination reasons were discarded.
- Chelsea/person 11: transfer verification called Liverpool coach Andoni Iraola
  a Chelsea target even though the person record correctly identified his role
  and club. The source articles discussed other recruits and quoted the coach.
  The old prompt asserted that an outsider should be framed as an incoming target.
- Acquisition could overwrite a person's current club with the first career club.
- Rogers's Shooting comparison crossed measurements: 2025 shots on target (32)
  versus 2026 expected goals (0.14). Shared labels were treated as comparable units.

## Local changes

`form.rs` owns a 140-character hook and 1,200-character body, including spaces.
These are ceilings, not targets. Paragraphs follow the story rather than a
one-paragraph-per-claim outline. Character prompts retain voice and perspective.

All writers request JSON. Provider completion checks reject exhausted answers;
surface violations get one bounded rewrite, not prose clipping. Retired hook
salvage and sentence-removal routines were removed.

Compact identity records reach the six writers, Graph candidates, Editor
hypotheses and transfer verification. Current identity changes participate in
writer/transfer input hashes; confirmation timestamps do not. Acquisition seeds
identity; the sourced reporting sweep owns subsequent person role/club updates.
Role and affiliation revisions are independent and transactional, with quoted
provenance and superseded history. See [the convention](../docs/cognition-output.md).

Migration 252 preserves measurement identity beside the existing SQL formulas,
with backward-compatible six-column rating adapters. The Scout cannot derive a
season change from different measurements or league cohorts. Prompts show the
underlying measurement where the skill label is ambiguous.

## Verification and limits

- 444 Rust tests pass; all-target clippy with warnings denied passes.
- SQL numeric parity sampled each available sport/season and player rate mode
  against the existing production functions. Migration smoke checks and loader
  projections passed using session-only temporary functions and rollback.
- The compact transfer prompt rejected the exact Chelsea/Iraola false positive
  in a non-persisting resident-model replay.
- Scout replays included granite4.2:3b and the installed ministral-3:8b. Surface
  retries can produce a fitting card, but unsupported availability and trend
  claims still occurred. Parser success is not a factual-quality pass. The final
  paragraph-form wording has unit coverage, not a new live-model quality result.

Apply migration 252 before deploying the Rust binaries. No production schema,
canonical metadata, stored readings or model routing were changed in this session.
The schema snapshot remains the live snapshot; refresh it after migration release.
The pre-existing Go/articulator edits were left untouched.
