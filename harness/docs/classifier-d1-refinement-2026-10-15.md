# Native d1-3B refinement — October 15, 2026

**Keep Kev live; retain d1-3B as a serious native challenger.** Model-specific questions
improved d1's development results and probability error. The frozen candidate did not
beat the incumbent on the user's preference for fewer extra character reviews in a
separate twelve-article check. Neither sample establishes production accuracy.

The live four-score implementation is committed as `ee45e201`. Its [deployment and
capacity report](classifier-four-score-calibration-2026-10-15.md) records roughly 71
minutes for 8,500 successful Classifier articles in a short persisted-throughput window.
The previous matched native-only d1 benchmark projects about 119 minutes before database
work. Acquisition, Graph and character completion are separate budgets.

## What changed in the experiment

LiquidAI documents optional `true` and `false` descriptions for native `noul` questions.
The installed llama.cpp parser supports these fields. Tested three four-question banks:

- A mixture of the strongest previously tested short and longer questions.
- Literal subject questions with explicit positive/negative descriptions.
- Character-usefulness questions with those descriptions and explicit sport scope.

All calls use the native GGUF and `/v1/systemone`; Python is only a standard-library HTTP
load generator. There is no Python model runtime, new dependency or replacement service.
Reference: [official d1 question schema](https://huggingface.co/LiquidAI/d1-3B#questions-and-answers).

The development set contains 41 cases. Four explicit unavailable-page cases are excluded
from routing-policy selection because the deployed acquisition guard rejects them before
inference, leaving 37 cases and 148 role labels. The labels come from the earlier synthetic
and source-reviewed development set plus three boundary cases concerning wrong-sport
material, a front-office appointment and recent movement within a retrospective article.
Those three informed the rubric, so they were deliberately excluded from the later check.

Per-role cutoffs were selected on development data using cost `2 × false nomination +
miss`, reflecting the user's preference for fewer SmolLM3 calls. The grid was 30–90 in
five-point increments; ties prefer fewer false nominations, fewer misses, then a higher
cutoff. Longer rubrics were not uniformly better: explicit descriptions hurt Journalist,
while usefulness wording improved Scout. Per-role bank choice used development cost,
with lower Brier score breaking ties. This is a development optimization, not a claim
that these operating points generalize.

The [frozen offline candidate](../fixtures/classifier/d1-character-candidate.json) uses:

| Role | Question family | Cutoff |
| --- | --- | ---: |
| Journalist | Longer meaningful-reporting question | 70 |
| Influencer | Emotional usefulness with explicit yes/no descriptions | 60 |
| Scout | Availability/roster usefulness with explicit yes/no descriptions | 85 |
| Insider | Concise movement question | 60 |

The exact candidate was hashed and archived before the separate-set inference. Replaying
the assembled four-question bank verified its behavior rather than assuming scores from
separate banks would compose without change. Production still uses Kev and the existing
v5 bank; no route, threshold or character enrollment was changed by this experiment.

## Separate-source check

Twelve additional complete retained articles were labeled before looking at their model
responses: four NFL, four NBA and four football sources. Cases cover substantive analysis,
expressed reactions, injury and roster reports, stats tables, schedules and navigation.
Seven-word source shingles were compared against development sources; no same-story
copy was identified. Shared publisher navigation was the largest overlap.

Labels were assigned by the coding agent, not independently adjudicated. This is a small,
selected source-level holdout with 48 correlated role judgments, not 48 independent
articles or a representative final evaluation. The incumbent policy had less development
tuning than the new challenger; compare deployed-policy behavior, not intrinsic model
quality under equal optimization budgets. No further thresholds were tuned from these
results. The user explicitly confirmed that Journalist should prioritize reported stories
and analysis, with statistics-only pages treated as negative examples.

| Policy | Development false nominations / misses | Separate-set false nominations / misses | Separate-set Brier score |
| --- | ---: | ---: | ---: |
| Current Kev | 5 / 9 | **1 / 1** | 0.1151 |
| Frozen d1 candidate | **3 / 8** | 3 / **0** | **0.0690** |

All 106 model requests completed with four typed probabilities and no transport errors.
These are protocol successes, not semantic passes.

On the separate articles, d1 nominated a rankings table for Journalist and a scoreboard
page for Journalist and Influencer. Kev's extra nomination was a navigation-heavy game
page; its miss was an Insider nomination for a short roster update inside a longer injury
article. The false-negative cost differs by story importance; aggregate counts cannot
establish that every miss or extra call matters equally.

D1's lower Brier score means its probability estimates were closer to these provisional
labels overall. It does not prove globally calibrated probabilities. At the frozen
operating points, the user's precision-weighted separate-set cost was 6 for d1 versus 3
for Kev. There is no sufficient evidence here to promote d1 into the live pipeline.

## Next useful evaluation

The next promotion check should use a larger source-grouped, independently reviewed set
with explicit narrative-impact labels, then freeze the questions and cutoffs before a new
untouched evaluation. Include genuine major developments, low-impact updates, denials,
historical context and publisher furniture. Inspect the most important missed stories
separately. Character acceptance can provide review candidates; agreement with a character
model is not ground truth.

Extraction quality remains a high-impact shared issue. Do not add another per-entity model
fan-out, a broad rule-based sports classifier, or an automatic cascade to compensate for
this small pilot. Keep one complete-article/four-score request, make the source cleaner,
and promote a replacement only when the desired tradeoff and capacity are demonstrated.

The [aggregate receipt](../fixtures/classifier/d1-refinement-2026-10-15.json) records the
candidate freeze, operating points, development runs, final comparison and limits. Private
article text and raw responses remain in the user-approved restricted Archbox archive
`/mnt/data/backup/scoracle/classifier-calibration/2026-10-15`.
