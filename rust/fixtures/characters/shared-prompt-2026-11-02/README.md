# Shared prompt and context-owned instructions

All six characters now share [`tools/prompt.rs`](../../../src/tools/prompt.rs). The separate `tools/context.rs`, shared `context.txt`, per-character `context.txt` and `instructions.txt` files are removed. There is no general-purpose template engine, extra inference pass or new dependency.

- Each `character_instructions.rs` owns the task and system/output rules.
- Each `voice.rs` owns expression. One fixed `{voice}` slot in the task preserves the original wording and sentence order; voice is rendered once.
- Each character's `fresh.rs` supplies its evidence together with its interpretation instructions. Shared `tools/meta.rs` supplies identity, optional attributes and their instructions together.
- Shared `prompt.rs` filters definitions to supplied fields and assembles `job → context_guide → meta → fresh`. Zero and false remain supplied values; missing/null are unknown, empty collections supply no items. Source values are not clipped or rewritten by the guide filter.
- The old character `prompt.rs` files are now `request.rs`: these retain request types, schemas, settings and preparation. They call the shared assembler rather than owning its structure.
- The decision model has `character_instructions.rs` for criteria/target binding and `fresh.rs` for source data and interpretation. Its native scoring adapter remains in `decision.rs`; it has no prose voice. Its 24 saved native requests are byte-identical after the refactor.

The top-five selection is unchanged: Journalist uses the decision model's subject score, Insider movement, Influencer emotion. Schemas and parsers use the same selected reports as the model. Historical context remains deferred.

## Verification

[Parity receipts](parity.json) cover 59 complete character requests: 52 quality fixtures across all six characters and seven development packets. Identity, selected evidence, guide meanings, schema, generation settings and should-call decisions are unchanged. The headline characters' full system and user inputs are byte-identical to the previous top-five implementation. Scout, Analyst and Oracle lose their redundant trailing tone fields and tone definitions; Scout also loses the obsolete system reference to that field. This removes 151, 99 and 170 input bytes per request respectively; these are byte counts, not native token claims. No generations were run for those three or for the decision model.

All 52 quality fixtures and the seven new development cases were regenerated through `character_package`, which uses production assembly. Original full candidate parts remain unchanged. [Before](before-requests.json) and [after](after-requests.json) requests and [original fixtures](before-fixtures.json) are retained. No parser rule was loosened. The unrelated production-recovery planning document retains its recorded SHA-256.

Run `cargo test --offline --lib --examples --bin eval`; results are in `tests.log` and `verification.json`. The initial missing-import compile failures are retained in `tests-initial-compile-failure.log`.

## Controlled comparison

Two batches of seven local `Qwen3.5-4B-Q4_K_M` generations, only Journalist, Insider and Influencer: temperature 0, thinking disabled, concurrency 1, no retries. The first version appended the voice sentence to each task and rephrased Insider's first sentence. It made Chelsea's charges story and Roma's actor errors worse. That variant was discarded; [its review](voice-at-end-review.json), requests, full outputs and native receipts remain under `voice-at-end-*`.

The retained implementation preserves the original task wording using the single voice slot. All seven final parsed JSON outputs match the prior top-five trial, including its known failures. Six pass mechanical checks; Neal fails the unchanged source/name check. Complete native input token IDs also match the prior trial exactly:

| Character / target | Articles | Before refactor | Final input tokens |
| --- | ---: | ---: | ---: |
| Journalist / Chelsea | 5 | 2,459 | 2,459 |
| Journalist / Demichelis | 4 | 2,102 | 2,102 |
| Journalist / Beal | 3 | 2,811 | 2,811 |
| Insider / Neal | 5 | 2,162 | 2,162 |
| Insider / Roma | 5 | 2,456 | 2,456 |
| Influencer / De Zerbi | 5 | 2,369 | 2,369 |
| Influencer / Jones | 2 | 1,412 | 1,412 |

This structural refactor adds no token savings for these three characters beyond the preceding five-article/instruction-trimming change. That preceding change reduced these seven packets from 32,070 to 15,771 aggregate input tokens (50.8%); see the [earlier trial](../top-five-2026-11-02/README.md) for the distinct per-request baselines. Aggregate counts are not one request's size. All seven retained requests remain below 4,096 input tokens, including system instructions and the native chat template.

[Full outputs and source reviews](outputs.md), [complete current prompts](prompt-preview.md), [native counts](input-tokens.json) and [machine-readable review](review.json) preserve the results. Chelsea still invents the source and intent of advice; Neal reverses destination versus competitor; Roma invents a coaching role; Jones's score still misreads two negative reactions as mixed. Beal retains the earlier separation of the jersey story from injuries. The refactor establishes structural parity, not factual improvement or held-out accuracy.

## Reproduce and limits

Build `character_package`, `character_outputs` and `classifier_replay` offline. `recapture.py` rebuilds the saved evidence through production and checks allowed differences against the frozen pre-refactor captures. `run_outputs.py` runs only the seven current cases and requires an absent `outputs/` directory; preserve existing receipts before another batch. `review_outputs.py` verifies native token IDs/counts and baseline output equality and renders both batches. Saved binary hashes identify the exact executables used.

These are dated development captures, not current news or tests of current ingestion/day selection. Native admission retains the existing 10,240-token trial context and unchanged output reservations; 4,096 is the input goal, not a newly enforced whole-context limit. Five unusually long articles/URLs can still exceed that goal and will not be silently truncated. No database access, production writes, deployment, publication, model downloads or extra model passes occurred.

Final versions: Journalist `n129-shared-prompt`, Insider `insider-source-v30-shared-prompt`, Influencer `vibe-frame-v42-shared-prompt`, Scout `s87-shared-prompt`, Analyst `momentum-s50-shared-prompt`, Oracle `or38-shared-prompt`.
