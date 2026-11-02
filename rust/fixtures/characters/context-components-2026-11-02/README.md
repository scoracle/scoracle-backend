# Explicit context components and inline character roles

This pass implements the agreed ownership refinements without changing what the models receive:

- Role and voice are inline in each `character_instructions.rs`. All six `voice.rs` files and the `{voice}` substitution are removed.
- Each context provider returns `Context<T> { guide, data }`. `Guide` contains always-present relationship guidance and explicit `(field_path, meaning)` pairs. No Markdown/prose parsing determines program behavior.
- Each plugin's `request.rs` obtains prepared metadata and fresh components, then passes them to shared `tools/prompt.rs`. The assembler no longer imports database-record or identity types, constructs metadata, or selects evidence.
- The shared assembler retains the four-key `job → context_guide → meta → fresh` structure. There is no trailing tone field. The decision model retains its native scoring protocol and now uses the explicit guide structure too.

[Ownership and tuning documentation](../../../docs/character-prompts.md) explains where changes belong and how a new character reuses the existing components and registration. No framework, registry, dependencies, dynamic inheritance or model passes were added. The five-article selector remains unchanged for Journalist/Insider/Influencer, using subject/movement/emotion scores respectively.

## Structural verification

Before editing, `recapture.py --before` exported 59 complete character requests through production preparation: all 52 quality fixtures across the six characters and the seven development packets. It also froze 24 native decision requests and the unrelated production-recovery document's SHA-256.

After the refactor, all 59 complete requests are identical to the snapshots, including exact system/user strings, JSON schemas, options and should-call decisions. All 24 decision requests are byte-identical. Candidate fixture parts remain identical. [Before requests](before-requests.json), [after requests](after-requests.json), [original fixture parts](before-fixtures.json) and [parity receipts](parity.json) are retained.

Tests cover zero/false versus missing/null/blank/empty values, array paths, prose punctuation independent of field selection, a custom metadata shape, request ordering and multiple target types with stored relationships. The existing selection, parser and production/replay tests remain intact. `cargo test --offline --lib --examples --bin eval` passes **420 tests, with 74 ignored**; see [verification](verification.json) and [test log](tests.log).

Prompt versions were bumped and all 52 quality fixtures plus seven development cases regenerated. The version/instruction representation changes affect internal fingerprints, even though rendered model requests are unchanged. The unrelated planning document is preserved.

## Resumed local generation tests

Seven new generations on the existing `Qwen3.5-4B-Q4_K_M` endpoint at `http://127.0.0.1:8081`: temperature 0, thinking disabled, concurrency 1 and no retries. Only Journalist, Insider and Influencer were generated. Every complete native input token sequence and parsed JSON output matches the preceding shared-prompt trial.

| Character / target | Selected articles | Native input tokens | Mechanical result |
| --- | ---: | ---: | --- |
| Journalist / Chelsea | 5 | 2,459 | Accepted |
| Journalist / Demichelis | 4 | 2,102 | Accepted |
| Journalist / Beal | 3 | 2,811 | Accepted |
| Insider / Neal | 5 | 2,162 | Rejected |
| Insider / Roma | 5 | 2,456 | Accepted |
| Influencer / De Zerbi | 5 | 2,369 | Accepted |
| Influencer / Jones | 2 | 1,412 | Accepted |

This refactor saves **zero additional model-input tokens** for the tested characters: its purpose is clearer ownership and simpler code. It preserves the earlier reduction to inputs below 4,096 tokens in these seven cases. Counts include system instructions and the native chat template; 15,771 is the aggregate across seven requests, not one request's size.

Manual source review reconfirms the known failures. Chelsea invents the adviser and direction of advice, and pluralizes a single player. Neal turns joining an NFC East rival into competing with one; the unchanged parser rejects the expanded Philadelphia Eagles name separately. Roma invents a coaching role for Pellegrini, includes injury reporting and contradicts supplied movement evidence. Jones's two negative reactions still receive a mixed/50 assessment. Influencer exceeds its requested sentence count. Beal keeps the jersey story separate from injury reporting. These are development observations, not held-out accuracy results or evidence of a general quality improvement.

All outputs, including the rejected Neal result, remain in [outputs.md](outputs.md), [review.json](review.json) and `outputs/`. [Complete prompts](prompt-preview.md) and [native token receipts](input-tokens.json) make the tested inputs inspectable. Parser acceptance does not establish factual correctness; no validators were relaxed.

## Reproduce and limits

Build `character_package`, `character_outputs` and `classifier_replay` with Cargo offline. Run `recapture.py` to rebuild through production and assert exact request parity. `run_outputs.py` runs the seven current cases with a fresh output directory; it refuses to overwrite receipts. `review_outputs.py` compares native token IDs and outputs with the previous trial and retains the source reviews. The generation binary hash is saved in `binary.json`.

The test uses the existing 10,240-token admission context with unchanged output reservations. The 4,096-token input goal is not a new hard total-context limit; five unusually long articles/URLs can still exceed it and are not silently clipped. These are dated development captures and do not test current ingestion/day selection. Scout, Analyst, Oracle and decision-model generation remain outside this tuning run. Historical context stays deferred. No database writes, deployment, publication or downloads occurred.

Current versions: Journalist `n130-context-components`, Insider `insider-source-v31-context-components`, Influencer `vibe-frame-v43-context-components`, Scout `s88-context-components`, Analyst `momentum-s51-context-components`, Oracle `or39-context-components`.
