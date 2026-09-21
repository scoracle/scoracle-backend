# Finding-first Scout task — 2026-09-20

Compared the previously tested omission prompt with a finding-first add-back on sparse and rich evidence. Metadata was retained. No production prompt, route, schema or runtime guard was changed.

## Candidate and controls

Starting from s50 with the four general synthesis requests removed, add:

> Select a finding established by the supplied measurements and explain its significance as a contribution on the field of play. Related measurements may support that same finding; an individual measured contribution is also a complete finding.

Sparse fixture: Noah Reed, football midfielder, zero goals, unmeasured expected assists, 25 games, unavailable trend. Uses the precise Expected assists label. Rich fixture: Nia Torres, NBA center, elite rim protection, strong defensive rebounds and screen assists, poor turnover standing, unavailable trend.

One call per cell: sparse/rich × omission/finding-first. Granite 4.2 3B, temperature 0, thinking false, context 4096, output limit 700. Direct provider calls, no production correction retries or database. Wire comparisons confirm that each pair differs only by the candidate text; the sparse control exactly matches the prior omission experiment.

## Completed Granite results

| Condition | Manual review |
|---|---|
| sparse-control | Preserves unknown expected assists and unavailable trend; unhelpful stat/metadata recap. |
| sparse-finding | Turns unknown expected assists into no expected assists and invents a consistent-performance headline. |
| rich-control | Preserves the supplied quality bands including poor turnovers; mainly a stat recap. |
| rich-finding | Invents physical presence, offensive-board contribution, consistency and hesitation/poor decisions; weakens strong screen-assist standing to modest/moderate. |

All four completed normally. The omission condition preserves evidence but does not meet the product aim: it recites data and identity. The added request for significance reintroduces unsupported interpretation on both fixtures. This is consistent with prior tests implicating the interpretation task as a trigger in this model. It does not establish which individual word causes the problem, whether the issue generalizes to other models, or that useful interpretation is impossible.

The narrow finding-first candidate is not promoted. Repeatedly rewording the same request on one small model risks confusing prompt-specific sensitivity with a general harness defect. Identical-request checks on other installed models are the next control, with no production route switch.

## Full Granite outputs

### sparse-control

**Noah Reed: 25 games, 0 goals, unmeasured expected assists**

Noah Reed played 25 games in the 2025 season. He scored 0 goals and had unmeasured expected assists. His expected assists are higher is better, and his goalscoring is higher is better. No trend is available to describe recent performance.

### sparse-finding

**Noah Reed's consistent performance**

Noah Reed played 25 games this season. He scored 0 goals and had no expected assists. His performance is consistent with the raw measurements provided. There is no trend available to describe recent form.

### rich-control

**Nia Torres: Strong rim protection and rebounding, but higher turnover rate**

Nia Torres is a center with strong rim protection, high defensive rebounding, and solid screen assists, but a higher turnover rate. Her rim protection is at 3.2 with a 96th percentile, indicating elite performance. Defensive rebounds are 10.8, ranking in the 88th percentile, which is strong. Screen assists are 5.9, ranking in the 79th percentile, also strong. Turnovers are 2.8, ranking in the 28th percentile, which is poor. Overall standardized score is 64, above average. The sample includes 25 games played.

### rich-finding

**Nia Torres: Strong rim protection and defensive rebounding, moderate screen assists, elevated turnovers**

Nia Torres is a center who maintains strong rim protection and solid defensive rebounding, indicating reliable physical presence and ability to contest and recover on the offensive and defensive boards. Her screen assists are modest but consistent, suggesting effective playmaking when setting screens. Turnovers are elevated, reflecting occasional hesitation or poor decision making during play execution. The overall standardized score of 64 suggests above-average performance relative to peers, though not exceptionally high. The rim protection percentile of 96.0 (elite) and defensive rebounds percentile of 88.0 (strong) highlight her role in limiting opponent scoring and contributing to team possession. These findings are based on the provided measurements and sample size of 25 games.

## Second-model controls and execution limit

Installed Ministral 3 8B was requested with the identical sparse candidate (only the model field changed). It returned no response before the diagnostic was interrupted after approximately 235.8 seconds. A running-model inspection during the wait showed only Granite loaded. No server restart, model unload or production route change was performed. The reason for the delay was not established.

The same sparse request was then sent to installed Qwen 3 4B with a 120-second client timeout. It timed out after 120.0 seconds without an output. This is an execution failure, not a quality result. Both alternate-model wire requests were verified identical to Granite except for the model field.

These attempts do not support a cross-model conclusion. Before more prompt tuning or adopting a model route, complete the identical-request comparison when the local service can execute the alternate model. The complete prompt/data audit and previously fixed renderer defects remain independent of this diagnostic limitation.
