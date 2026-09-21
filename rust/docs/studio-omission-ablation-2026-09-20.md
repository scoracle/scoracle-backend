# Scout omission-only experiment — 2026-09-20

The user proposed removing the cause of unsupported correlations instead of adding another prohibition. The unvalidated shared metadata-rule candidate was withdrawn. No runtime guard was added. All six previous publishing prompt versions and their fixtures were restored; earlier accepted work, including the context-rendering fixes, was retained. A captured candidate remains under `logs/studio-metadata-boundary-20260920/` with an explicit interrupted/withdrawn status.

## Design

Use the precise-label s50 baseline from the prior test unchanged: synthetic football player Noah Reed, midfield position, 25 games, zero goals, unknown expected assists, no recent direction. Compare two factors independently: position present/absent and four story-connection requests present/absent. Deletion only; no replacement commands or new guards.

The omitted sentences were:

- Connect the supplied measurements into a sporting read of the available profile.
- Read them together to describe the contributions they establish and the relationships between them.
- Write a qualitative scouting read of what those contributions mean together.
- Connect the selected findings, evidence and meaning into a coherent read.

Other instructions remain, including the Scout persona, partial-evidence rules, paragraph/card structure and the request not to reproduce the graph. This tests those four sentences as a group, not every storytelling instruction and not each sentence individually.

Model: local `granite4.2:3b`, thinking false, temperature 0, context 4096, output limit 700. Direct provider calls, no database or production retries. One initial call per condition; the no-synthesis/position-retained condition was then repeated. Exact wire comparisons verified the baseline matched the prior experiment, matched cases differed only by the specified deletions, and the repeat used the identical request.

## Results

| Condition | Manual review |
|---|---|
| baseline | Invented disciplined execution and limited scoring ability; missing xA became a potential gap in ability. |
| no-synthesis | No unsupported role/technique/correlation identified; repeats stats, metadata and awkward higher-is-better wording. Product quality fails. |
| no-position | Invented defensive/support role; unknown xA became lack of assist potential. |
| neither | No unsupported role/technique/correlation identified; repeats stats/metadata and refers to provided data. Product quality fails. |
| no-synthesis-repeat-2 | Same body as first no-synthesis run, different headline; no invented playing traits identified. Still a stat recap. |

All five responses completed normally. The repeated no-synthesis condition produced the same body and a different headline. The three responses without the four story-connection requests contained no invented playing traits identified in manual review, but all were poor product outputs: stat recaps rather than useful scouting prose. Removing position alone did not prevent unsupported role inference.

This is evidence that the request to synthesize relationships contributes to gap-filling on this sparse fixture. It is not proof of a universal cause, an estimate across models, or a reason to remove synthesis from rich evidence. The small diagnostic has no database data or old z-scores, so those cannot explain its fabrications.

## Decision and next isolation

No omission candidate was promoted to production: the resulting stat recap does not meet the user's aim. Continue deletion-first testing by adding back the four sentences one at a time against the no-synthesis condition, then test the promising brief against a rich profile. This can separate useful interpretation from a demand to manufacture relationships when only a limited conclusion is supported. Preserve metadata for naming and relevance; do not assume that removing it alone solves the problem.

Artifacts: `logs/studio-omission-ablation-20260920/` contains the plan, exact requests, responses, review and diagnostic runner. No new production prompt rule or runtime guard remains from the withdrawn candidate.

## Full outputs

### baseline

**Noah Reed: limited direct scoring and uncertain assist potential**

Noah Reed is a midfielder who consistently contributes through disciplined execution on the field. His measured contributions indicate limited direct scoring ability, as he has recorded zero goals in the current season. While his expected assists are unmeasured, the lack of a raw value suggests a potential gap in his ability to create passing opportunities for teammates. The sample size of 25 games provides a moderate window for observation, but the absence of recent trend data limits interpretation of form. Without additional context on positioning, technique, or tactical context, the profile remains focused on the quantified contributions established by the measurements.

### no-synthesis

**Noah Reed (FOOTBALL player, MF) - 25 games, 0 goals, unmeasured expected assists**

Noah Reed played 25 games in the 2025 season. He scored 0 goals and had unmeasured expected assists. His expected assists are higher is better, and his goalscoring is higher is better. No trend is available to describe recent performance.

### no-position

**Noah Reed: limited scoring output**

Noah Reed shows limited scoring contribution in this season, with zero goals recorded. His expected assists are unmeasured, indicating a lack of assist potential in this snapshot. The lack of measurable assists combined with no goals suggests a role focused on defensive or support work rather than direct scoring. Without additional context, the profile remains consistent with minimal offensive output.

### neither

**Noah Reed's 2025 Season Performance**

Noah Reed played 25 games in the 2025 season. He scored 0 goals and had unmeasured expected assists. His recent performance trend is unavailable, so direction is unknown. These findings are based on the provided data.

### no-synthesis-repeat-2

**Noah Reed: 25 games, 0 goals, unmeasured expected assists**

Noah Reed played 25 games in the 2025 season. He scored 0 goals and had unmeasured expected assists. His expected assists are higher is better, and his goalscoring is higher is better. No trend is available to describe recent performance.
