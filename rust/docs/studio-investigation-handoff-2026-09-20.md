# Studio investigation handoff — September 20

## Status

The hallucination problem is not solved and no single culprit has been proven. Several real harness defects were found and corrected. Do not treat passing Rust tests, valid JSON, or an enabled abstention path as proof of grounded prose. Current Scout version: s54; output contract: rating-commentary-v6. Work is local and uncommitted; production was not deployed or modified. Preserve the existing working tree.

## User's direction

Keep shared claim selection and story structure. A supported 96th-percentile measurement is already worth interpreting. Scout tells its part of the entity's story, not a complete biography or a complete offensive/defensive profile. Identity metadata cannot establish statistical causes. Missing is unknown, observed zero is evidence, and omitted evidence is not necessarily missing at the source. Stop comparing models. Work on the evidence/harness, not model-specific prohibitions or a larger ban list.

## Confirmed local changes

- Restored shared claim selection/story structure after the omission experiment.
- Scout accepts explicit JSON null, completes without retry, and publishes a null-body marker with called provenance. Shared instruction/schema helpers exist, but other voices have not been enabled for this null contract. Analyst now respects the latest Scout marker rather than retrieving older non-null prose.
- Fixed a reproduced preparation defect: prior-season history with no compatible comparisons erased all current datapoints and emitted an incorrect missing-identity explanation. Empty comparisons now retain current evidence. Regression covers changed league and changed measurement.
- Final s54 cleanup: the legend limits magnitude interpretation to a supplied quality z, identifies evidence as selected rather than exhaustive, and differentiates no supplied measurements from withheld unidentified measurements. These are evidence-description corrections; they have not been claimed as a model-quality fix.
- Earlier work preserved units and dated facts in memory rendering, standardized polarity presentation, removed retired Scout strengths/weaknesses scaffolding and distinguished unavailable trend from steady.

## Most useful evidence

1. [`studio-evidence-contract-2026-09-20.md`](studio-evidence-contract-2026-09-20.md): granular source-to-prompt field inventory and reproduced evidence-loss bug. Its remaining legend/selection recommendations are partially addressed by s54 as described above.
2. [`studio-pressure-audit-2026-09-20.md`](studio-pressure-audit-2026-09-20.md): same-model ablations. Removing graph language, raw values, both, or most of the character brief did not eliminate invented mechanisms. Removing the unavailable magnitude demand changed the turnover certainty, but did not solve overreach.
3. [`studio-abstention-test-2026-09-20.md`](studio-abstention-test-2026-09-20.md): Ornith 9B wrote a card even while saying there were no supportable claims. Direct schema compatibility probe returned null. Offering a pass works technically but does not resolve expansion of a valid claim.

Exact requests/raw results and diagnostic scripts are under ignored `logs/studio-*` directories. Frozen rich NBA cases are synthetic semantic stress tests, not actual current SQL-adapter exports. In particular, blocks are the underlying SQL measure for NBA Rim Protection; the fixture did not preserve that definition. Do not repeat the earlier inference that a blocks interpretation is necessarily false for live data.

## Next concrete work — preserve meaning at the producer

Start with one NBA player evidence path through the existing SQL producer and Rust adapter. Avoid building a parallel Rust dictionary of category meanings.

- Inspect deployed `rating_measurements`/rating bundle definitions and representative read-only rows before claiming production incidence. This investigation inspected repository source, not deployed data.
- Repository migration 252's NBA and NFL branches return `label` as `measure`. NBA Rim Protection derives from `blk`, Playmaking from `ast`, Ball Security from `turnover`, and On-Court Impact from `plus_minus`. Return/persist precise source measurement identity separately from the display label. Use a new migration, not edits to applied history.
- Preserve unit/time basis and source coverage. Existing `stat_definitions` and rate metadata can describe single-source keys; derived formulas require explicit formula identity/meaning. Retain numerical calculation parity while enriching the evidence contract.
- Carry that meaning through stored breakdown, `RatingDatapoint`, prepared prompt and input hashing. Keep cohort, percentile and z semantics distinct; do not relabel a percentile as ordinal rank or a technique/causal observation.
- Freeze an example through the real preparation path. Verify the exact prompt against source keys, formula, units, missingness and comparison population before evaluating prose. Start with one high-standing positive stat and one negatively oriented stat.

Separately unresolved: provider-suppressed zero versus missing in FPL import/memory context; undefined-spread z fallback to zero; per-measure coverage and cohort cardinality; reporting actual selection omissions. Do not globally remove zero defaults without understanding the provider contract, and do not assume a selected partial profile proves source data is absent.

## Verification and operations

Final checks: 509 library tests and 14 eval tests passed; 57 database tests skipped because they require an isolated migrated database. Formatting and diff checks passed. Clippy all targets with warnings denied passed. No model calls in the final evidence cleanup. Temporary isolated Ollama sessions from earlier experiments were stopped; production Ollama, worker routes and databases were left unchanged.
