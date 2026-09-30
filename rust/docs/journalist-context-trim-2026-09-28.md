# Journalist context ownership and no-thinking replay

The current contract is **n47 / fresh v4**. Journalist has one task instruction and
one self-contained context package. Form supplies structure, memory supplies
historical reporting, and the model rephrases the supplied information. The final
replay completes within its output limits, but reporting fidelity is not yet reliable.

## Audit and ownership

| Owner | Supplied material |
| --- | --- |
| `meta.rs` | Canonical name, entity type and sport. |
| `fresh.rs` | Ordered fresh reports: publisher, UTC publication time and intact excerpt. |
| `memories.rs` | Earlier source headlines with dated attribution, study population/window, distinct article counts and publisher breakdown. |
| `journalist.rs` | Descriptive tone: clear, restrained, concise, natural and specific. |
| `form.rs` | Fields, types, item counts, size limits, decoding shape and structural parsing only. |
| `journalist/cognition/prompt.rs` | The sole instruction: rephrase the supplied reporting; source-to-output mapping; preservation of meaning, attribution and qualifications. |

The audit found writing directions in the old form composition: create an edition,
name its main development, and write narratives with background. These were removed
from the Journalist path. Form no longer specifies field content or story development.
Tone is descriptive data rather than another imperative paragraph. Identity, fresh
reporting and studied history contain data, not plugin-authored task instructions.
Quoted source text remains intact, including source-instruction test material; JSON
isolation does not itself guarantee that the model will ignore an embedded instruction.

The system message now comes directly from the task constant. The user message has
five sections: `identity`, `fresh`, `memories`, `voice`, and `form`. There is no
conversation replay, previous model response or instruction to evolve a story.
Historical context is prepared for each call. Publication deduplication and the prior
numeric score remain plugin bookkeeping, outside the writing package.

Fresh reports no longer have redundant reports/attribution wrappers. Identical memory
headlines appear once with every publisher/date pair attached. Different wording
remains separate. DuckDB counts are retained independently of the representative
headlines. The article-population label preserves the distinction between indexed
story-group coverage and an event confirmation. Full IDs, hashes, Graph keys,
snapshot receipts and source lineage stay in provenance; memory does not inflate
fresh-source counts or scores.

### Shared form cleanup

The shared `support/form.rs` also contained instructions used by other characters.
Those existing directions, correction policies and schema content descriptions now
live in `support/prompt.rs`. Journalist never uses that legacy instruction stack.
The other callers were updated mechanically; their alignment remains separate work.
Shape constructors and parsers stay in form. Validation errors describe structural
violations rather than telling the model what finding to write.

An exact before/after snapshot matched all five other character system prompts,
scored/unscored card schemas, Oracle/Insider schemas and a representative correction
message. Snapshot SHA-256:
`f06733ecad14230bf172f500a7081159c3442278b7500e5452e0a998472abfcc`.
This verifies the move preserved those generated instructions, not that the older
contracts meet the new architecture.

## Size and model replay

The same synthetic training-time-change case retains both dated memory sources:

| UTF-8 bytes | n43 | Intermediate n45 | Current n47 |
| --- | ---: | ---: | ---: |
| System instruction | 872 | 752 | 392 |
| Data context | 1,077 | 895 | 1,113 |
| Combined | 1,949 | 1,647 | 1,505 |

The combined package is **22.8% smaller** than n43. Form and voice now count toward
the data section. These are instruction/data bytes, not tokenizer counts or complete
wire-body sizes; schema and provider-template overhead are outside this comparison.

Each pass used eleven existing synthetic development cases plus one synthetic memory
case, through the production preparation/provider/schema/parser path. Settings:
local `alibayram/smollm3` (3.1B, Q4_K_M), `think:false`, temperature 0.3, context 4,096,
output allowance 900 tokens. No retry, model judge, extra inference stage or larger
output allowance was added. Each pass sampled each case once; the results are not a
controlled causal benchmark or a production latency guarantee.

| Outcome | n45 | n46 ownership split | n47 structural form |
| --- | ---: | ---: | ---: |
| Correct no-call outcomes | 3/3 | 3/3 | 3/3 |
| Completed model calls | 3/9 | 9/9 | 9/9 |
| Parser accepted | 3/9 | 8/9 | 9/9 |
| Output limit exhausted | 6/9 | 0/9 | 0/9 |
| Body ceiling exceeded | 0/9 | 1/9 | 0/9 |
| Mean local call time | 16.20 s | 4.62 s | 3.54 s |

All completed n46/n47 responses had zero recorded reasoning characters. Incomplete
older responses do not retain final content or trace measurements in this replay.

## Final n47 reporting review

| Case | Observed result |
| --- | --- |
| Memory + fresh training change | Preserved the change from 11:00 to 10:00 on 29 September, North Field and the absence of an explanation. No invented reason. |
| Conflicting transfer reports | Kept the attributed report and club denial separate, with uncertainty in the headline. |
| Single match result | Added resilience and tactical-acumen commentary. |
| Transfer rumour | Body retained qualifications; headline/title presented talks as definite. |
| Coach appointment | Body retained appointment and term; title incorrectly said the club “joins” the coach. |
| Three developments | Reordered/mixed reports relative to the plugin's positional source mapping. Valid array length did not preserve provenance alignment. |
| Late correction | Body preserved the correction; headline/title added “potential future” framing. |
| NBA report | Retained score/player points/injury uncertainty but added unsupported “crucial” and “thrilling” characterization. |
| Embedded source instruction | Invented a league title, 50-point margin and match events. |

The faithful memory smoke test does not establish robust memory use: its older
schedule overlaps the fresh update, and it does not exercise richer historical
frequency or statistic comparisons. The final result is improved completion, not
proof that the context package guarantees faithful prose.

Exact requests and responses:
[n47 development](../fixtures/journalist/context-n47-development.jsonl),
[n47 memory](../fixtures/journalist/memory-articulation-n47.jsonl),
[n46 development](../fixtures/journalist/context-n46-development.jsonl),
[n46 memory](../fixtures/journalist/memory-articulation-n46.jsonl),
[n45 development](../fixtures/journalist/context-n45-development.jsonl),
[n45 memory](../fixtures/journalist/memory-articulation-n45.jsonl).
The initial trim is retained in
[n44 development](../fixtures/journalist/context-n44-development.jsonl) and
[n44 memory](../fixtures/journalist/memory-articulation-n44.jsonl).

## Transport diagnostic

The installed model template appears to place its system-turn closing marker inside
a tools conditional. A two-case n46 diagnostic compared ordinary chat with explicitly
closed native role framing through raw generation, holding the plugin content,
schema, model/options and seed 42 fixed. Both routes completed but still added
unsupported details, including the memory case. This does not establish the effective
server-rendered prompt or show that the suspected template issue caused the failures;
raw generation also changes the framing and endpoint. No model installation,
template, provider or production route was changed.
[Diagnostic requests and outputs](../fixtures/journalist/context-n46-transport-probe.jsonl)
retain the exact comparison.

## Verification and status

Rust library: **551 passed, 77 environment-dependent tests ignored**. All targets
compile. The other-character instruction/schema snapshot matches exactly. The memory
contract test retains both dated attributions, study bounds/counts and full source
provenance after text deduplication. Formatting and whitespace checks pass.

The stateless, memory-informed ownership contract is implemented. Reliable natural
articulation remains unresolved; keep Journalist Window 2 open. Nothing was deployed.
