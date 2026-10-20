# Premier League throughput replay: 20 October 2026

Scope: the current-season Premier League population in the application database:
20 clubs and 444 players with league 8, FOOTBALL, season 2026 statistics. This is
not a verification of the complete registered league roster. Historical
`teams.league_id` membership would include 30 clubs and is deliberately excluded.

The replay uses Qwen3.5-4B Q4_K_M on the production Mac M4 with 16 GB RAM. Four
physical slots have 10,240 tokens each; requests admit at most 10,000 tokens,
including reserved output. Thinking and context shifting are disabled.

## Results

All 464 subjects have six final stage receipts: **2,784 attempted stages**.
Active replay time was **63m 48s**, including 42.94s of preparation; total elapsed
time was **65m 25s**, including interruption recovery and fetching restoration.
The final results contain 1,146 recorded native responses, plus five responses
from interrupted partial subjects that were repeated. Four slots were active
concurrently; the largest observed input plus reserved output was **6,895 tokens**.
No completed receipt exceeded the token cap or recorded a native response failure.

| Stage | Generated | Abstained | No call | No stats | Failed | Native responses |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Journalist | 35 | 1 | 359 | 0 | 69 | 104 |
| Influencer | 10 | 35 | 419 | 0 | 0 | 45 |
| Scout | 282 | 13 | 0 | 132 | 37 | 657 |
| Insider | 28 | 0 | 426 | 0 | 10 | 38 |
| Analyst | 9 | 0 | 455 | 0 | 0 | 9 |
| Oracle | 293 | 0 | 171 | 0 | 0 | 293 |
| Total | 657 | 49 | 1,830 | 132 | 116 | 1,146 |

Scout used **657 responses for 332 invoked stages**, almost two each; its 325
extra attempts account for 28% of final-run responses. Most Scout failures concern
unsupported stability/direction or treating sparse stored appearances as complete
participation. Journalist failed 69 stages: 68 output rejections and one changed
Classifier source detected before inference. Insider rejected ten outputs. These
are debugging targets, not grounds to relax the guards or label every rejection
a false positive.

At the end, all four model slots were idle; the API, classifier and fetching
services were active; the managed Mac worker retained PID 48242. Memory free
percentage was 43%; swap used 2,749.94 MB, below the 2,997.94 MB pre-test reading.

## Harness interruption

After 452 complete subjects, the progress sampler read a JSONL record while it
was being appended. Its error handler stopped its own child and restored
fetching. This was a benchmark-controller failure. The corrected sampler reads
only newline-complete records. Twelve unfinished players were rerun in a separate
read-only pass, taking 80.17 seconds; input preparation for those subjects was
repeated against the then-current corpus. Fetching was paused again for that pass
and restored afterward.

The aggregate keeps all 452 complete first-pass subjects and all twelve recovery
subjects. It excludes 26 partial duplicate stages from outcome totals, retaining
their five recorded native responses as overhead. Up to four in-flight attempts
could have been canceled without receipts, so the native-response counts are not
a complete count of work at cancellation. Both passes and all original files
remain intact. `recovery.patch` contains only the subject selector added to the
driver. This is a completed two-pass replay, not an uninterrupted nightly run.

## What was timed

The source baseline is `d93c15d75c89bbf3aaed1bb8f73aab43f19d2db0`. The unchanged
production worker runtime is the preceding guard-fix release, `3df810c5189c`.
All database connections in this replay enforce `default_transaction_read_only`.
It neither claims queue work nor publishes products.

Each pass prepares its subjects' inputs before generation, then overlaps eight
subjects through the production router's four inference permits. Journalist,
Influencer, Scout and Insider run concurrently; Analyst receives only newly
accepted Scout/Influencer peers; Oracle receives only this replay's peers.
Missing evidence and missing peers retain the production no-call paths. Studio
corrections and optional-memory admission retries are included. Insider uses the
production parser, schema and activity SQL through its evaluation request builder.

Influencer preparation simulates the production settlement of sources with
unresolved identity or missing/future publication time, keeping a separate local
receipt and reloading memories from the eligible sources. It makes no settlement
writes. An earlier calibration that omitted this step is excluded from timing.

Go RSS fetching on Archbox was paused during the replay and restored afterward.
The API, classifier and managed Mac worker remained active; the Mac worker was
in its idle coverage phase. Input preparation is a short series of read-only
queries, not one transaction snapshot; classification could continue during it.

## Interpretation

The [optimization handoff](../../../docs/character-capacity-2026-10-20.md#optimization-handoff-for-the-next-context)
prioritizes the speed levers, code entry points and next-session checks.

This measures fresh creators and parsers, not a complete overnight edition.
It excludes unchanged-product reuse, queue retries/backoff, publication,
source-claim revalidation, write-time metric normalization and durable handoffs.
In particular, the Oracle input is assembled directly from creator products;
publication can null unmeasured Analyst directions before loading them as peers.
The six stage receipts per subject do not imply six generated products.

`report.py` counts a called Journalist with no narratives and a Scout with an
empty/abstained body as abstentions, preserving the raw driver receipts. A parser
acceptance is not a factual accuracy score. Failed stages stay failed; no guard
was weakened to improve throughput. This league test does not qualify the
four-hour deadline for the whole multi-sport nightly population.

## Receipts and reproduction

`replay.rs` is the exact measured driver; `summary.json` is the aggregate from
`report.py`. Raw prompts, replies, source selections, slot samples, roster,
service receipts and the excluded calibration remain private locally at:

`/Users/scotty/scoracle-worker/releases/premier-league-test-20261020/`

Verify and regenerate the aggregate against those complete receipts:

```sh
python3 fixtures/character-runs/premier-league-2026-10-20/report.py \
  /Users/scotty/scoracle-worker/releases/premier-league-test-20261020/combined
```

The verifier requires every subject/stage exactly once, successful fetching
restoration, matching response totals and every native input plus reserved output
within 10,000 tokens. The driver also asserts read-only SQL and 20 clubs.
`combine.py` documents how the two original passes were assembled; it requires a
new `combined/` directory and refuses to replace the retained aggregate.

To rebuild the historical driver, copy `replay.rs` temporarily to
`examples/premier_league_replay.rs` and run
`cargo build --release --example premier_league_replay`. Running it requires the
private production model/database environment, the deployed memory helper and a
new output directory. Repeating now reads a changed corpus and is a new test.
Raw service backups and credentials are not part of this fixture.

Automatic approval review rejected an earlier plan that stopped production
cognition, canceled classifier replays and cloned much of the production database,
because it exceeded this benchmark's authorization and could copy sensitive data.
None of that plan executed; this test used read-only creator/parser calls instead.
