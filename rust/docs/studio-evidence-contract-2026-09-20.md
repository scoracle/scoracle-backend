# Scout evidence: source-to-prompt audit

> Follow-up s54: the legend now conditions magnitude interpretation on supplied quality z, distinguishes selected evidence from source completeness, and separates empty assignments from unidentified measurements. See [handoff](studio-investigation-handoff-2026-09-20.md).

## Scope and evidence

Repository-source audit of SQL migrations 252/253, their schema snapshot equivalents, the Scout application adapter, the Rust evidence types, prompt rendering, selection and memory context. No deployed database functions or rows were inspected. No model calls were made. Findings below distinguish a reproduced preparation defect from source-contract gaps; these do not establish a single cause for every observed hallucination.

## Reproduced and fixed: valid current evidence erased by incompatible history

`build_rating_request` wraps `build_skill_changes` in `Some` whenever a prior profile exists. That function legitimately returns an empty map for a different league, changed measurement identity, or missing comparable ranks. `model_prompt_profile` previously entered comparative selection for every `Some`, including an empty map, and retained no datapoints. The renderer then reported that measurement identity was unavailable.

The regression starts with current scoring at percentile 95 / quality z +3.10 and defense at percentile 40 / quality z -0.50. With incompatible prior history, both disappeared, although the overall score remained. The failure was reproduced before the fix. Both league-change and measure-change cases now preserve exactly the same current-evidence prompt as having no historical comparison. Nonempty comparison selection is unchanged.

Fix: only enter comparative selection for a nonempty comparison map. Scout version s53 ensures the changed preparation contract affects input provenance/debounce. This defect cannot explain the frozen rich fixture's overreach: that test bypasses profile preparation.

Source: `src/application/scout.rs` comparison construction; `src/studio/scout/mod.rs::model_prompt_profile`; regression `incompatible_history_does_not_erase_current_measurements`.

## What survives each boundary

| Evidence component | SQL/source | Rating payload and Rust | Model-facing consequence |
|---|---|---|---|
| Display label | Curated category | Preserved as `label` | Good descriptive organization; not necessarily a definition |
| Actual measurement | SQL knows source key/formula | NBA/NFL player branches return label as `measure`; football preserves several underlying measurements; many team branches also return labels | Renderer suppresses identical label/measure; source meaning can disappear |
| Value | Extracted or computed | Optional numeric `value` | Rendered number or unmeasured; upstream zero substitution is no longer distinguishable |
| Unit/denominator | `stat_definitions.unit`, source expressions and rate-mode definitions exist | No per-datapoint unit/denominator fields | Renderer applies NBA per-game / other sports season-total blanket text, with general exceptions |
| Raw polarity | Explicit SQL `sign` | Preserved; Rust only treats +1/-1 as known | Clear higher/lower raw-value semantics |
| Standardized distance | SQL raw z | Preserved; Rust multiplies by sign once | Favorably oriented quality z; invalid/nonfinite z is withheld |
| Percentile | SQL `percent_rank` over matching label/measure | `pct` preserved; scoped percentile map also exists | Generic sport/season eligible-population explanation; no exact cohort size or ordinal rank |
| Eligibility | SQL player bundle persists `eligible`; adapter also recomputes a gate | Gate clears ranks for ineligible profiles; Rust datapoint does not retain the eligibility field | Detailed eligibility basis unavailable in the card evidence |
| Population mean/spread | Computed in SQL | Not carried in bundle | Cannot reconstruct absolute effect size, confidence, or sample reliability from percentile alone |
| Coverage | Stats update date and appearances/minutes where available | Profile-wide sample, no per-measure coverage or missingness reason | Missing, inapplicable, provider-suppressed zero and selection omission are not consistently distinguishable |
| Direction | Prior percentile / recent overall-rating trend | Separately computed and supplied | Rank movement is a different claim from absolute improvement; recent overall trend is not a measure-specific trend |

## Concrete measurement examples

| Display concept | Repository calculation | What is lost or easy to overread |
|---|---|---|
| NBA Rim Protection | `blk` | Blocks do not directly measure deterrence, positioning, every defensive action, or a sequence ending in a rebound. SQL returns `Rim Protection` as measurement identity, not `blk`. |
| NBA Playmaking | `ast` | Recorded assists are narrower than all creative play, passing technique or vision. |
| NBA Ball Security | `turnover`, negative polarity | Favorable rank says something about the measured turnover quantity, not handling technique, pressure or decision-making cause. |
| NBA On-Court Impact | `plus_minus` | A broad impact label conceals the exact statistic and invites causal attribution to one player. |
| Football Chance Creation | Key passes if present, otherwise expected assists | These are different measurements; football does preserve this distinction in `measure`. Neither is proof of every form of creative contribution. |
| Football Goals Prevented | Expected goals conceded minus goals conceded, or saves relative to league save percentage | Formula/branch matters; provider-era identity must travel with the value and comparison. |
| NFL Air Yards Responsible | Sum of passing, receiving, kick-return, punt-return, punt and interception yards | This label is not a conventional passing air-yards measurement. Without the formula, ordinary sporting terminology can mislead the model. |
| NFL Points Responsible For | Weighted sum of touchdown categories, field goals and extra points | Constructed contribution score, not necessarily unique team points or independently caused points. |
| NFL Tackles For Loss | Maximum of supplied tackles for loss and sacks | Label conceals fallback/combination semantics. |

The frozen NBA rich fixture is a synthetic semantic stress case, not an exact current-player rating export. It contains screen assists and defensive rebounds absent from the current NBA player measurement list. Team defensive rebounds are display-only and the adapter removes display-only datapoints. This fixture remains useful, but source-to-prompt fidelity requires a separate adapter-built case.

## Missingness and degeneracy

- FPL importer code suppresses zero numeric stats; football measurement expressions restore some missing keys to zero. Memory SQL reads raw source keys and may see unknown where the rating expression sees zero. This is a source-coverage contract issue; globally removing COALESCE would erase legitimate provider-suppressed zeros.
- SQL player/team pipelines filter missing measurement values before ranking. The team NULL-to-average concern is ruled out by `dp.value IS NOT NULL` when populating `_team_dp`; do not report it as a discovered bug.
- Both player and team standardization coalesce a division by unavailable/zero spread to z=0 when a population mean exists. Tied/degenerate populations get no percentile, but the numeric z fallback can still be presented as an ordinary standardized distance. Preserve an explicit degeneracy state before changing the analytical formula.
- Current selection has at most 14 statistics, only two for thin samples, and a bounded set of comparative movements/held anchors. Omitted does not mean unmeasured. The model-facing payload lacks a complete account of which categories were excluded and why; full exclusions exist outside the prompt.
- The legend asks for z-magnitude interpretation even when none is present. Make that task conditional on a supplied valid z; percentile remains sufficient evidence of relative standing.

## Cohort and the 96th percentile

Main player percentiles are computed within the sport/season invocation over eligible rows grouped by label and measure. They are not inherently positional percentiles or football league-only percentiles; separate scoped windows compute those. Raw population size and ordinal rank are not carried in the Scout datapoint. The same player can have multiple league rows, so “eligible entities” is a simplification of the underlying row population.

A high percentile is a valid claim of high relative standing on the named measurement. It does not establish league rank one, best overall player, causal contribution, technical method, or a large standardized difference. Supplying the comparison population explicitly helps preserve the valid claim without exaggerating it.

## Next source-contract work

Start where source meaning is known, rather than reconstructing a Rust label-to-stat dictionary:

1. Preserve source measurement keys or formula identity alongside display labels, including provider/rate variants. Reuse `stat_definitions` for single-stat descriptions/units; derived formulas need their own definitions.
2. Carry per-measure unit, denominator/time basis, observation/coverage status and comparison population through rating storage and the Rust assignment. Distinguish missing, observed zero, degenerate comparison and evidence deliberately not selected.
3. Keep percentile and quality z as distinct observations. Ask for magnitude interpretation only when supported; keep ordinary high-standing claims available.
4. Build representative frozen requests through the actual profile adapter/renderer. Compare source values and meanings to exact transmitted evidence before running prose evaluation.

These are bounded evidence-contract changes to review and implement next. They are not a recommendation for more inference calls, a narrative plan, model-specific rules or a larger ban list. The only production behavior changed in this audit is the reproduced empty-comparison evidence-loss bug; no SQL migration or source calculation was changed.
