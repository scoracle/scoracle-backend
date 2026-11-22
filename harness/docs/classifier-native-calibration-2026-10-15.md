# Native Kev calibration and capacity — October 15, 2026

Historical per-target baseline, superseded by the [four-score article implementation](classifier-four-score-calibration-2026-10-15.md). Statements about the current bank below describe the earlier experiment.

Plumbing committed as `3e0e1e11`. Calibration uses the native Kev GGUF on Archbox's
GTX 1070 Ti; the retired Laya/Python/GLiNER measurements are historical experiments.
The [machine-readable receipt](../fixtures/classifier/native-calibration-2026-10-15.json)
records the model fingerprint, candidate questions, sample limits and private receipt hashes.

## Released performance change

Keep four concurrent article workers; increase native question slots from four to eight,
with total context 65,536 and 8,192 tokens per slot. The installed llama.cpp
`server_decision_group_tasks` groups questions by available slot count and shares their
common prefix. Six questions previously needed two groups; eight slots fit them in one.
Question slots and concurrent article workers are different resource counts.

With production Classifier inference briefly paused, replayed the same 24 retained
article/target pairs, six unchanged questions and four concurrent requests. Acquisition
continued. The order was four slots, eight, eight, four; no test wrote spectra or deliveries.

| Native slots | Run 1 | Run 2 | Mean requests/second |
| --- | ---: | ---: | ---: |
| 4 | 26.52 s | 26.54 s | 0.905 |
| 8 | 15.14 s | 14.57 s | 1.616 |

This is **1.79× request throughput** on this sample. All 96 requests completed; across
the 144 probabilities per run there were no routing-threshold crossings compared with
the first run. Maximum probability difference was 0.00638 (0.638 points on the stored
0–100 scale); repeated four-slot execution also varied slightly. This is batch/runtime
qualification, not proof of semantic correctness.

Eight slots deployed at 14:02:55 EDT. Health asserted eight slots of 8,192 tokens each;
GPU memory was approximately 2,335 MiB. Kev and the Classifier worker are active, with
automatic restart configured. The temporary benchmark server was removed. A rollback
copy is retained at Archbox `~/tmp/run-classifier-model-before-eight.sh`.

## Question count and prompt findings

Five predicates currently drive all routing: direct subject, emotional charge,
availability, movement and staffing. `relevance.opponent_context` has no routing consumer.
Removing it is the smallest viable reduction. Six and five unchanged questions produced
the same threshold decisions on all 105 shared development labels; maximum probability
difference was 0.00477. At eight native slots, either bank fits in one group, so dropping
one question no longer removes a repeated full-article prefill.

Movement and staffing nominate the same downstream worlds, so a four-question bank
could merge them into personnel changes. Tested that candidate rather than assuming
equivalence. It did not improve this development set. Keep them separate while measuring
their individual error patterns. Production questions and thresholds remain unchanged.

The pilot contains 12 authored synthetic controls and nine source-reviewed real cases.
The real sample deliberately emphasizes unrelated targets and unavailable pages; labels
were assigned by the coding agent, not independently adjudicated. These are development
diagnostics, **not representative, held-out or human-reviewed accuracy**. Timing the
interleaved prompt candidates under production load does not isolate prompt speed.

| Bank | Wrong shared predicate labels | Brier score |
| --- | ---: | ---: |
| Original six, excluding unused opponent label | 12 / 105 | 0.1302 |
| Original five | 12 / 105 | 0.1301 |
| Longer target-scoped five | 16 / 105 | 0.1454 |
| Five with literal target names in questions | 16 / 105 | 0.1270 |

Source excerpts and all raw responses remain in the private paths recorded in the
receipt. The merged original four made 12 errors across 84 labels; the longer scoped four made
16 across 84. Different banks have different label denominators, so their percentages
are not a controlled comparison of overall classification accuracy.

The longer relevance question reduced false positives but missed eight of nine expected
direct subjects. It was rejected. Literal target names also introduced errors. A longer
instruction is not automatically a better instruction for this small decision model.

Topic nomination should next be tested jointly with target relevance. In this development
sample, requiring `direct_subject >= 50` for Influencer, Scout and Insider reduced their
false nominations from 12 to 1, with their four missed nominations unchanged. Journalist
already uses direct subject and is unchanged. This is a promising *candidate policy*, not
an independently validated release. It could miss useful opponent/context reporting.

The user prefers fewer SmolLM3 calls, accepting missed stories. On all 2,614 retained
native target spectra at 14:07 EDT, the current rules produce 2,713 character nominations.
Requiring direct-subject score at least 50 reduces this to 1,406; at least 60 reduces it
to 596 (78% fewer). These are **nominations, not measured generation calls**: downstream
batching, deduplication and structured-record checks also affect call volume. The false
negative rate on this unlabeled production cohort is unknown. Treat 60 as the next
precision-oriented candidate; 65 and 70 already lose important pilot positives.

Changing only cutoffs cannot repair reversed score ordering: an unavailable transfer
blog scored movement 79.1, whereas a genuine transfer denial scored 29.6. The gate is
supposed to nominate discussion, including denials; characters determine what actually
happened. A higher movement cutoff would preserve this false positive and worsen recall.

## Highest-impact next calibration work

1. Review complete article extraction before identity discovery. Menus, related-story
   links and standings contribute spurious candidate targets; RSS provenance also includes
   unrelated abbreviation matches. Preserve source history when correcting acquisition.
2. Handle legitimate league-wide injury and depth-chart pages efficiently without
   silently removing their players. In the production sample, 34 of 184 articles consumed
   1,342 of 2,131 target evaluations (63%). A target cap would hide useful evidence.
3. Establish source-grouped development and untouched evaluation samples, covering
   positive topics, denials, failed moves, opponents, historical passages, unavailable pages
   and all three sports. Independently review labels before claiming production accuracy.
4. Compare topic-plus-relevance routing and per-predicate operating points on those
   samples. Choose the acceptable missed-story versus extra-character-call tradeoff.

Do not interpret a score of 80 as demonstrated 80% correctness. The earlier launch notes
describe older models, keyword proxies and cross-model agreement. The current pilot does
not establish a global Kev accuracy figure. Upstream also distinguishes validated context
and task-dependent performance in the [Kev model card](https://github.com/jaredpalmer/kev/blob/main/docs/model-cards/kev-0.8b.md).

Capacity measurements below concern acquisition and Classifier only. SmolLM3 character
work remains queued; its end-to-end daily completion time has not been measured.

## Live throughput and estimates

Before the slot change, the 30-minute production window completed 94 articles and 1,101
target spectra: 188 articles/hour, averaging 11.71 targets per completed article. An article
with 118 legitimate identity candidates costs much more than a two-team report; spectra
per second cannot be treated as articles per second.

From 14:02:55 to 14:07:52 EDT after the change, 41 articles both started measurement and
completed classification, plus work on four articles still in flight: **497 articles/hour**
over this short window. There were 384 target evaluations, zero native error receipts,
median request latency 1.37 seconds and p95 2.05 seconds. Completed articles averaged
7.85 targets, so this article-rate improvement also reflects a lighter mix. Holding the
earlier 11.71-target mix at the observed target throughput projects about 397 articles/hour.

| Workload | Current short-window mix | Earlier heavier target mix |
| --- | ---: | ---: |
| 8,500 new articles, Classifier only | 17.1 hours | 21.4 hours |
| Approximately 43,948 queued article tasks, no new arrivals | 3.7 days | 4.6 days |
| Same queue while 8,500 new articles arrive daily | 12.9 days | about 43 days |

The exact computed estimates and queue counts are in the machine-readable receipt.
These are conditional projections from a **five-minute** observation, not a multi-day
capacity guarantee. Daily demand requires 354 articles/hour; the heavier-mix estimate has
little spare capacity, so catch-up time grows sharply as that rate approaches demand.
Longer articles, acquisition failures, retries and input changes can extend completion.
Acquisition was around 10–11 articles/minute before the benchmark and continues separately;
it is not currently the measured limiting stage. Failed acquisition rows remain retained
for retry and cannot be promised a successful completion date.

## Updated target

This is the historical per-target baseline. See the [four-score article report](classifier-four-score-calibration-2026-10-15.md) for the later implementation and measurements.

The user prioritizes the most impactful narratives and requires daily Classifier work
around 1–2 hours. The per-target baseline above does not meet that requirement. At the
observed 384 target evaluations per 297 seconds, one evaluation per article would take
about 1.8 hours for 8,500 articles, before acquisition and persistence. Test an article-level
impact/topic pass before expanding targets, and compare native decision models on that
same task. This projection is not a qualified article-level implementation or throughput result.
