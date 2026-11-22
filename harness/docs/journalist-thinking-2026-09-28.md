# Journalist: SmolLM3 thinking experiment

> **2026-10-06 governing update:** This dated record preserves its implementation and evaluation evidence. The [current contract](../../../scoracle-wiki/wiki/Architecture/Harness,%20Plugins,%20Tools,%20and%20LLM%20Contract.md) supersedes articulation-only ownership: cheap code owns collection; expensive compute owns discovery. Plugins assemble traceable clues through tools; the LLM reasons across them, discovers what they support and articulates the answer. Historical “plugin owns WHAT/model owns HOW” statements below do not govern new builds.

## Follow-up: native reasoning activation repaired, not promoted

The later n94 assembly investigation found that the output schema was not the
reason reasoning stayed empty. Ollama's SmolLM3 renderer retains `Reasoning Mode:
/think`, but a custom system message replaces the model's default instruction to
emit reasoning inside `<think>` tags. The provider now adds one transport-owned
tagging cue only when `think:true`; it leaves `think:false` system content unchanged.

With the original n94 form, schema, prompt and strict parser restored, the full
11-case comparison produced seven provider calls and four correct no-calls per
mode. No-thinking completed 7/7 calls with faithful output at a 1.70-second mean.
Thinking emitted 15,493 separated reasoning characters, completed 6/7 calls at a
13.75-second mean among successes, and exhausted the 1,500-token allowance once.
It also leaked the voice descriptor into one report and added unnecessary source
date framing. Thinking is therefore genuinely available for explicit evaluation,
but remains disabled for Journalist production. The exact local records are
`/tmp/journalist-n94-original-form-think-ab-20260928.jsonl` for this run.

Thinking activated successfully in five diagnostic cases, but all five final answers
added unsupported content. Mean local articulation time was 13.99 seconds with
thinking and 3.41 seconds without (about 4.1×). This does not establish a fidelity
benefit or justify a production routing change.

## What ran

Local Ollama 0.32.14, `alibayram/smollm3:latest`, Q4_K_M, digest
`6463ebe1ce6ae2f6369498f54f136baad56b4380e5365bfecc938079f8e6733c`.
The plugin used fresh v2 / prompt n38. Fresh content, identity, memory selection,
voice and form were held fixed. No database writes, publication, model installation,
service configuration changes or Laya calls occurred.

The [publisher documents](https://huggingface.co/HuggingFaceTB/SmolLM3-3B#enabling-and-disabling-extended-thinking-mode)
`/think` and `/no_think` selectors. Local `/api/show` advertises thinking support,
and a simple capability probe returned a separated reasoning trace. That alone did
not establish that Journalist calls would reason.

### Production transport check

The existing replay now accepts an explicit mode and records actual requests,
final answers, elapsed time and reasoning character counts:

```sh
cargo run --example journalist_replay -- \
  fixtures/journalist/development.jsonl /tmp/journalist-thinking.jsonl \
  http://localhost:11434 compare
```

The output path must not already exist. Optional decoding mode `unconstrained`
after `compare` removes the schema for transport diagnosis; the default is `schema`.
The provider, Studio, preparation, output parser and source mapping are shared with
production. No correction/retry was added. The recorder omits reasoning contents;
only final answers and trace lengths are retained. Provider-incomplete errors retain
wall time and error text but lose raw response and trace measurements.

Eleven cases per mode produced eight calls and three correct no-call outcomes.
Parser acceptance was 6/8 with `think:false` and 5/8 with `think:true`; failures were
surface limits or exhausted output budgets. All seven completed provider responses
in each mode had zero reasoning characters. The one incomplete response per mode
has no retained trace measurement. Parser success was not factual fidelity: outputs
invented match details, biography, transfer context and league positions.

Consequently these flag-only timings are **not a reasoning-overhead comparison**.
The exact records are in [chat results](../fixtures/journalist/fresh-v2-thinking-chat.jsonl).

### Actual reasoning diagnostic

Removing the schema from the ordinary chat request did not by itself activate
reasoning, including with the explicit `/think` system selector. A raw SmolLM3
prompt with an assistant `<think>` prefix did. The matching nonthinking prefix was
`<think>\n\n</think>`. Both raw conditions used the same explicit role framing,
metadata date and plugin content; the thinking mode and assistant prefix differed.
Adding the JSON schema back to the prefixed diagnostic yielded no reasoning trace.
A further raw control with the native thinking flag and no schema did reason.
This isolates an integration problem worth investigating; it does not establish
that all Ollama thinking models or all templates have this behavior.

Five paired cases ran once per mode, alternating which mode ran first. Both used
unconstrained decoding, seed 42, temperature 0.3 and a 4,096-token context. Output
ceilings matched the existing provider policy: 900 tokens without thinking, 1,500
with thinking. No final answer was rewritten or repaired. This diagnostic uses
`/api/generate` with an explicit prompt, not production `/api/chat`. Its timings and
quality cannot be presented as a verified production route.

| Measurement | Thinking off | Thinking on |
| --- | ---: | ---: |
| Completed calls | 5/5 | 5/5 |
| Nonempty, closed reasoning section | 0/5 | 5/5 |
| Mean elapsed time | 3.41 s | 13.99 s |
| Median elapsed time | 3.09 s | 13.56 s |
| Range | 2.37–4.73 s | 12.31–16.43 s |
| Total generated tokens, including reasoning | 688 | 3,111 |
| Required JSON shape | 0/5 | 0/5 |

The final answer shape was either prose or JSON with top-level body/title fields
instead of the required narratives array. No parser salvage was attempted. Reasoning
was measured separately and never treated as publishable text or source evidence.

## Manual review of thinking-enabled answers

| Case | Unsupported result |
| --- | --- |
| Match result | Added goal times, a scorer, league positions and manager reactions |
| Rumour | Added player position, spokesperson statements and squad-building context |
| Conflicting reports | Claimed neither side would confirm or deny despite an explicit denial; inferred ongoing negotiations |
| Late correction | Invented an updated club statement; City Wire made the supplied correction |
| Source instructions | Followed the 50-point instruction and invented game events and a publisher quotation |

The [raw diagnostic records](../fixtures/journalist/fresh-v2-thinking-raw.jsonl)
retain exact requests, final answers, metrics and trace lengths. Each request can
be replayed directly against the local `/api/generate` endpoint; never publish these
outputs. The [review summary](../fixtures/journalist/fresh-v2-thinking-review.json)
contains per-case findings and limitations. Supporting activation controls are
[retained separately](../fixtures/journalist/fresh-v2-thinking-controls.json).

## Implication

Reasoning fits within tens of seconds on this local host for these small inputs,
but consumes about 10.6 extra seconds per call in the paired diagnostic. Whether
that fits a plugin's latency budget remains a product choice. This run did not
measure Laya/Editor savings, Granite speed, concurrency or end-to-end throughput.

More time to reason did not keep these answers within the prepared evidence.
Window 2 remains in progress. Keep production routing unchanged. The next harness
question is how to support a verified reasoning channel alongside the final output
contract; it should be addressed independently of new fact-selection stages or
additional corrective prompts. This is a small inspected development experiment,
not a general ranking of models or a release benchmark.
