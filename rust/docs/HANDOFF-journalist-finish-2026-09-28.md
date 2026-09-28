# Finish Journalist from the n47 checkpoint

Checkpoint date: September 28, 2026. **Window 2 remains open.** Preparation, memory
serving and file ownership are implemented; model fidelity remains unresolved.
This document is a continuation guide under the
[plugin alignment plan](PLAN-plugin-alignment-2026-09-27.md), not another plan.

## Read first

1. The plan's ownership contract and current Window 2 section.
2. [README](../README.md): file ownership and the shared memory contract.
3. [Current context audit and exact results](journalist-context-trim-2026-09-28.md).
4. [Memory study implementation and limits](memory-studies.md).

The older [Journalist history](journalist-alignment-2026-09-27.md),
[thinking experiment](journalist-thinking-2026-09-28.md) and
[live database inspection](journalist-memory-world-2026-09-28.md) are supporting
evidence. Their early implementation descriptions have been superseded.

## User decisions to preserve

**Plugins prepare and govern WHAT. Laya scores. SmolLM3 articulates HOW.**
Harvester v7 and shared entity metadata are the accepted upstream baseline.
Calibration is accepted for now; do not reopen it to pursue Journalist failures.

Each articulation call is **stateless and memory-informed**. The plugin supplies
the current reporting world, including studied history. The model does not need
to evolve a story, fill a news article, invent significance or reconstruct history.
It does not retrieve, calculate, prepare claims, fact-check or decide validity.
Natural prose is required; a fixed phrase palette is not the target.

Each file owns one task:

- `meta.rs`: canonical identity; internal IDs stay outside the writing view.
- `fresh.rs`: complete newly fetched reporting, publisher and publication time.
- `memories.rs`: requested historical scope, selection and presentation.
- `journalist.rs`: descriptive tone only.
- `form.rs`: structure only—fields, types, counts, size limits, schema and parser.
  No content directions or editorial obligations.
- `journalist/cognition/prompt.rs`: the sole task instruction and output mapping.

The system message is the task constant. The data package has `identity`, `fresh`,
`memories`, `voice`, and `form`. Source text remains quoted data. There is no model
conversation history. Publication deduplication and the previous numeric score
are separate bookkeeping, not historical prose injected into the call.

## Current code and runtime

- `src/plugins/journalist/cognition/mod.rs`: n47, output contract
  `narratives-v7-studied-memory`; fresh presentation `journalist-fresh-v4`.
- The adapter consumes Harvester source receipts. Preparation selects up to three
  complete fresh reports from a 72-hour horizon, with exact-source deduplication,
  eligibility decisions and budgets before inference.
- Memory requests cover 30 days before the oldest selected fresh report, with up
  to three frequency-ranked groups. Whole findings fit a 2,200-byte allowance;
  fresh context has 6,000 bytes and the combined frame 8,200 bytes.
- Identical memory headlines share text while retaining every publisher/date pair.
  Study windows, populations and publisher counts remain visible. IDs, hashes and
  complete study receipts stay in provenance; memory does not inflate fresh scores
  or source metadata.
- `form.rs` owns the output shape: headline plus ordered title/body narratives.
  Headline/title ceilings are 140 characters; bodies share 1,200 characters.
- One articulation call, temperature 0.3, context 4,096, output allowance 900 tokens,
  no corrective retry. The replay explicitly uses `think:false`. Verify actual
  environment routing before making production claims; no production config changed.
- Shared instructions still needed by other plugins moved from `support/form.rs`
  into `support/prompt.rs`. Journalist does not use that legacy composition path.
  Other character prompts/described schemas matched an exact before/after snapshot.
- Publication revalidates locked source receipts and retains atomic disposition,
  product/source links and completion intent. Preserve these fences during cleanup.

**Postgres stores the world; DuckDB studies the world.** Rust's
`src/evidence/memory_studies` reads bounded, read-only repeatable-read snapshots.
`go/cmd/memory-study` invokes the existing Go DuckDB package over JSON stdin/stdout.
There is no new memory database, precompute job or LLM retrieval stage. The shared
contract supports entity/pair reporting frequency and compatible team-stat windows.
Other plugins choose their own scope/data through that same boundary.

Known limits: legacy Graph bindings and storyline groups can be wrong; membership
is not event identity. Headline/name matching narrows the measured population but
does not establish semantic truth. Recorded frequency is not independent
confirmation. Missing statistics stay missing; thin samples stay visible. Person
reporting is unsupported until its ID namespaces are reconciled. Other character
runtime selectors have not been migrated.

## Evidence and unfinished behavior

Latest Rust verification: **551 passed, 77 environment-dependent tests ignored**;
all targets compile; formatting/whitespace checks pass. Go study tests, the isolated
cross-language memory test and six isolated Journalist publication tests passed
during implementation. Do not run database-writing tests against Archbox.

The final n47 replay used eleven synthetic development cases plus one memory case:
three correct no-call decisions and **9/9 completed, structurally accepted calls**,
mean local call time **3.54 seconds**. The memory case preserved the old/new training
times, venue and absence of an explanation. Its combined instruction/data package
is 1,505 bytes versus n43's 1,949 (22.8% smaller; this is not a token measurement).

Still unresolved:

- Unsupported commentary/details, including in otherwise accurate match reports.
- Rumour qualifications lost in headlines/titles while retained in the body.
- Multiple reports reordered/mixed despite positional source-ID mapping.
- Following instructions embedded in a source excerpt.
- Stronger demonstrations of nonredundant history, counts and statistical context.

See `fixtures/journalist/context-n47-development.jsonl` and
`memory-articulation-n47.jsonl`, with the case-by-case review in the context audit.
Parser acceptance is not fidelity. The source-instruction case is deliberately
synthetic; do not mistake its fabricated result for supplied reporting.

Thinking was tried and rejected: substantially higher cost without a demonstrated
fidelity benefit. The installed model's template has a suspected system-turn
closing-marker issue. A small n46 comparison with explicit native role framing
still added unsupported facts on both routes; the cause is **not established**.
No provider, template, installed model or production route was changed. The
diagnostic is retained in `context-n46-transport-probe.jsonl`.

## Resume and verify

Start with the exact current request and failing output, not another generic
prompt rewrite. Investigate preparation, data presentation, task/structure mapping
and transport with bounded tests. Keep instructions in prompt.rs and supporting
files informational/structural. Do not add an accumulating prompt/guard/eval stack,
generative claim-preparation stage, mandatory model judge, blanket retries or larger
output allowance to mask the failures. Do not silently restore the fixed palette.

Use the existing replay, which shares production preparation/provider/parser:

```sh
cargo run --example journalist_replay -- \
  fixtures/journalist/development.jsonl /tmp/journalist-next-development.jsonl \
  http://localhost:11434 false
cargo run --example journalist_replay -- \
  fixtures/journalist/memory-articulation-case.jsonl /tmp/journalist-next-memory.jsonl \
  http://localhost:11434 false
```

Choose unused output filenames; the replay intentionally refuses overwrite. Without
an endpoint it prepares requests offline. Local networking may need sandbox approval.
Keep exact requests and failure records, and manually compare all headline/title/body
content with the supplied reporting. Do not publish test prose.

Build the shared helper from `go/` if memory tests need it:

```sh
go build -o /private/tmp/scoracle-memory-study ./cmd/memory-study
go test ./internal/analytics/duckdb ./cmd/memory-study -count=1
```

Set `SCORACLE_MEMORY_STUDY_BIN` to that binary for local integration tests. The
earlier disposable Postgres cluster used port 56487, database `postgres`, user
`scotty`; verify that it is still the isolated test cluster before using it.
Archbox is available over SSH/LAN for **read-only** source inspection. The earlier
port-56488 live tunnel was closed; do not assume it exists. `examples/memory_request.rs`
uses `SCORACLE_MEMORY_DATABASE_URL` and enforces a read-only connection. Access and
measurement details are in the memory documents; do not rediscover the whole database.

Finish Journalist and necessary callers only. Preserve the accepted Harvester
handoff and shared memory infrastructure. Update the plan and README with measured
results and honest remaining limits. Move to Influencer only after Window 2's exit
criteria are supported. **Nothing in this checkpoint was deployed or pushed.**
