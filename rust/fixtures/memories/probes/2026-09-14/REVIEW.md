# First memory generation test: Morgan Rogers

September 14, 2026. Four first-pass offline Scout calls; no production writes,
queue work, route changes or surface retries. All four outputs are rejected for
editorial/factual reasons. Two full-memory outputs passing the parser is not a
release qualification.

## Experiment

The same current evidence, verified identity, character and form appear in both
conditions. `full_memories` retains the historical performance group;
`without_performance_baseline` removes that group only. The rest of the package,
including references to Villa history in identity qualifications, remains fixed.
This is a narrow ablation of historical measurements, not an old-pipeline versus
new-pipeline comparison or an entirely memory-free control.

Runtime was read from archbox's current environment and `/api/ps` before the
calls: `granite4.2:3b`, Q4_K_M, digest
`7a65e6414ad66ecf95515ba9292530a168d6f97860044ada11809795dca5bdde`.
Ollama `/api/chat`, thinking false, temperature 0.6, context 4096, output reservation
700. Seeds 17 and 43 were the only added diagnostic settings. Calls ran sequentially
against the already-loaded local model. Wall times include HTTP/runtime overhead;
prompt caching and other host activity make these exploratory timings, not a
throughput benchmark.

The request builder uses the real `OllamaClient::request_body`, shared Scout JSON
schema, and `composition::compose_card`. Raw responses retain provider token and
completion metadata. Local review uses the production `RatingParser` plus an
explicit complete/stop check. There was no automatic prose correction or judge.

| Condition | Seed | Prompt tokens | Output tokens | Wall seconds | Body characters | Parser | Editorial review |
|---|---:|---:|---:|---:|---:|---|---|
| Full memories | 17 | 1,986 | 166 | 4.842 | 682 | Pass | Reject |
| Baseline removed | 17 | 1,738 | 354 | 6.572 | 1,741 | Fail: over surface | Reject |
| Full memories | 43 | 1,986 | 153 | 3.377 | 589 | Pass | Reject |
| Baseline removed | 43 | 1,738 | 137 | 2.788 | 606 | Pass | Reject |

All calls completed with `done=true`, `done_reason=stop`. The historical group costs
248 prompt tokens in this runtime. Full context plus the reserved 700 output tokens
uses 2,686 of the 4,096-token envelope, leaving 1,410 nominal tokens. That does not
qualify an additional RSS corpus or another junction's prompt.

## Evidence-linked findings

**What worked:** all four outputs identify Chelsea. Both full-memory outputs use
Rogers' prior 37 appearances, 3,285 minutes, ten goals, six assists and 32 shots on
target. The older numerical baseline survives the current one-appearance sample.
Neither full-memory output exceeds the surface. These are useful observations,
not evidence that every claim is sound or that memory caused all improvements.

**Full memories, seed 17:** describes the historical production as performance
"against Aston Villa". The source's team_id=15 identifies Villa as the team Rogers
played for, not his opponent. It also frames a stored sample as his current total
appearances, and suggests evaluating relative to other players without supplied
ranks. The hook is a topic label rather than a finding. Much of the body narrates
how to use the baseline instead of interpreting the football.

**Full memories, seed 43:** correctly connects the old output to Villa, then claims
his current form is "fragile due to limited playing time". Source coverage does not
establish fragility, actual restricted participation or its cause. "Participation
clock" also leaks an internal context label into the card.

**Baseline removed, seed 17:** invents a September signing date and places the
appearance in reporting week 7. Neither follows from the source. The package
explicitly distinguishes ingestion timestamps from signing dates and reporting
weeks from competition/participation. It additionally invents consistent/modest
production and tactical implications, then exceeds the character ceiling.

**Baseline removed, seed 43:** infers transfer timing "as indicated by the clock"
and calls Rogers a developing player with limited exposure. A thin local data
sample does not establish limited career experience. The missing old production
leaves the model supplying an unsupported profile.

## What to change next

The next iteration should change the context presentation while keeping the same
voice/form/model:

1. Give historical statistics an explicit named affiliation (`played for Aston
   Villa`) instead of relying on team IDs and a separate identity-conflict block.
2. Make the measurement window and known coverage part of each statistical record.
   `recorded appearances in this source snapshot` must remain distinguishable
   from the player's actual season appearance total. Do not add a speculative
   explanation for the difference.
3. Keep database reconciliation details and schema/clock vocabulary in the audit
   artifact. The character should receive compact sporting facts with dates,
   source references and uncertainty, rather than reproduce the assembly labels.
4. Repeat the matched test and inspect every relational, temporal and causal claim.
   Do not add a regex for "fragile", tune voice, or mark a schema-valid card good.

Iraola and the Lions have not been generation-tested here. Their downstream input
adapters need to preserve the real Insider/Journalist task contracts (especially
numbered article references and activity inputs) before treating their standalone
memory packages as complete writer requests.

## Artifacts and reproduction

- `requests.json`: exact four wire bodies, conditions, seeds and memory fingerprints.
- `responses.jsonl`: unmodified response bodies and per-call wall timing.
- `summary.json`: parser/completion results and extracted card text.
- `../../packages-2026-09-14.json`: original three entity packages.

```sh
cargo run --example memory_probe > /tmp/memory-probe-requests.json
# Submit those requests sequentially to the recorded local Ollama /api/chat
# endpoint, preserving each raw response; do not run the production queue.
cargo run --example memory_probe -- --review \
  fixtures/memories/probes/2026-09-14/responses.jsonl
```

No broad model-quality conclusion follows from four samples on one entity. These
are rejected counterexamples for developing the memories product.
