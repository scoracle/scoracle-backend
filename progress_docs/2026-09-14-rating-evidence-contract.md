# Rating evidence contract: implementation

## Findings

This continues [the editorial investigation](2026-09-13-editorial-quality-investigation.md).
The durable fix starts at the data boundary, not with another prose guard.

- `_compute_rating_bundle` stored two different quantities as `pct`: actual
  percent-rank for eligible players and a clamped `50 + 10 * sign * z` score for
  ineligible players. Consumers treated both as percentiles and assigned tiers.
- Morgan Rogers' 2026 row had **one appearance**, no composite rating, and a
  fallback Chance Creation score of 95.4. That was not a 95th-percentile rank.
  His 2025 row had 37 appearances. Neither sample nor the stat observation date
  reached the Scout.
- Skill labels do not identify measurements: Shooting can mean shots on target
  or expected goals; Chance Creation can mean key passes or expected assists.
  Population calculations grouped by label alone. Historical measurement
  identity cannot safely be reconstructed using today's formula.
- Null numeric evidence became zero in Rust. A missing rank is not a bottom
  rank; a missing statistic is not a measured zero.
- Rate-mode formulas could fall back to raw totals when a rate input or its
  denominator was absent. Missing Shooting/Chance Creation sources could also
  emit zero without any known measurement behind that zero.
- Prior generated commentary re-entered the writer as memory, enabling an
  unsupported statement to become apparent continuity. Stored measurements and
  sourced personnel records provide the useful continuity without that loop.
- The existing keyword-oriented rating evaluation omitted live enrichment.
  Passing it does not establish editorial accuracy.

The earlier findings remain relevant: provider truncation was hidden, output
salvage damaged hooks, and transfer framing presumed that outsiders were targets.
Those local fixes, the 1,200-character body/140-character hook canvas, and the
dated identity/fact-update conventions are recorded in the linked investigation
and [output contract](../docs/cognition-output.md).

## Implementation contract

1. PostgreSQL owns evidence meaning: persist measurement identity, keep true
   percentile ranks separate from standardized scores, and compare like measures.
2. Unknown evidence stays unknown. Recompute existing derived rows from source
   statistics atomically; do not guess historical measurement identity.
3. Scout receives a compact, dated sample and measurement profile. No automatic
   reuse of its own generated prose and no extra runtime editorial judge.
4. Evaluation builds the same evidence context as production. Regression tests
   prove the evidence contract; editorial quality still requires reading actual
   cards across representative cases and runtimes.

## Implemented locally

- Migration 252 defines measurement-aware producers and keeps the six-column
  compatibility adapters. Missing source/rate inputs no longer become raw-rate
  fallbacks or invented zeros. Explicit zero-suppressed feed conventions remain.
- Migration 253 removes the fallback percentile, persists measurement identity
  and player eligibility, groups populations by the actual measurement, and
  leaves singleton/constant skill populations unranked. Thresholds are computed
  once per cohort. Existing player/team season and rate bundles are rebuilt and
  checked before the migration records itself and commits.
- Scout numeric fields are optional. It receives dated samples, clear value
  units, named measurements and only comparable prior ranks. Relative rank
  changes are not mechanically called changes in ability. The compact character
  brief owns interpretation; generated Scout prose is no longer loaded as memory.
- Values, sample sizes and sourced enrichment participate in input identity.
  Live evaluation uses the same enrichment path as production. The card surface
  remains 1,200 body characters plus a separate 140-character hook.

## Verification

- 448 Rust tests passed across all targets; strict Clippy and formatting passed.
- Go articulator tests passed; its existing nullable percentile handling required
  no changes to the user's Go work.
- SQL regression fixtures cover unranked samples, measured zero, absent
  populations, changed measurements, team cohorts and missing rate denominators
  and inputs.
- Both migrations' functions were replayed in a temporary schema against copies
  of **44,300 player rows and 1,224 team rows**, rebuilding all **25 player and
  25 team sport/season cohorts**. Contract checks passed and the transaction was
  rolled back. No production tables, functions or metadata were changed.
- The rebuilt Morgan 2026 row retains 1.01 expected assists, one assist, 0.14
  expected goals and zero goals, with no skill percentiles. The sample query
  supplies one appearance/90 minutes, updated September 7. His 2025 sample is
  37 appearances/3,285 minutes, updated June 13.
- Rebuilding corrected populations intentionally changes derived ratings; Morgan's
  2025 composite score in the replay is 70.1, previously 72.4. This is not a
  byte-preserving migration. Source statistics and generated prose are untouched.
  [The retained evidence](../run_docs/experiments/2026-09-14-rating-evidence.json)
  records both rebuilt Morgan rows and the validation scope.

## Release status

Data-contract implementation and verification are complete locally. No production
migration, model route change, regeneration, or deployment has been performed.

Two controlled s33 rechecks on the existing `granite4.2:3b` configuration
(`think=false`, 4,096 context tokens, 700 output tokens, temperature 0.6, seeds
17/43) still failed editorial review. The compiled writer used the rebuilt
statistics and recorded identity; optional personnel, report and recent-form
blocks were omitted. This is a controlled evidence test, not a full live replay.

- Seed 17 returned 1,186 body characters—inside the surface—but invented a
  strong defensive rank and consistent/limited abilities from one appearance.
- Seed 43 expanded one combined CBI total into three individual counts, invented
  physical/positional characteristics, and exceeded the body surface at 1,277
  characters. Neither hook stated a meaningful main finding.
- Both requests used 530 prompt tokens. Smaller, better-typed context did not
  by itself make this runtime reliable.

The source labels now explicitly identify the CBI sum, weighted card formula,
goals, accurate passes and successful dribbles. The two outputs precede those final
label clarifications; they are not a claim about their effect. No additional
prompt prohibitions, prose-repair rules or runtime judge were added.

[Requests, responses and review notes](../run_docs/experiments/2026-09-14-editorial-recheck.json)
are retained. An initial probe accidentally used the offline client's omitted
thinking preference; its two budget-exhausted responses are excluded from the
production-configuration comparison.

**Release remains gated on editorial runtime qualification.** The data repair is
necessary and verified, but an in-bounds, completed answer can still be wrong.
Do not deploy on the strength of parser tests, do not change the route blindly,
and do not rewrite stored cards until a suitable runtime has been qualified.
