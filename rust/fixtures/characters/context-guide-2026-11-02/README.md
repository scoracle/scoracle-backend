# Task → context guide → data

All six characters now receive a reusable field guide before their data. The guide explains what each field means **in relation to the target**, without constructing a bespoke target-description sentence. This implements the requested template locally. The generation trials remain mixed; this is not an overall output-quality qualification or a deployment.

Read the [Journalist prompt and generated output](prompt-preview.md), the [complete Chelsea request](chelsea-request.json), or the [36 complete outputs and source review](outputs.md).

## Runtime contract

The serialized order is `job`, `context_guide`, `meta`, `fresh`, `tone`.

- `job` is still taken from the opening of the authoritative character instructions.
- `context_guide.meta` comes from `src/tools/context.txt`. It defines canonical identity, optional attributes, recorded relationships, dates and provenance. It distinguishes the target from its coach, affiliation and the actors in a report.
- `context_guide.fresh` comes from each plugin's `context.txt`. It explains reports, measurements or prepared perspectives using the actual field names for that character.
- `context_guide.tone` limits tone to expression.
- `meta` contains the supplied name, entity type, sport and optional attributes. The derived `meta.target` sentence and its formatter have been removed. No missing attribute is inferred.
- `fresh` remains unchanged. Publishers, wording, IDs, links, dates, scope, measurement values and comparisons retain their original values.

The same template reads team, player and person identities, including coaches and owners. It is not specialized to any named entity. This does not extend upstream routing eligibility: the existing routes remain responsible for selecting supported work. Historical context is still deferred. No model pass, dependency, output-schema change or parser relaxation was added.

Current prompt versions: Journalist `n124-context-guide`, Influencer `vibe-frame-v36-context-guide`, Scout `s83-context-guide`, Insider `insider-source-v25-context-guide`, Analyst `momentum-s46-context-guide`, Oracle `or34-context-guide`.

## Controlled local trials

Model: `Qwen3.5-4B-Q4_K_M` on the existing local llama.cpp endpoint. Temperature 0, thinking disabled, concurrency 1, no retries. Original schemas, output limits and other generation settings are identical across arms. Full inputs, native preparation/token receipts, requests, responses and production-parser results are retained.

Two development cases per character: 11 frozen real-data captures and one explicitly synthetic dated-comparison case. The real reporting is from the earlier October captures, not newly retrieved November news. Metadata was reused from the preceding read-only capture. These trials made no database requests or publications.

The first round alternated old/new order for 24 generations. The initial guide used explanatory paragraphs. The next 12 generations used direct field-to-meaning definitions on the same cases; the earlier control was retained rather than rerun. There is no held-out accuracy claim.

| Arm | Calls | Mechanically accepted | Input tokens | Output tokens | Summed request seconds |
| --- | ---: | ---: | ---: | ---: | ---: |
| Previous job/target template | 12 | 10 | 28,533 | 1,054 | 111.881 |
| Initial context guide | 12 | 9 | 34,773 | 1,057 | 124.374 |
| Refined field definitions | 12 | 10 | 34,685 | 1,152 | 124.193 |

Total: 36 generations, 97,991 input tokens and 3,263 output tokens. All completed without truncation or context holds. The refined template adds 21.6% input tokens on this set. Timings are from a shared workstation, including concurrent builds; they do not establish production throughput. Acceptance is a mechanical check, not factual accuracy.

The case builder asserts identical jobs, structured metadata, fresh evidence and tone. It removes only the previous derived `meta.target` when comparing metadata. The source review also asserts identical generation request settings after excluding the prompt tokens.

## What the outputs establish

- Journalist preserves the coach's two separate discussions, but still combines the player's injury and jersey-number stories into one entry.
- The initial Scout guide incorrectly produced a team cohort for a player. Directly relating `cohort` to the target's entity type corrected that in the refinement. The refined team output also keeps sample size and peer count separate.
- The initial Analyst guide avoids the unsupported desire-to-leave claim in the LaPorta case. That error returns in the refined version. The synthetic case still conflates distinct comparison windows.
- Influencer's first guide misassigns a criticism to the target. The refinement corrects its direction, but still treats two negative reports as conflicting/mixed and pluralizes a single former player.
- Insider's first guide preserves an injury absence as an injury, though it is irrelevant to the movement brief. The refined version again turns absences into departures and misstates the ownership of a package deal.
- Oracle still reverses some measurement meanings and adds unsupported overall diagnoses. The refined Boston output restores the strong rebounding description but calls elite turnovers poor.

These are specific improvements and regressions, not evidence of a general quality gain. The full [review](review.json) keeps every output and source-comparison note, including failures. The requested reusable context contract is retained locally; interpretation quality remains unfinished.

## Verification and context limits

`cargo test --offline --lib --examples --bin eval`: **417 passed, 74 ignored** (394 library, 14 eval, 9 example tests). Formatting and diff-whitespace checks pass. The shared assembly test preserves the same guide and exact data for team, player and person targets with different relationships. Existing quality fixtures were regenerated through the production assembler and match current versions.

One old unit check compared the entire assembled request with a source-only byte allowance. It now checks the source portion while retaining all 42 selected headlines. Runtime admission continues to measure the complete system and user request using the native tokenizer; it does not use this byte heuristic or drop source text.

A separate zero-generation tokenizer check found that the existing 24-report Chelsea preview already exceeds the 7,900-token application budget: 10,076 input tokens before the guide and 10,354 with the initial guide, both before reserving 2,100 output tokens. See [budget-check.json](budget-check.json). That file records the initial guide, not the later refinement. The current Chelsea JSON is an inspection preview, not an admitted generation request. All 12 smaller comparison cases fit without clipping.

No production deployment, publication or history integration was performed.
