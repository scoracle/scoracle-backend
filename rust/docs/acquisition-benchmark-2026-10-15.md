# Acquisition capacity — October 15, 2026

The current bounded acquisition policy projects **about 530–685 accepted articles in
two hours**, or **6.2–8.1% of an 8,500-article planning day**. It does not meet a goal of
acquiring most of that daily volume within one or two hours.

| Window | Accepted articles | Share of 8,500 |
| --- | ---: | ---: |
| One hour | 265–342 | 3.1–4.0% |
| Two hours | 531–685 | 6.2–8.1% |

These are projections from a 192-article cold acquisition cohort, not measured hour-long
runs. The lower estimate includes the final rate-limit retry tail; the upper uses the
active intake period before only that deferred article remained. This range is a
comparison of timing methods, not a statistical confidence interval.

## Measured cohort

The latest completed production sweep discovered 6,582 new URLs. All were Google News
wrappers and none had supplied full text. The benchmark sampled 192 proportionally by
sport, using a fixed MD5 ordering within each sport: 72 football, 54 NBA and 66 NFL
articles, representing 98 distinct publisher names. Selection did not require a prior
successful acquisition or favor easy extraction.

The actual Rust `AcquireHandler` ran on Archbox with four concurrent channels, one shared
web broker, the existing two-second domain spacing, real HTTP requests, canonical identity
queries, source retention, claim fencing and queue completion. The disposable database
started with no source documents, resolved URLs or acquired bodies. It copied production
schema and canonical reference data into a separate temporary PostgreSQL instance.
Classifier and character inference were excluded; their production service remained active.

| Outcome | Articles |
| --- | ---: |
| Accepted article bodies | 73 |
| Poor extraction | 97 |
| HTTP rejection | 19 |
| Exhausted fetch retry | 2 |
| Failed source-document persistence | 1 |
| Total | 192 |

| Sport | Sampled | Accepted | Skipped | Failed |
| --- | ---: | ---: | ---: | ---: |
| Football | 72 | 30 | 41 | 1 |
| NBA | 54 | 16 | 38 | 0 |
| NFL | 66 | 27 | 39 | 0 |

The active intake period lasted **767.74 seconds (12m48s)**. The cohort's final retry
settled after **990.53 seconds (16m31s)**. Two articles used the deferred fetch retry:
an HTTP 503 response and an HTTP 429 response. The latter's five-minute delay produced
the final idle tail. Accepted yield was 73/192, **38.0%**, under this extraction policy.

For successful acquisitions, median fetching/resolution time was **15,922 ms** and median
extraction time **42 ms**. Fetch time includes shared domain-budget waiting. These medians
describe accepted articles, not every failure or an additive breakdown of total wall time.

## What limits the window

Each new Google wrapper requires a wrapper GET and resolver POST, serialized under the
same domain budget. Two requests at two-second spacing give an approximate ceiling of
**900 new links/hour**, before publisher and database overhead. Even perfect extraction
would examine only about 1,800 new links in two hours, 21.2% of 8,500. Fast terminal skips
save retries and downstream work, but usually happen after that resolution cost.

More extraction CPU or more publisher workers cannot remove this Google ceiling. The next
capacity experiment should reduce resolutions before network work using existing RSS rank,
or test discovery that supplies direct publisher article URLs. Better extraction can improve
accepted yield within that ceiling. Neither change's coverage or throughput is qualified here.

## Failure found and corrected

One publisher returned NUL-bearing content. The original fetcher tried to retain it in a
PostgreSQL text column, failed five times and dead-lettered the article. Those attempts
remain in the baseline evidence. After all other cohort work settled, the original benchmark
process was stopped rather than waiting on the parked dead-letter.

The shared fetcher now rejects that response before persistence with a typed
`UnsupportedBody` outcome. Classifier records a terminal `unsupported_response` skip;
real database failures retain normal queue recovery. The same live URL then skipped in
0.21 seconds in a separate one-article probe, without an infrastructure failure. A local
HTTP/database regression fixture also passes. The full cohort was not rerun after this fix.

Production acquisition was restored and both production services verified active. No new
acquisition binary or migration was deployed to production. The temporary test servers were
stopped after verification.

Validation: 388 runnable library tests and 17 binary tests passed; the database-backed
acquisition regression passed. The replay asserts its cohort accounting. Accepted extraction
does not establish complete reporting, unique-story coverage, native Classifier token fit or
character usefulness. There was no independent coverage review or multi-day throughput run.

The [aggregate receipt](../fixtures/classifier/acquisition-benchmark-2026-10-15.json) records
exact timing, selection, outcomes and code hashes. The [replay driver](../examples/acquisition_bench.rs)
refuses databases outside the `classifier_bench_*` namespace. Private source snapshots and
HTTP/queue receipts remain at Archbox
`/mnt/data/backup/scoracle/acquisition-benchmark/2026-10-16-run1`.
