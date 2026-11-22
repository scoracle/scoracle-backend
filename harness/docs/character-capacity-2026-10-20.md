# Character capacity and 10k ceiling: 20 October 2026

The production configuration uses a 10,000-token request ceiling, including
reserved output. Four Qwen3.5-4B Q4_K_M slots have 10,240 physical tokens each.
The server's 40,960-token allocation is shared across four concurrent requests;
it is not a 40k prompt. Actual short packets remain short. Thinking and context
shifting stay disabled; source completeness, exact token admission and output
validation remain active. The 24 KB reporting-packet bound remains unchanged.

The capacity change initially retained the guard-fix runtime. The memory cleanup
below is now deployed as runtime `31d4ba335e5a`, with the same model configuration.
`VOICE_NUM_CTX=10000` explicitly overrides the safe 7,900-token code default.
The regression check verifies that raising only the logical cap still fails on
an 8,192-token physical slot, and that both bounds must admit the whole packet.

## Nightly target and current schedule

Target about four hours of character work, after the midnight America/Detroit
cutoff and at most 15 minutes of acquisition/classification drain. That aims for
fresh coverage around 04:00–04:15 local time. Keep the existing six-hour safety
allowance while qualifying real completion; shortening a deadline would expire
unfinished coverage rather than accelerate it. Coverage resumes when the frozen
edition and its handoffs finish, or the existing deadline fences a partial run.
Daytime acquisition and classification remain managed continuously on Archbox.
Existing served products remain available during the overnight run.

## Native hardware check

Two uncached four-request batches per configuration reused the retained Chelsea
Journalist, Influencer, Scout and Insider packets. Each batch exercised all four
slots. These historical packets test transport and hardware; their prose is not
being requalified or published.

| Logical cap | Physical slot | Batch 1 | Batch 2 | Mean |
| --- | --- | --- | --- | --- |
| 7,900 | 8,192 | 39.591 s | 41.426 s | 40.508 s |
| 10,000 | 10,240 | 40.290 s | 39.414 s | 39.852 s |

The small difference does not establish a speed improvement. All 16 responses
were complete, untruncated JSON with exact input token counts. Swap usage stayed
at 2,997.94 MB during both trials, with zero swap-out events. System memory free
percentage went from approximately 53% before the smaller configuration to 50%
after the larger trial; this is a short hardware check, not sustained-load proof.

The retained synthetic 80-report packet contains 6,310 input tokens and reserves
2,100 output tokens: 8,410 total. Exact template/tokenization confirms it fits the
new bounds, while the old logical limit would hold it. This admission check made
no inference call and does not override current canonical identity validation.

## Four-hour capacity limit

The workload snapshot had 14,600 pending/failed character queue rows and 1,142
subjects with pending reporting published within the previous 24 hours. Queue
rows are not model calls: empty evidence, missing measurements, abstention and
missing-peer paths can finish without inference. The cutoff may also add missing
peer-stage assignments. Daytime arrivals and canonical reconciliation change
these counts, so they are not tonight's frozen call count.

At the measured reporting-heavy batch mix, four hours holds about 361 batches,
or 1,445 inference calls before preparation, parsing, retries and handoffs.
Continuous slot refill and shorter typical packets may improve on this closed
batch estimate; larger packets and corrections can worsen it. It is not an SLA
or a forecast of tonight's whole edition. Historical ledger timings are from
SmolLM3 and cannot qualify this newly pinned Qwen configuration.

A four-hour completion claim requires observing the frozen assignment population,
actual inference/no-call counts, packet sizes, retry rate, output tokens and elapsed
edition time. Extra context reduces some holds; it does not solve an excessive
number of model calls. Do not silently drop evidence or force partial completion
to meet the clock.

The subsequent [Premier League creator replay](../fixtures/character-runs/premier-league-2026-10-20/README.md)
attempted six stages for 20 clubs and 444 current-season players in 63m 48s of
active replay time, or 65m 25s including a sampler interruption and recovery.
It produced 657 generated stage products and 116 failed stages. Scout made 657
native calls for 332 invoked stages. The largest request including reserved output
was 6,895 tokens, so context capacity did not limit this sample. This read-only,
two-pass test excludes queue publication/backoff and does not qualify the entire
nightly population or the factual accuracy of accepted prose.

## Optimization handoff for the next context

The working goal remains roughly four hours for daily character coverage, with
continuous daytime fetching and overnight character work. The league result is
an encouraging starting point. Next, improve first-attempt validity and avoid
repeating unchanged work; measure a full frozen edition before treating four
hours as established capacity. This handoff records proposals, not deployed fixes.

Start with the [retained benchmark and limitations](../fixtures/character-runs/premier-league-2026-10-20/README.md)
and its [machine-readable totals](../fixtures/character-runs/premier-league-2026-10-20/summary.json).
Benchmark artifacts were committed in `2a5814ab`; production runtime remains
`3df810c5189c`, with the 10k configuration from `d93c15d7`. The benchmark was
read-only and published nothing. Fetching was restored; API, classifier and the
Mac worker were healthy afterward. No optimization is running in the background.

### Levers, in priority order

1. **Make Scout succeed on its first attempt.** It used 657 responses for 332
   invoked stages: 325 extra calls, about 28% of all final-result responses.
   Its 37 final rejections include 20 unsupported stability/direction claims and
   15 unqualified appearance-count claims. Inspect actual first replies and
   corrections before changing the prompt. Try a shorter, concrete thin-sample
   instruction/example: recorded source coverage is not complete participation;
   absent comparison is unknown, not stability. Check whether correction wording
   identifies the rejected claim clearly enough for this small model. Preserve
   the qualifications and null/abstain path. Entry points:
   [Scout prompt](../src/plugins/scout/prompt.rs),
   [parser](../src/plugins/scout/parser.rs),
   [regressions](../src/plugins/scout/tests.rs), and
   [shared three-attempt extraction](../src/harness/session.rs).
   The 325 calls are an opportunity bound, not a promised 28% wall-time saving.

2. **Diagnose Journalist rejections against the exact cited reports.** There were
   68 output rejections across 104 native responses, plus one pre-inference
   changed-source failure. Separate invented identities, dates and merged facts
   from demonstrated alias, capitalization or claim-boundary false positives.
   Publication time does not establish an event date. Tighten prompts/citations
   for real errors; change matching only with a positive and negative regression
   demonstrating the same fact remains supported. Do not globally loosen name or
   number guards. Inspect [Journalist preparation](../src/plugins/journalist/fresh.rs),
   [prompt](../src/plugins/journalist/prompt.rs),
   [parser](../src/plugins/journalist/parser.rs), and the shared
   [named-fact guard](../src/tools/fresh.rs). Insider's ten rejections similarly
   require an exact quote naming both parties and stating the move or denial;
   [its parser](../src/plugins/insider/parser.rs) must retain those checks.

3. **Treat local correction and durable retry as separate budgets.** Scout can
   make up to three local extraction attempts; the worker permits five failed
   queue executions. Repeated local failure can therefore amplify to 15 calls
   for one unchanged assignment. The current queue ladder waits 30s, 2m, 10m,
   then 30m: 42m 30s before the fifth execution, excluding generation time.
   This queue tail was not measured by the replay. Classify transport/transient,
   stale-source and deterministic evidence failures before proposing a retry
   policy. Consider bounded, specific repair only where replay demonstrates it
   helps; unchanged unsupported claims should not receive blind repeated calls.
   Keep failures visible and preserve new-input/next-edition recovery. Do not
   simply add retries, shorten all delays or lower the global attempt limit.
   Inspect [session corrections](../src/harness/session.rs),
   [worker failure handling](../src/harness/queue/worker.rs),
   [retry policy](../src/harness/queue/work.rs), and coverage edition tests.

4. **Verify existing reuse and dependency gates before adding caching.** The
   fresh replay deliberately bypassed product reuse; production already has input
   hashes/debounce in [Scout](../src/plugins/scout/mod.rs),
   [Analyst](../src/plugins/analyst/mod.rs) and
   [Oracle](../src/plugins/oracle/mod.rs), plus Journalist's unchanged-input path.
   Measure their real hit rate and avoid regenerating downstream prose when its
   meaningful peer inputs have not changed. Oracle made 293 calls, about 26% of
   final-result calls, so inspect its input hashing and settled-pillar gate.
   Preserve current-edition provenance, source completeness, readiness and valid
   partial-peer behavior. The 1,830 no-call stages, 132 no-stats stages and 49
   abstentions are distinct outcomes, not automatic reasons to call again.

5. **Reduce generated verbosity before increasing hardware pressure.** Trial
   shorter complete prose and simpler evidence-bound output where quality allows;
   retain required qualifications and exact quotes. Measure actual output tokens
   and truncation/correction rates: lowering reserved output alone can cause more
   retries. Keep all eligible evidence and the admission guards; do not meet the
   clock by dropping reports. The largest observed request was only 6,895 tokens
   including reserved output, so a larger context is not the first speed lever.
   Retain one pinned Qwen and four slots initially. Use the slot trace to measure
   idle time and queue/model wait before tuning concurrency or stage claim caps.
   Preparation took only 42.94s; database preparation is a lower priority than
   inference/rework in this sample.

### Smallest useful next experiment

Replay representative retained Scout failures and successful controls first,
then Journalist rejection examples. Compare the same prepared evidence and keep
the model, context and concurrency fixed while changing one lever. Record first
attempt acceptance, corrections per invoked stage, final grounded acceptance,
abstention, error category, native input/output tokens and elapsed time. Manually
check a sample of accepted prose; valid JSON and parser success are not accuracy.
Add focused regressions for each proven parser bug before changing a shared guard.

Then repeat the whole league and measure an actual frozen nightly population,
including product reuse, queue retries/backoff, publication and handoff drain.
Count scheduled retries, running claims and visible terminal failures when deciding
an edition is finished. Preserve the six-hour safety allowance until the full
edition establishes the four-hour target without silently expiring coverage.

The benchmark controller's partial-JSONL race was repaired for the twelve-player
recovery pass. Reuse the newline-complete sampler and separate observer failures
from worker lifetime in the next test; do not accidentally repeat the sampler
that stopped its own child. Original/recovery receipts, controller scripts, slot
samples and final health checks remain in
`/Users/scotty/scoracle-worker/releases/premier-league-test-20261020/`.
The recorded timing includes recovery; up to four canceled in-flight attempts
lack receipts. Publication normalization and semantic quality were not qualified.
Do not revive the rejected production-clone/worker-stop plan merely to benchmark;
use the retained read-only inputs for tuning first.

## Memory cleanup and production activation

The [memory contract](memory-studies.md) now records the intended boundary:
Postgres stores durable data, DuckDB analyzes source snapshots for plugins,
and plugins publish products back to Postgres for the API. Memories remain
compact dated context, including reported stories, emotions, movement and injuries.
The active DuckDB headline and cohort calculations are retained.

The cleanup removes unused historical memory-study modes and their Rust
adapters, skips Journalist history loading when no fresh reports are usable,
and fixes Articulator's stale arguments to the daily news/vibe/transfer queries.
Rust library/binary tests, seven classifier/memory integration tests, Go
API/database/DuckDB checks, all-target Rust checking and Go package builds passed.
Before/after helper comparison preserved all three character rankings on the
same bounded sample. This is not model-quality or nightly throughput qualification.
Runtime commit `31d4ba335e5af1315a952d79d28fa477b642fc86` was pushed to main
and deployed on 20 October at 22:07 UTC. Archbox's API, acquisition binaries,
Classifier, batch binaries and memory helper were built from that commit; the
managed Mac character worker and its bundled helper match. DuckDB remains on
engine 1.5.5 through Go driver v2.10505.0. No schema or model configuration changed.

Post-deployment checks passed:

- [CI for the deployed commit](https://github.com/scoracle/scoracle-backend/actions/runs/39505504829):
  runtime/database contracts, schema checks and shell checks all passed.
- Local and public API/CORS smoke: 18/18 each.
- All eight Articulator slices for both a team and player: 16/16 valid responses.
- Both helper startup probes returned the expected nonempty ranking; the managed
  Mac worker logged successful helper verification before starting its queue.
- Both workers and the API reported the deployed commit. All six Mac character
  routes retained Qwen3.5-4B, the 10,000-token cap and four 10,240-token slots.
- DuckDB cohort refresh completed with 50 unchanged cohorts. Coverage retained
  the midnight Detroit cutoff, 15-minute drain and six-hour safety limit, with a
  fresh controller heartbeat and no long lock waiters. Classifier shutdown left
  no captured claims requiring recovery.

Private binary backups, unchanged environments, prior service states, hashes and
verification receipts are retained in `lean-memory-20261020-31d4ba33` under
`/Users/scotty/scoracle-worker/releases/` and Archbox's
`/mnt/data/backup/scoracle/releases/`. To roll back this cleanup, stop the managed
workers and binary watchers, restore the saved Archbox binaries and Mac launcher,
then restore only previously active services/watchers. The prior Mac release is
retained intact. No database or model rollback is needed for this cleanup.

## Earlier capacity trial and recovery

Private trial and rollback receipts are retained at
`/Users/scotty/scoracle-worker/releases/qwen-10k-20261020-922e3b22/`.
They include the previous worker environment and model agent, candidate files,
both benchmark rounds and raw native replies, slot traces, memory counters and
the exact-token stress receipt. The managed worker stays paused during model
reconfiguration and starts only after four-slot checks pass. No second queue
worker or test publication is used.

Rollback pauses the managed character worker, restores its saved environment and
Qwen agent, restarts the four 8,192-token model slots and verifies health, then
starts the worker. No schema migration or Rust binary replacement is required.
