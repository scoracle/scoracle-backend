# Harvester headline calibration, September 27

> **October 7 retirement:** This is a historical experiment record. Its binary annotation/scoring, trace-manifest and Fastino adapter tools were removed for the [Classifier launch](scoracle_classifier_plugin_launch.md). Commands below describe the old experiment; their source is recoverable from commit `5c956402`.

> **2026-10-06 governing update:** This dated record preserves its implementation and evaluation evidence. The [current contract](../../../scoracle-wiki/wiki/Architecture/Harness,%20Plugins,%20Tools,%20and%20LLM%20Contract.md) supersedes articulation-only ownership: cheap code owns collection; expensive compute owns discovery. Plugins assemble traceable clues through tools; the LLM reasons across them, discovers what they support and articulates the answer. Historical “plugin owns WHAT/model owns HOW” statements below do not govern new builds.

## Measurement

The read-only `harvest-headline-relevance-v1` replay sent the target team and Google headline, with no article
body or description, to the production Laya endpoint for 300 query/team edges
from pipeline run 333. It sampled 100 each from the previous article-text
classifier's `relevant`, `irrelevant`, and `unclassified` strata. There were no
transport or contract failures. All 90 edges repeated from the 30-per-stratum
pilot returned exactly the same probability and checkpoint revision
`55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851`.

The nightly candidate population behind those strata was 3,890 previously
relevant, 1,030 previously irrelevant, and 2,003 unclassified query/team edges.
The table estimates publisher-read volume by weighting each sampled stratum by
its population count. Historical article-text choices are **comparison signals,
not human relevance labels**. The unclassified group includes acquisition
failures, so its opening quality is unknown.

| V1 Harvester read threshold | Old relevant retained / 100 | Old irrelevant read / 100 | Unclassified read / 100 | Estimated cohort read rate |
| ---: | ---: | ---: | ---: | ---: |
| 0.05 | 87 | 66 | 81 | 82.1% |
| 0.10 | 77 | 44 | 70 | 70.1% |
| 0.15 | 73 | 34 | 67 | 65.5% |
| 0.20 | 72 | 30 | 62 | 62.9% |
| **0.25, current shadow policy** | **71** | **28** | **58** | **60.8%** |
| 0.30 | 69 | 27 | 56 | 59.0% |
| 0.40 | 68 | 22 | 51 | 56.2% |
| 0.50 | 64 | 18 | 48 | 52.5% |
| 0.70 | 48 | 9 | 34 | 38.1% |
| 0.90 | 5 | 0 | 7 | 4.8% |

The current 0.25 boundary could withhold 29% of articles the old body pass
called relevant. That is a warning about potential recall loss, not a measured
false-negative rate. Likewise, the 28% old-irrelevant read rate is not measured
false-positive rate: a headline can warrant a read even when the publisher
opening is unusable. A 0.90 boundary would retain only five of 100 old
relevant cases in this sample and is unsuitable as a default on this evidence.

The old-relevant stratum also varies by sport at 0.25: 13/26 football,
22/27 NBA, and 36/47 NFL edges pass. These are small, uneven subsamples and
may reflect source or headline mix, but the football gap is a reason to include
sport diversity in review. It is not evidence for a sport-specific threshold
yet. The blinded packet includes 32 football, 14 NBA, and 26 NFL cases.

The v1 wording told Laya that uncertain but plausible coverage was relevant
for reading. That placed a Harvester admission preference inside the model's
predicate. `harvest-headline-relevance-v2` instead defines exact sports-entity
subject matter or direct consequence and asks Laya to express uncertainty in
its probability; Harvester alone applies the admission policy. A second
read-only replay of the same 300 edges had zero failures. The mean absolute
probability change was 0.0949, and 36/300 edges crossed the 0.25 boundary
(17 old-irrelevant, five old-relevant, 14 unclassified). This demonstrates
question wording sensitivity, not which wording is more accurate.

| V2 Harvester read threshold | Old relevant retained / 100 | Old irrelevant read / 100 | Unclassified read / 100 | Estimated cohort read rate |
| ---: | ---: | ---: | ---: | ---: |
| 0.05 | 89 | 76 | 86 | 86.2% |
| 0.10 | 78 | 46 | 74 | 72.1% |
| 0.25, current shadow policy | 70 | 27 | 52 | 58.4% |
| 0.50 | 53 | 13 | 41 | 43.6% |
| 0.90 | 0 | 0 | 2 | 0.6% |

For v2 at 0.25, old-relevant retention is 12/26 football, 22/27 NBA, and
36/47 NFL. The sample still cannot establish actual missed useful coverage.

## Review before policy selection

`harvest_headline_review` produces blinded headline and opening review packets
on the production host from a replay JSON file. It selects 12 unique articles
from each of the six old-stratum by current-boundary cells: six development and
six holdout cases per cell, 72 total. Scores and old strata remain in a
separate manifest. The two source packets and blank label sheets are created
with the host umask set to `077`; keep them on that host. Review the headline
packet before opening the source text. Record whether the headline warrants a
publisher read, then whether the opening is useful and which character themes
it supports. `unsure` is valid and should not be silently converted to no.

The current v2 run-333 packet is `/tmp/harvester-headline-review-v2-run333-20260927` on
`archbox`. It has 36 development and 36 holdout cases; all files are mode
`0600`. All 24 selected `unclassified` cases lack a retained publisher
opening, so they support headline/read labeling only. The other 48 cases have
source openings for second-stage review. A missing opening is an acquisition
state, not a negative relevance label. The earlier v1 packet is superseded;
labels from it must not be joined to the v2 manifest by case ID. The v2 packet
contains 29 football, 15 NBA, and 28 NFL cases.

The balanced packet is deliberately enriched for boundary disagreements. Its
raw label percentages must not be reported as production precision or recall.
`harvest_headline_score.py` computes approximate inclusion-weighted recall,
precision, and publisher-read volume for candidate policies from each completed
label sheet. Score the development sheet while choosing the Harvester rule;
open the holdout sheet only after the rule is frozen. The current split is
article-disjoint but not verified story-disjoint. Verify the chosen plugin rule
on fresh, untouched nightly results as well. The
existing 84-case annotation queue and 120-article seed use provisional Codex
labels; neither is human gold. No numeric read threshold or question wording
should be promoted to live delivery as calibrated until that review exists.

The read-only replay file contains only IDs, hashes, Laya output, and timings.
It writes no database rows, fetches no publisher page, and enqueues no character
assignment. Review packets contain publisher text and URLs, so they remain on
`archbox` and are never copied into the repository or local machine.
