# Harvester CPU, concurrency, and admission experiment

Historical experiment record. Its retired training, comparison, packet-archive and
alternative-adapter tools were removed during the September 27 cleanup. Commands
below document that experiment; they are not current operating instructions. See
[the cleanup handoff](harvester-cleanup-2026-09-27.md) for retained tools and owners.


The intended path is sweep → probabilistic entity/content admission and scope flags →
verbatim headline/opening extraction → character context. Characters make the final
editorial relevance decision. Extraction itself needs no generative model. The current
Rust implementation already copies bounded opening text and asks about news/narrative,
emotional material, transfers/organizational change, and performance/availability.
These scope scores are routing hints, not extracted facts or finished stories.

## Admission quality

An 0.80 score is a candidate cutoff, not a demonstrated 80% probability of relevance.
Replaying the saved English baseline and trained predictions with entity relevance
`P(subject) + P(opponent) >= 0.80` forwards **zero of 25** fully labeled test articles,
even without an additional reporting threshold. Eight articles were provisionally
useful. Applying 0.80 today would defeat the desired coverage goal.

The earlier 75% result was useful-news recall (6/8), not accuracy, and training did
not demonstrate a recall improvement. At that operating point the trained checkpoint
forwarded 13 articles: six useful and seven unwanted. These small, repeatedly inspected
provisional labels cannot establish general coverage or calibrated probabilities.
See [training findings](harvester-laya-training.md).

Prefer a recall-oriented intake threshold selected on calibration data, validated on
fresh labeled stories, and evaluated with forwarding cost. Retain separate entity,
content, and scope scores rather than multiplying them into an unvalidated joint
probability. Multiple scopes may apply. Scope quality needs its own labels; only entity
and content have been trained. Characters can reject surplus evidence but cannot recover
articles discarded upstream. Sample rejected material during shadow evaluation to
measure these misses. No production threshold was changed by this experiment.

## Throughput method

`examples/harvest_laya_bench.py` runs one shared Laya model with configurable CPU threads,
concurrent calls, and heterogeneous article batching. Each article retains its entity
question. Tokenizer preparation is locked; CPU forward calls may overlap. GPU calls
must be serialized: four concurrent MPS calls crashed Metal (exit 134), so the script
now rejects that configuration before loading the model.

Pinned English base checkpoint: `convaiinnovations/laya` revision
`55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851`, weight SHA-256
`891102d372688fc2a094dac56a384bc537b87c63f21f9f3dac0be2b7cbc8d86c`.
Both hosts use Laya 0.3.20, Transformers 5.17.0, and PyTorch 2.14.0 (CPU wheel on Linux).
CPU weights execute in float32; Mac MPS uses the SDK's float16 autocast.

The initial Mac runs use 24 real prepared articles; Archbox uses the first 12 of that
same corpus. Warmup and loading are excluded. Timings include tokenization and decoding,
but exclude fetch, HTTP, persistence, retries, queue coordination, and long-running
contention. These are classification capacity projections, not overnight SLA tests.
The benchmark checks input coverage, probability distributions, device fallback, and
agreement with four public-SDK reference predictions per question set. All completed
cases passed; MPS gate batching changed probabilities by at most 0.0019 with no reference
choice flips. Other completed cases had zero reference deltas at API precision.

Two workloads are intentionally separate:

- **Gate:** entity involvement and content category (two questions).
- **Full:** the existing seven-question Harvester contract, including scope and tone.

The gate-only benchmark cannot replace the seven-question adapter as-is. A staged
gate-then-scope design requires an explicit contract change and separate quality and
end-to-end timing checks. Gate-only throughput does not include the scope pass.

## Initial measurements

| Host / execution | Workload | Threads / calls / batch | Articles | Articles/s | 8,900 projection |
|---|---|---|---:|---:|---:|
| M4 CPU | Gate | 2 / 1 / 1 | 24 | 1.69 | 88 min |
| M4 CPU | Gate | 4 / 1 / 1 | 24 | 1.85 | 80 min |
| M4 CPU | Gate | 1 / 4 / 1 | 24 | 2.15 | 69 min |
| M4 CPU | Full | 4 / 1 / 1 | 24 | 0.67 | 221 min |
| M4 CPU | Full | 1 / 4 / 1 | 24 | 0.57 | 261 min |
| M4 MPS | Gate | 2 / 1 / 1 | 24 | 3.74 | 40 min |
| M4 MPS | Gate | 2 / 1 / 4 | 24 | 3.77 | 39 min |
| M4 MPS | Full | 2 / 1 / 1 | 24 | 1.03 | 145 min |
| M4 MPS | Full | 2 / 1 / 4 | 24 | 1.13 | 131 min |
| Archbox CPU | Gate | 2 / 1 / 1 | 12 | 0.36 | 412 min |
| Archbox CPU | Gate | 4 / 1 / 1 | 12 | 0.44 | 334 min |
| Archbox CPU | Gate | 1 / 4 / 1 | 12 | 0.53 | 280 min |
| Archbox CPU | Full | 2 / 1 / 1 | 12 | 0.11 | 1,398 min |
| Archbox CPU | Full | 4 / 1 / 1 | 12 | 0.12 | 1,231 min |
| Archbox CPU | Full | 1 / 4 / 1 | 12 | 0.17 | 896 min |
| M4 CPU confirmation | Gate | 4 / 1 / 1 | 120 | 2.21 | 67 min |
| M4 MPS confirmation | Full | 2 / 1 / 4 | 120 | 1.05 | 141 min |

The M4 has four performance and six efficiency CPU cores with 16 GiB memory. Archbox
has an i7-7700 (four physical cores, eight logical threads), 31 GiB memory, and was
running its normal production workload, with a busy GPU and occupied swap. Its numbers
describe available capacity under that load, not an isolated hardware comparison.
No Archbox GPU test displaced production. Process peak RSS was roughly 2–3 GiB;
this is not a complete GPU/unified-memory budget.

The larger CPU corpus contains shorter cases and a different length mix; its faster
average is not proof of a further optimization or sustained overnight capacity.
Four concurrent CPU calls helped the two-question workload but hurt the full Mac
workload relative to four-thread serial calls. GPU batching improved the full workload
modestly, not fourfold. Retain per-backend measured concurrency limits.

Both 120-article confirmation runs completed without errors. The full GPU projection
is **02:21 ET after a midnight start**, leaving only about nine minutes before 02:30
for overhead: feasible enough to test, not enough margin to promise. The full CPU
24-article projection is **03:41 ET**, offering a route toward the 04:30 goal while
leaving GPU compute available, although simultaneous character production was not tested.

The best measured four-call gate rates sum to 2.68 articles/s across the two hosts,
or about **55 minutes** for 8,900. This is an additive capacity estimate from separate
small runs, not a measured distributed eight-channel run, and excludes scope tagging.
For full seven-question work, the two four-call CPU rates sum to just 0.73 articles/s
(about 202 minutes). Eight CPU calls do not deliver the earlier hypothetical 18-minute
completion. The Mac's faster four-thread serial configuration plus Archbox's four-call
configuration would project about 177 minutes, also untested as a distributed pool.
For this hardware, start a shadow deployment with one shared Mac model and measured
resource limits. Use full seven-question CPU execution if 04:30 is sufficient; use
serialized MPS batching to explore the tighter window. Evaluate a two-stage gate/scope
optimization separately rather than presenting its cheaper first stage as the whole job.

## Shared work and scheduling

The durable queue already uses atomic claims with `FOR UPDATE SKIP LOCKED` and fenced
completion. Both hosts can safely claim distinct jobs from one Harvester stage. However,
Harvester is currently callable, not a deployed durable worker: admission, fenced
publication, and character-context delivery still need a production adapter.

The queue deliberately rotates the first claimant for fairness. Registering Harvester
first will not make it a strict fleet-wide priority. Existing four-slot Mac and four-slot
Archbox budgets describe generative backend capacity, not eight independent Laya engines.
Budgets are per process; multiple workers must not independently overbook one model server.
The existing Laya HTTP adapter serializes requests, so merely enqueueing eight calls
against it does not unlock concurrency measured by this experimental CPU runner.

If strict nightly priority is wanted, define a bounded sweep cohort and explicit
completion/retry policy, then release shared GPU resources to character production.
Continuous arrivals or a failed item must not hold a global phase open forever. With
CPU harvesting, characters may consume accepted contexts as they arrive while their
GPU is available, subject to shared CPU/memory capacity. A full fleet barrier is not
required for per-article gatekeeping.

At exactly one article per second, 8,900 articles take 2h28m20s; ideal eight-per-second
throughput would take 18m32.5s. Eight queued requests do not establish that rate.
Midnight projections assume all inputs are ready then; sweep duration and retries add
to completion time. No schedule, production service, or database admission was changed.

## Reproduction and artifacts

Run with the isolated local Python environment and prepared Rust request corpus:

```sh
python examples/harvest_laya_bench.py \
  --model-dir /path/to/pinned/english \
  --revision 55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851 \
  --prepared /path/to/prepared.jsonl --output /path/to/new-results.jsonl \
  --device cpu --cases 2/1/1,4/1/1,1/4/1 --limit 24
```

Case notation is CPU threads / concurrent calls / articles per batch. For MPS use
`--device mps --cases 2/1/1,2/1/4`. The script refuses to overwrite a result file.
Local raw evidence is under ignored `logs/harvester-throughput-20260924/`; the initial
Metal crash log is `logs/harvester-throughputput-mps.log`. Archbox's isolated runtime,
checkpoint, and results are under
`/mnt/data/backup/scoracle/experiments/harvester-cpu-20260924`.
