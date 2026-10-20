# Qwen3.5 emotional-reading promotion

Promoted October 20, 2026, at 07:45 Eastern, following the user's production decision.
The managed Mac worker connected to PostgreSQL and resolved `vibe-logic` to
`Qwen3.5-4B-Q4_K_M@http://127.0.0.1:8081`. The other five character routes retain
SmolLM3. This initial promotion uses the deployed October 19 source plus the
compact Influencer prompt and native ordered-schema transport. The subsequent
full pipeline release is recorded separately.

## Saved findings

The [matched follow-up](../fixtures/influencer/top3-followup-2026-10-20/report.txt)
contains 47 cases, three models and two greedy passes. Qwen passed 39/43 unique
hard decision cases, OmniDimen 32/43 and Ministral 26/43. Qwen passed all six
ordinary negative-emotion cases. These are provisional decision labels, not
factual-prose qualification or independent real-news gold.

Deployment testing found a material request difference: the screen supplied a
score-first JSON schema, while Rust alphabetized properties and decoded prose
first. Five identical-prompt controls all abstained with the screened order and
all invented a reading with the alphabetized order. The native adapter now
preserves the explicitly supplied schema bytes, verifies semantic agreement with
the declared schema, and retains the exact wire body in ordered-schema receipts.
Other calls keep their existing serialization.

The [production-path comparison](../fixtures/influencer/top3-followup-2026-10-20/production-path-comparison.json)
uses the real Rust assembler, native template/tokenizer/completion adapter and
parser: 24 common synthetic cases plus one retained neutral live headline.
Qwen passed 21/25 decisions versus SmolLM3's 8/25 with the same compact instructions
and score-first schema. All 50 outputs completed and satisfied the parser.
Qwen correctly abstained on the retained live headline; Smol invented a role,
opponent and performance details. Warm completion medians were 2.86s and 1.99s.
This single-pass transport check uses 32,768-token slots and Rust packet
serialization; it does not reproduce the original Python screen's 4,096-token
request envelope. Keep the denominators separate.

Qwen still missed cautious hope, explicit current relief with historical worry,
opposing coaches and another team's joy in the production-path check. Unsupported
gendered pronouns and affiliations remain known risks. No post-generation repair
or new factual guarantee was added. The existing schema/parser, evidence and
publication guards remain active.

## Runtime and provenance

- Weights: `unsloth/Qwen3.5-4B-GGUF`, revision
  `e87f176479d0855a907a41277aca2f8ee7a09523`, Q4_K_M.
- SHA-256: `00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4`.
- Durable file: `~/models/Qwen3.5-4B-Q4_K_M.gguf`.
- Managed agent: `~/Library/LaunchAgents/com.scoracle.qwen35-emotion.plist`.
  Localhost 8081; one 32,768-token slot; GPU layers 99; flash attention on;
  thinking off; cache RAM disabled. Requests use temperature zero, no prompt
  reuse and a 600-token output reservation.
- Prompt: `vibe-frame-v30-compact-emotion`. Screened system instructions are
  byte-identical; the user packet contains `meta`, `fresh`, `memories`.
- Compact system SHA-256:
  `82d24383d3111fe06cd2999c713a5c0f88ec7a0579ac9cc3ae33f49ca65fbbcb`.
- Route concurrency: existing Smol server four permits; Qwen server one permit.
- Initial promotion binary SHA-256:
  `2fdfc22ef43ee250670016c1e9073a96bcc67a0eb0af040d401c2cb3bc69e224`.
- Initial frozen source archive SHA-256:
  `cfef3b624ae9860dc871c6f215cf75a8868f5d08701dfdbc00b3e0b1cd6139ee`.

Private durable recovery and receipts:
`~/scoracle-worker/releases/qwen35-emotion-20261020/`. This includes the original
benchmark receipts, model metadata, both transport smokes, schema-order controls,
frozen source, binaries and prior environment/launcher/agent. Do not depend on
`/private/tmp` for recovery. Before the subsequent pipeline migration, reverting
the saved launcher and environment and gracefully restarting the managed worker
reverts this promotion. After migration 315, use the coordinated full-release
recovery procedure instead of restoring the older worker.

The daily schedule was not advanced and no fictional output was published.
Natural nightly publication and sustained concurrent model throughput remain
unobserved for this promotion.
