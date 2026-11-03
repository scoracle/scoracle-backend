# Three-character monitoring: final report

Final capture: **2026-11-03 02:49:49–51 UTC (November 2, 9:49 PM Detroit)**. This closes the requested monitoring window before its 03:00 UTC deadline. Production continues; the monitoring automation has been deleted.

Counters below are cumulative from **2026-11-02 23:34:50 UTC**, the rollout activation, through the final capture (approximately 3 hours 15 minutes). The first comparison snapshot was at 00:01 UTC. Host captures and database reads are not atomic.

| Character | Saved reviews / distinct targets within character | Qualified for display | Not qualified |
|---|---:|---:|---:|
| Journalist | 1,271 / 1,271 | 1,092 | 179 |
| Insider | 28 / 28 | 28 | 0 |
| Influencer | 1 / 1 | 1 | 0 |
| **Total rows** | **1,300** | **1,121** | **179** |

At capture, **1,121/1,300 saved rows (86.2%)** qualified for display. This is publication state, not factual accuracy or proof that every row is the newest card displayed by every client. All rows retain the October 22 reporting date. Distinct targets must not be summed across characters as unique people/teams. The initial 00:01 snapshot had 67 saved rows and 23 qualified rows; net changes are +1,233 saved and +1,098 currently qualified.

The October 23 edition settled **partial, with failures**, at 01:25:19 UTC. Collection/classification resumed. Character generation stopped around 01:25 because writers correctly wait during the collection phase; there were no new character calls in the final 30-minute interval. At the final capture there were 698 pending Journalist jobs, 342 Insider jobs and 376 Influencer jobs for upcoming work, plus three failed Journalist queue entries. The classifier had 13,775 pending and three running jobs. New arrivals replenish that queue, so its size alone is not a throughput measure.

All six Archbox services, the Mac writer and both Qwen health endpoints were healthy. The controller tick was current. Required stages remained Journalist, Insider and Influencer; the article cap remained ten. No character prompt/model version drift appeared in the saved products.

## Separate policy periods

Strict validation ended at **00:47:29.920975 UTC on Mac** and **00:47:40.680765 UTC on Archbox**. Observation mode then allowed content/style failures past validation while retaining diagnostics. Counts use these per-host boundaries. Prompt text and model weights did not change.

| Period / character | Completed native calls | Retained parser rejection attempts | Advisory warning attempts allowed past validation |
|---|---:|---:|---:|
| Strict Journalist | 1,582 | 976 / 1,582 (61.7%) | — |
| Strict Insider | 197 | 170 / 197 (86.3%) | — |
| Strict Influencer | 1 | 0 / 1 | — |
| Observation Journalist | 913 | 1 / 913 (0.11%) | 744 / 913 (81.5%) |
| Observation Insider / Influencer | 0 | Not measured | Not measured |

There were **2,693 completed native calls** overall. The strict period contained 1,146 rejection receipts across 1,780 calls (64.4%). Under observation, 912/913 Journalist calls passed parsing, including 744 advisory failures; 168 calls had no strict-parser warning. These are attempt-level counts, including repeated evidence. Passing parsing can still abstain or fail later publication checks. The change in rejection rate is the intended policy change, not evidence of better model output.

The single observation-mode rejection was an invalid or duplicate paragraph report ID on Archbox. That structural rejection also failed its queue execution. No additional character transport/admission failures were detected in the monitored intervals. Historical queue failures are separate from parser receipts and must not be counted as additional failed generations.

## Native input and call latency

| Period | Host / character | Calls | Native input min–max | Mean native input | Call p50 / p95, seconds |
|---|---|---:|---:|---:|---:|
| strict | mac / Journalist | 732 | 874–1,668 | 1,211.1 | 8.84 / 29.72 |
| strict | mac / Influencer | 1 | 1,799–1,799 | 1,799.0 | 39.13 / 39.13 |
| strict | mac / Insider | 197 | 971–2,328 | 1,324.2 | 9.28 / 22.79 |
| strict | archbox / Journalist | 850 | 820–1,654 | 1,185.1 | 3.09 / 9.70 |
| observe | mac / Journalist | 436 | 882–1,739 | 1,236.4 | 8.91 / 19.66 |
| observe | archbox / Journalist | 477 | 891–1,742 | 1,213.4 | 3.58 / 9.21 |

Complete native inputs ranged from **820 to 2,328 tokens**. These are measured inputs, not JSON-byte estimates; output/context-window budgets are separate. Percentiles are cumulative within each period/host/character, not subtracted snapshots. Call duration includes request-path waiting and concurrent workloads. Different evidence and concurrency prevent interpreting these as controlled hardware or prompt-speed comparisons. The one Influencer call is insufficient for a latency distribution.

## Diagnostics and review priorities

Observation-mode first-warning categories across 744 warned attempts:

- Named-claim checks: **591/744 (79.4%)**. Review identity matching, aliases, speakers and attribution; the guard may also flag valid paraphrases.
- Fact-relationship checks: **123/744 (16.5%)**. Review whether actions, numbers and comparisons remain attached to the right actor.
- Other validation checks: **30/744 (4.0%)**. Inspect the retained exact diagnostics before assigning a factual or presentation label.

Strict-period normalized categories across 1,146 rejection attempts: named claims 826; fact relationships 222; finding contract 33; target-name opening 13; chronology/causality 3; other validation 48; one heuristic `context_budget` category. That last category is based on text matching and is not verified evidence of native token overflow. Full diagnostics are retained privately. The first failing check can hide additional problems in the same output; these are not exhaustive error counts or a factual audit.

Representative manual-review categories for the corpus are actor/source attribution, cross-story merging, unsupported interpretation, missing qualifications, false abstention, hook/body usefulness and Influencer score meaning. This monitoring window did not add an independent human factual review. A prior live check verified that a warning-bearing Journalist response matched its saved and API-served body; that demonstrated publication behavior only.

Fresh classifier work from 01:25:19 through final capture recorded **4,581 successful execution events and 15 failed execution events** (15/4,596 = 0.33%). All 15 failures were database row-lock contention on `news_articles`, not model rejection or transport/admission errors. They involved 14 jobs, all 14 later observed completing successfully through existing retries. The final interval had 1,668 successful and four failed execution events. No retries were manually reset. Job execution success does not imply admission or a published character card.

## Retained evidence and next comparison

Final private archives:

- Mac: `/Users/scotty/scoracle-worker/monitoring/prompt-baseline-20261102/20261103T024949495902Z/`
- Archbox: `/mnt/data/backup/scoracle/prompt-baseline-20261102/20261103T024950858284Z/`

Exact retained requests, saved normalized products, rejection/advisory response receipts and per-call telemetry remain private. Archive parsing reported zero malformed records. There are 2,020 older undated Mac diagnostic records excluded from this dated window; they were not deleted. Successful raw provider replies are not universally retained. Local checkpoint JSON files contain aggregates only and were not committed or pushed.

For the next prompt comparison:

1. Freeze a case manifest referencing private target/evidence snapshots. Include published, held, rejected and abstained cases, stratified by sport, entity type, evidence count and warning category. Keep repeated evidence grouped.
2. Use exactly the same canonical identity, headlines, publishers, source qualifications and evidence order for both prompts. Preserve model fingerprint, host/runtime, scoring/validation policy and concurrency. Change one prompt component at a time.
3. Review labels independently of parser outcomes. Score factual support, attribution, relationship fidelity, uncertainty, story coverage, hook/body usefulness and score interpretation separately. Retain all outputs, errors, native token counts and timings.
4. Split development cases from an untouched comparison set before tuning. Report paired wins/losses and failures with denominators; do not describe this production corpus as held-out accuracy.

The main limitation is coverage: 1,271/1,300 saved rows (97.8%) are Journalist, with only 28 Insider and one Influencer. Observation mode has no live Insider/Influencer generations in this window. More of those outputs are needed before drawing cross-character conclusions. The 179 hidden saved rows retain their assignment/source qualification state; monitoring did not retroactively publish them.
