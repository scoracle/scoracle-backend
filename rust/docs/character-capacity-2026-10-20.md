# Character capacity and 10k ceiling: 20 October 2026

The production configuration uses a 10,000-token request ceiling, including
reserved output. Four Qwen3.5-4B Q4_K_M slots have 10,240 physical tokens each.
The server's 40,960-token allocation is shared across four concurrent requests;
it is not a 40k prompt. Actual short packets remain short. Thinking and context
shifting stay disabled; source completeness, exact token admission and output
validation remain active. The 24 KB reporting-packet bound remains unchanged.

The Rust runtime and memory helper are unchanged from the guard-fix release.
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

## Deployment and recovery

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
