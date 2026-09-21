# Scout-specific relationship task — 2026-09-20

The user clarified that a character should find relationships only within its own scope. Tested a task-scoping addition against the prior omission condition, with metadata retained. No runtime guard or new prohibition list was added; no candidate was promoted to production.

## Candidate

Removed the same four general synthesis sentences as in the omission experiment. Added, within the Scout character:

> Explain relationships supported by the measurements about contributions to scoring or preventing scoring and their measured direction. Where the evidence establishes only an individual contribution, interpret that contribution.

Two runs on sparse Noah evidence (zero goals, expected assists unknown, no trend; precise measurement label) and a paired baseline/candidate comparison on the richer Nia rim-protection fixture. The rich fixture contains elite rim protection, strong defensive rebounds and screen assists, and a poor quality percentile for turnovers. Same local Granite 4.2 3B model, thinking false, temperature 0, context 4096, output limit 700. Direct provider calls, no database or production correction retries.

Verified that the sparse candidate differs from the earlier no-synthesis request only by the scoped task addition, its two requests are identical, and the rich pair has identical evidence. All four calls completed normally. This is a small diagnostic, not an across-model quality evaluation.

## Findings

| Run | Manual review |
|---|---|
| sparse-scoped-1 | Unknown expected assists became no expected assists; inferred limited preventing-scoring contribution without defensive measurements. |
| rich-baseline | Called the poor turnover percentile a relatively low turnover rate; lengthy stat recap. |
| rich-scoped | Correctly recognized turnover liability, but inferred below-average overall scoring creation from incomplete evidence; continued stat recap. |
| sparse-scoped-2 | Repeated the first sparse response exactly, including both unsupported conclusions. |

The sparse candidate was reproducibly worse on evidence fidelity than the prior omission condition: naming the two broad scoring facets coincided with a verdict on both, despite no evidence for preventing scoring. On rich evidence the candidate fixed the turnover interpretation in this pair, but generalized from available measures to an unsupported overall creation grade.

The task boundary therefore needs to distinguish a character's area of relevance from the specific findings established by this assignment. Naming Scout's whole area of expertise does not establish evidence for every part of it. Scope should select relevant relationships among supported findings; it should not demand coverage or a verdict on an entire facet.

The next useful candidate should begin from the supported findings and allow an unconnected finding to remain complete. The shared form can govern presentation while each character supplies its narrower question. Test this against both sparse and rich evidence before promotion, and retain the upstream data-contract work independently.

Raw requests/results and diagnostic runner: `logs/studio-scoped-relationships-20260920/`.

## Full outputs

### sparse-scoped-1

**Noah Reed's limited scoring and assist contribution in 2025**

Noah Reed played 25 games in the 2025 season. He scored 0 goals and had no expected assists. His recent form is not available, so we cannot describe improvement or decline. The measurements show his contribution to scoring and preventing scoring is limited. No other details are supported by the evidence.

### rich-baseline

**Nia Torres: solid defensive presence with screen-setting support**

Nia Torres is a center who consistently defends the paint and supports the team's rebounding efforts. Her rim protection is strong, with a percentile of 96.0, indicating elite defensive presence at the rim. Defensive rebounds average 10.8 per game, reflecting solid rebounding output that contributes to transition and offensive transition opportunities. Screen assists average 5.9 per game, showing effective playmaking when setting screens, which helps create scoring chances and improves ball movement. Turnovers average 2.8 per game, with a percentile of 28.0, indicating a relatively low turnover rate, though not exceptional. Overall, her defensive contributions are solid, and her ability to facilitate screens adds value beyond pure defense. The profile suggests she is a reliable defensive presence and a contributor in setting up play, but the lack of recent trend data limits interpretation of improvement or decline.

### rich-scoped

**Nia Torres’ strong defensive presence and turnovers limit her scoring impact**

Nia Torres is a center with strong rim protection, high defensive rebounding, and solid screen assists, but she turns the ball over too often. Her rim protection score of 3.2 at the 96th percentile indicates elite defense on the paint. Defensive rebounds of 10.8 per game at the 88th percentile show she consistently grabs boards when needed. Screen assists of 5.9 per game at the 79th percentile reflect effective playmaking off screens. However, turnovers of 2.8 per game at the 28th percentile (poor) suggest she loses possession too frequently, which can lead to opponent scoring opportunities. These measured contributions align with the overall standardized score of 64, suggesting she performs above average in preventing scoring but below average in creating scoring. The sample size of 25 games provides limited context, so trends over time are not reliable. Without additional trend data, we cannot confirm improvement or decline in recent form.

### sparse-scoped-2

**Noah Reed's limited scoring and assist contribution in 2025**

Noah Reed played 25 games in the 2025 season. He scored 0 goals and had no expected assists. His recent form is not available, so we cannot describe improvement or decline. The measurements show his contribution to scoring and preventing scoring is limited. No other details are supported by the evidence.
