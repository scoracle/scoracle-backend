# Clear character payloads — 2026-11-02

The model JSON is now `instructions → target → new_content`. Instructions contain the character task and readable lists explaining the target and supplied evidence. Field definitions are still conditional on supplied values. Context providers still own their data and rules; the shared assembler orders and pretty-prints them.

Article IDs, URLs and publication timestamps are omitted from writing projections. Internal source records and fixture parts retain them. Exact source wording, publishers, qualifications, selection order, output schemas and report citation handles are preserved. Dates inside source wording, stored-attribute observation dates, measurement windows and generated-card dates are not article publication metadata and remain.

This is a presentation change, not a same-day selection change. The live Dak capture contains pending reports published October 19–21. Their publication dates remain in the saved source parts; omission from the prompt does not establish that they are today's events. Historical context remains deferred.

## Reviewable inputs

- [Current Chelsea JSON](chelsea-live.payload.json), [system instructions](chelsea-live.system.txt), [complete native input](chelsea-live.complete-input.txt).
- [Current Dak Prescott JSON](dak-prescott.payload.json), [system instructions](dak-prescott.system.txt), [complete native input](dak-prescott.complete-input.txt).
- [All nine rendered development requests](prompt-preview.md).
- [Before requests](before-requests.json), [after requests](after-requests.json), [assembly parity checks](parity.json).

These JSON files are exactly the user-message payload, including indentation. System rules remain a separate system message. The complete-input files include both messages and the model's native chat framing. The JSON output schema is transported separately as a decoding constraint, not appended as prompt tokens.

## Controlled comparison

Qwen3.5-4B-Q4_K_M at `http://127.0.0.1:8081`; temperature 0, thinking disabled, concurrency 1, no retries. Only Journalist, Insider and Influencer were generated. Nine cases share identical before/after source parts. Seven prior baselines come from the context-component trial; two live baselines were generated using the preserved pre-change binary. Nine new-format outputs were generated: eleven new calls in this pass.

Complete native inputs total **20,743 → 11,668 tokens**, saving **9,075 (43.75%)**. Every new input is between **1,069 and 1,556 tokens**, below 4,096. Counts use the prepared native token-ID array and agree with generation receipts. They include system rules and chat framing, not just the visible JSON. This is measured on these development packets, not a hard size guarantee for arbitrary five-article excerpts; existing admission checks remain in place without truncation.

| Case | Before | After | Saved | Validation |
| --- | ---: | ---: | ---: | --- |
| chelsea-live-live-journalist-preview | 2,612 | 1,224 | 53.1% | accepted → accepted |
| dak-prescott-live-journalist-preview | 2,360 | 1,188 | 49.7% | rejected → rejected |
| chelsea-target-preview | 2,459 | 1,213 | 50.7% | accepted → accepted |
| narratives-person-86-FOOTBALL | 2,102 | 1,322 | 37.1% | accepted → accepted |
| narratives-player-37-NBA | 2,811 | 1,069 | 62.0% | accepted → rejected |
| transfer-player-83946-NFL | 2,162 | 1,556 | 28.0% | rejected → rejected |
| transfer-team-37-FOOTBALL | 2,456 | 1,451 | 40.9% | accepted → rejected |
| vibe-person-23-FOOTBALL | 2,369 | 1,435 | 39.4% | accepted → accepted |
| vibe-person-959-NFL | 1,412 | 1,210 | 14.3% | accepted → accepted |

## Output quality

The smaller payload does not establish a quality improvement. Mechanical acceptance fell from 7/9 to 5/9; one new accepted response was an incorrect abstention. Actor attribution, unsupported details and conflated measurements still occur. In particular, Chelsea still misidentifies speakers; Dak still conflates different ranking windows; Roma leaks stored coach metadata into prose; De Zerbi transfers “great news” from a player's return to the result; Jones abstains despite explicit emotional evidence. Beal newly fails the existing guard after expanding Clippers to Los Angeles Clippers. The parser has not been loosened to accept these outputs.

Some individual changes helped: Chelsea's unnamed player stays singular, the first live Chelsea story omits an invented squad detail, and Dak's backlash item keeps the exact 0-2 record and one publisher. These improvements coexist with regressions. [Manual review and before/after replies](outputs.md), [structured review](review.json), and [token receipts](input-tokens.json) retain the full comparison. Layout, guide formatting and provenance removal changed together, so their separate causal effects are not identified. These are development cases, not held-out accuracy results.

## Verification and replay

All 61 character requests were checked through production assembly, including 52 regenerated quality fixtures, seven existing development cases and two live captures. The source parts are identical, with only the specified provenance fields omitted from model evidence. Output schemas, settings and should-call decisions are unchanged. All 24 decision requests remain byte-identical. Prompt versions advanced for all six characters because the shared envelope changed; only the three requested characters received generation testing.

`cargo test --offline --lib --examples --bin eval`: **420 passed, 74 ignored**. Formatting and diff checks pass. The initial test failure log is retained: two expectations still checked the old `job` prefix, and were updated to the new envelope. The unrelated production-recovery planning document has the same hash as before this pass. See [verification receipt](verification.json).

```sh
cargo build --offline --example character_package --example character_outputs --example classifier_replay
python3 fixtures/characters/clear-payload-2026-11-02/recapture.py
VOICE_NUM_CTX=10240 target/debug/examples/character_outputs /tmp/scoracle-clear-payload-replay http://127.0.0.1:8081 Qwen3.5-4B-Q4_K_M fixtures/characters/clear-payload-2026-11-02/cases/*.json
```

The output directory must not already exist. Preserve the frozen `before-*` artifacts. Production and replay use the same projection, assembly, preparation and validation path. Current readers also understand the prior envelope for historical receipts. Generation artifacts retain their original capture annotations; “no generation” in those annotations describes the earlier read-only capture, not this controlled replay. No deployment, publication or production database writes occurred.
