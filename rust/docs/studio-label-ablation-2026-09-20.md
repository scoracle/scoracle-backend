# Scout label-only experiment — 2026-09-20

Question: does the surviving category name “Chance Creation” cue fabricated playing characteristics compared with the precise measurement name “Expected assists”?

## Design

Eight calls: two labels × two observation states (unmeasured and measured zero) × two repetitions. Counterbalanced label order between repetitions. Same synthetic Noah Reed fixture, midfielder metadata, 25-game sample, zero goals, absent trend, s50 Scout system, output schema and provider options. Only the text before the creation row's colon changes between labels; the parenthetical `(expected assists)` remains unchanged. These are direct provider calls with no database, production guards or correction retries.

Local model `granite4.2:3b`; thinking false, temperature 0, context 4096, output ceiling 700. Temperature zero does not guarantee identical responses. This small repeated diagnostic is not a statistical estimate of failure prevalence across players or models.

Exact requests, outputs, frozen input, experiment plan and diagnostic source: `logs/studio-label-ablation-20260920/`. Matched wire requests were checked for equality after the single label substitution; repetitions use identical requests.

## Interpretation

The precise label does not reliably ground the story. Legacy wording leads to unsupported positive creation claims, while precise wording leads to unsupported defensive/supporting-role claims. Unknown expected assists is also misinterpreted as evidence of low opportunity or low offensive influence. Acknowledging missing data in one sentence does not keep the rest of the narrative grounded.

The tested requests contain no historical z-score data or SQL output. These failures cannot be caused by the model reading old stored z-scores during this experiment. They are consistent with category/position associations shaping how the model fills gaps. The experiment does not separate the effects of midfield metadata from the wider scouting task; those remain candidates for a subsequent controlled test.

Replacing labels alone is not promoted as a production fix. The upstream zero/missing inconsistency and renderer fixes identified in the context audit are separate issues and still matter for real assignments.

The next discriminating experiment should independently vary position metadata and the scope of the writing task on the same precise-measure fixture, while preserving the observation states. Grade whether each claim is supported, rather than whether its number appears somewhere in the input.

## Results

All eight responses failed manual grounding review. All eight ended normally; prompts used [779, 780] tokens and outputs used [74, 169] tokens. No context/output limit was reached in these runs. This is a failure count for this fixture, not a general failure rate.

| Condition | Unsupported claims |
|---|---|
| legacy-unknown-1 | Unmeasured xA becomes consistent chance creation; unavailable trend becomes not yet stable. |
| precise-unknown-1 | Unknown xA becomes limited offensive influence; invents defensive discipline, positioning and tempo control. |
| precise-zero-1 | Zero values become an inferred support role, defensive stability, limiting transitions and subtle positioning. |
| legacy-zero-1 | Zero xA becomes consistent chance creation; invents no significant variation. |
| precise-unknown-2 | Missing xA becomes lack of opportunities; invents disciplined execution, controlled movement and reliable positioning. |
| legacy-unknown-2 | Unknown creation becomes strong creation and effective ball distribution; confuses current zero goals with recent form. |
| legacy-zero-2 | Zero xA becomes consistent, stable chance creation; invents finishing/decision-making needs in set pieces. |
| precise-zero-2 | Invents defensive work, distribution, positioning and a steady midfield presence. |

## Full model outputs

### legacy-unknown-1

**Noah Reed: consistent chance creator, limited scoring**

Noah Reed is a midfielder who consistently creates chances but does not score. His recent form is limited by unavailable trend data, and his performance is not yet stable. He maintains a solid foundation of chance creation, which is his primary contribution.

### precise-unknown-1

**Noah Reed: solid defensive presence with limited offensive contribution**

Noah Reed is a midfielder who contributes primarily through positioning and defensive discipline, not scoring. His measured contributions show no direct scoring output, and his expected assists are unmeasured, indicating limited offensive influence. The lack of goals and assists suggests a role focused on maintaining structure and controlling tempo, rather than creating or finishing chances. Without additional context on defensive actions or positional stability, the profile remains consistent with a solid defensive presence and limited offensive contribution.

### precise-zero-1

**Noah Reed: limited direct impact, focused on midfield stability and support**

Noah Reed is a midfielder who consistently delivers precise, low-impact contributions in midfield. His goal-scoring and expected assists are both zero, indicating minimal direct scoring or assist potential in the current snapshot. This suggests a role focused on positioning, support, and defensive stability rather than offensive creation or execution. The lack of measurable scoring or assist output supports a profile where his value lies in maintaining structure, limiting opponent transitions, and facilitating teammates through subtle positioning rather than direct involvement in scoring sequences. Given the sample size (25 games) and the absence of trend data, the profile remains consistent with limited direct impact, but the role may adapt based on situational context and team needs.

### legacy-zero-1

**Noah Reed: Consistent Chance Creator, No Scoring**

Noah Reed is a midfielder who consistently creates chances but does not score. His recent form is not available, so we cannot assess improvement or decline. He maintains a baseline level of chance creation with no significant variation observed.

### precise-unknown-2

**Noah Reed: limited goal output, consistent midfield presence**

Noah Reed is a midfielder who consistently contributes through disciplined execution on the field. His measured contributions indicate limited goal-scoring output, with zero goals recorded across the 25 games played. While expected assists remain unmeasured, the absence of a raw value suggests a lack of assist opportunities or consistent playmaking. The profile reflects a role focused on structural stability rather than direct scoring, emphasizing controlled movement and reliable positioning. No additional trends or patterns can be inferred from the available data.

### legacy-unknown-2

**Noah Reed is a midfielder who consistently creates chances but does not score.**

Noah Reed is a midfielder who consistently creates chances but does not score. His recent form is limited by the absence of goals, and his chance creation remains strong despite the lack of scoring. The profile reflects a role focused on playmaking rather than finishing, with no evidence of improvement or decline in recent trends. The absence of goals is a material limitation, and the strong chance creation indicates effective ball distribution. The profile is based on the current snapshot and does not include historical context beyond the sample games played.

### legacy-zero-2

**Noah Reed: Midfielder with limited scoring and consistent chance creation**

Noah Reed is a midfielder who consistently creates chances but does not score. His recent form is limited by unavailable trend data, so we cannot assess improvement or decline. He maintains a baseline level of chance creation, which is currently stable but not exceptional. The lack of scoring indicates a need for better finishing or decision making in set pieces.

### precise-zero-2

**Noah Reed: limited offensive impact, steady midfield presence**

Noah Reed is a midfielder who consistently contributes defensively and supports play in midfield. His recent snapshot shows minimal scoring and assist contributions, indicating limited offensive impact. He maintains a steady presence in midfield, focusing on positioning and support rather than direct scoring. The lack of measurable offensive output suggests he relies on distribution and defensive work, which aligns with his role as a midfielder. There is no evidence of improvement or decline in recent form, and the snapshot reflects his current contribution level.
