# Harvester headline calibration, September 27

## Measurement

The read-only replay sent the target team and Google headline, with no article
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

| Harvester read threshold | Old relevant retained / 100 | Old irrelevant read / 100 | Unclassified read / 100 | Estimated cohort read rate |
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

The run-333 packet is `/tmp/harvester-headline-review-run333-20260927` on
`archbox`. It has 36 development and 36 holdout cases; all files are mode
`0600`. All 24 selected `unclassified` cases lack a retained publisher
opening, so they support headline/read labeling only. The other 48 cases have
source openings for second-stage review. A missing opening is an acquisition
state, not a negative relevance label.

The balanced packet is deliberately enriched for boundary disagreements. Its
raw label percentages must not be reported as production precision or recall.
Use inclusion weights and an article/story-group split for policy comparison,
then verify the chosen plugin rule on fresh, untouched nightly results. The
existing 84-case annotation queue and 120-article seed use provisional Codex
labels; neither is human gold. No numeric read threshold or question wording
should be promoted to live delivery as calibrated until that review exists.

The read-only replay file contains only IDs, hashes, Laya output, and timings.
It writes no database rows, fetches no publisher page, and enqueues no character
assignment. Review packets contain publisher text and URLs, so they remain on
`archbox` and are never copied into the repository or local machine.
