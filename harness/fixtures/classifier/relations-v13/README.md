# Target-bound classifier candidate

Implements the four follow-ups from the November 3 corpus review. Production remains on the prior v12/brief-v2 release. This candidate uses `classification-criteria-v13-relations`, `classifier-brief-v3` and `headline-routing-v3-identity`.

The [complete native requests](complete-native-requests.json) and [readable inputs](complete-inputs.txt) include synthetic Chelsea, Dak Prescott, squad, speaker, roster-affiliation and namesake examples. [Development controls](cases.jsonl) contain 28 synthetic cases with assistant-provisional labels. None is independent human gold.

## Changes

- Newsworthiness requires a concrete reported development and a substantive connection to the exact target. Listing/advice templates and promises of unspecified news are distinguished from actual results, statements, rumours and denials.
- Emotion identifies the target of the feeling, praise or criticism. Speaking about another person does not transfer that person's emotion to the speaker.
- Movement and availability identify the affected participant. Replacement signings and the injuries prompting them can concern different players; a club can have both kinds of news.
- Identity context explicitly represents an unknown team category, accepts an explicit canonical league category when a team category is absent, drops undated coach background, and supplies dated roster/override affiliations only for people canonically named in the headline. A relationship is labeled “this target” only when canonical team ID and name both match. Stats/legacy affiliations are not used for that bridge. Internal IDs remain in provenance, not model state.

Explicit conflicting personal-name suffixes additionally block admission and every character route without modifying native scores. Jr./Junior and Sr./Senior are equivalent; a missing suffix remains ambiguous, and mentioning both people does not trigger this exclusion. The sports separator “v” is preserved. The SQL name-boundary function also avoids resolving a shorter name from inside a suffixed full name. Routing policy is versioned separately because this is an identity exclusion; all four numerical cutoffs are byte-for-byte equivalent to v2: subject 55, emotion 15, movement 80, availability 85.

The existing shared assembler still renders instructions → target → headline/publisher. Plugin task definitions remain in `character_instructions.rs`; context interpretation stays with providers. Production and replay use the same request builder and target-aware routing function. There is one native request per headline–target relationship and no extra model pass or writer-style batching.

## Checks and evaluation method

The required `cargo test --offline --lib --examples --bin eval` suite and five disposable-database checks cover the candidate. Database checks exercise category fallback, suffix boundaries, named-person context, cache reuse, archival delivery and historical readers. Old v9–v12 headline receipts remain readable; stored request bodies/hashes are not rewritten. Migration 331 was applied only to a disposable local database.

Three development comparisons were retained, each with 100 pairs/200 calls: 28 synthetic controls and the same 72 previously inspected production relationships. Each pair alternates baseline/candidate order, uses the same pinned d3 Q8_0 runtime on Archbox, and runs at client concurrency one. The classifier queue was empty and coverage in the character phase; the runner checks queue/phase and idle decision slots before every request and stops if production work resumes. No production concurrency or model configuration changed.

Production-derived metadata is replayed as saved, without querying fresh affiliations or filling missing category values. Thus the 72-case comparison does not establish the effect of newly populated relationship data in production; the new context path is tested through synthetic controls and disposable database fixtures. All 72 baseline native requests must exactly match their archived originals before replay begins. Native input tokens come from response usage, not JSON size. Client elapsed times are not isolated GPU compute time or production-concurrency latency predictions.

Route expectations derive only from available provisional criterion labels and deterministic entity eligibility. Denominators differ by character; unknown labels are excluded. Native scores at 50% are reported separately from production cutoffs. Revision two and later use the actual Rust `gate_for_target` via `classifier_replay --routes-only` to verify candidate routing against saved scores. This mode makes no inference requests and requires `replay_spectrum` in each case.

The first candidate improved admission but worsened emotion routing. It was retained rather than silently discarded. Subsequent development revisions shorten the questions, enforce explicit namesake conflicts, clarify canonical target affiliations and isolate availability wording. These are development iterations on inspected cases, not held-out qualification.

## Final development results

The final comparison completed 200/200 native requests (100 pairs), with zero request failures. These counts measure agreement with the available provisional labels, not population accuracy:

| Production route | Baseline correct | Candidate correct | False positives, before → after | False negatives, before → after |
| --- | ---: | ---: | ---: | ---: |
| Admission / Journalist | 66/87 | 78/87 | 20 → 8 | 1 → 1 |
| Influencer | 54/66 | 56/66 | 12 → 10 | 0 → 0 |
| Insider | 60/63 | 62/63 | 3 → 1 | 0 → 0 |
| Scout | 62/67 | 63/67 | 1 → 1 | 4 → 3 |

There were no newly incorrect routes among these labeled comparisons. Across all 100 inputs, including incompletely labeled ones, 14 admission, seven Influencer, five Insider and three Scout decisions changed. Two corrected namesake admissions come from the deterministic identity exclusion; both still pass the native subject cutoff. The routing gains therefore cannot all be attributed to better model scores.

Native criterion agreement at the replay's 50% cutoff was subject 63/87 → 74/87, emotion 32/35 → 34/35, movement 28/30 → 28/30 and availability 26/30 → 28/30. These are distinct from the production routing results above. The final availability-only revision reproduced all 300 other candidate criterion scores and all 400 baseline scores from revision two.

Native input usage was present for 100/100 requests in each arm: 180,648 → 214,204 total tokens (**+18.6%**), median 1,792 → 2,163. Median client latency increased from 0.668 to 0.828 seconds (**+24.0%**); p95 increased from 0.756 to 0.858 seconds. The extra context has a measurable cost. All three development trials and their 600 successful native calls are retained, including the first candidate's emotion-routing regression. [Aggregate results](evaluation.json) include each trial and final error review.

The remaining false positives include vague revelations and reactions, a stream listing, promotional emotion, and praise attributed to its speaker. Five remaining admission false positives in the production subset comprise four withheld-development/target-link cases and one listing/advice case. Remaining false negatives include the synthetic roster-affiliation bridge (subject 31.89 despite availability 90.00) and a replacement-signing club case below the Scout cutoff (availability 77.56). Supplying clearer affiliations alone did not solve the bridge case. Preserve these failures for human review and subsequent qualification; do not tune thresholds from this inspected set.

Validation: the required Rust suite passed **433 tests**, with **75 ignored**, followed by five passing targeted disposable-database checks. The benchmark self-check, Rust formatting and whitespace checks also pass. Synthetic complete requests were re-exported from the final candidate after the last criteria change.

## Deployment proposal — not activated

Review the comparative results and remaining regressions before choosing a release. Build an isolated commit containing only this classifier change, the shared context changes, synthetic fixtures and migration 331. Apply migration 331 before starting that binary; it accepts brief-v3 alongside older receipt layouts and extends the existing provenance trigger to v13. Stage compatible readers for the writer workers without changing their observation policy or starting paused workers. Preserve the existing d3 weights/runtime, thresholds, queues, retries and stored scores.

The suffix-boundary correction can cause an old source's canonical identity set to differ from its captured set. Existing identity checks should continue to fail closed for those affected sources; do not bypass them or automatically rescore historical records. Assess any reconciliation separately before activation. A rollback would restore the prior binary while leaving compatible receipt migrations and collected records intact.

There are still no explicit canonical league categories in the inspected production database, and the saved corpus supplied none for its 206 team targets. The fallback therefore prepares the path for verified data; it does not manufacture missing gender or squad values. Canonical data repair remains a separate, evidence-backed prerequisite for confidently resolving those teams.

Raw production cases, every native response, setup failures, both rejected/intermediate candidates and per-case diagnostics remain on Archbox under `/mnt/data/backup/scoracle/classifier-calibration/2026-11-04-relations-v13/`. Only aggregate comparisons and synthetic examples belong in this directory. No production service was restarted, deployed, rescored or otherwise changed.
