# Exclude empty teasers — November 3, 2026

Historical trial record: the user subsequently removed the teaser-example sentence and shortened the instruction about keeping specific information. Current versions and exact wording are recorded in [the prompt ownership notes](../../../docs/character-prompts.md). The original results below remain attached to their tested versions. The later eight-call test of the exact cuts is recorded at the end of this document.

The user requested a sharper distinction between relevance and usable news: a headline can concern the target while withholding the actual information to attract clicks. The final candidate adds this instruction to each character's existing `fresh.rs` guide:

> Some articles match the target but reveal no news. Skip vague teasers such as 'shares big news' or 'major update' when the supplied text does not say what happened. Keep specific news, opinions and reactions that are actually stated in the other articles. Do not invent the missing details or turn a question or implication into a fact.

The condition refers to all supplied text, so an informative excerpt can rescue a vague headline. Specific speculative claims remain subject to the existing uncertainty rules. The change directs model selection within the supplied packet; it does not change the article cap, classifier, source selection, parser or production configuration.

Final versions: Journalist `n139-skip-empty-teasers`, Insider `insider-source-v40-skip-empty-teasers`, Influencer `vibe-frame-v52-skip-empty-teasers`. These remain experimental and undeployed. Production stays paused. All work and inference in this refinement ran on the Mac; Archbox was not accessed.

## Targeted comparison

Eight cases from the preceding [search/themes trial](../search-themes-2026-11-03/README.md) retain identical target/evidence, task, system rules, schemas and generation settings. The previous combined-candidate outputs are the frozen comparison baseline. Three variants received one generation per case, **24 new calls total**, with Qwen3.5-4B-Q4_K_M, temperature zero, thinking disabled, concurrency one, no retries and no publication.

1. **Substantive-news wording** explicitly excluded articles without concrete developments, claims or reactions. It caused the mixed useful/teaser Journalist packet to abstain entirely.
2. **Per-article wording** added separate assessment and preservation of remaining usable reports. It also dropped a second mixed packet and converted a specific speculative movement headline into a claim that no movement was reported.
3. **Final concise wording** uses examples of empty teasers. It restores the second mixed packet and removes an invented contract-expiry claim in one Insider case, but still drops the first mixed Journalist packet. Another recap still repeats vague teasers, and a concrete-control case invents a jersey retirement. Influencer's negative-emotion packet still receives a mixed score and has a damaged hook.

The final wording is clearer about the requested behavior, but **reliable item-level filtering is not demonstrated**. None of the variants is justified for deployment from this test. Strict validation passes are 4/8 at the prior baseline, then 5/8, 6/8 and 5/8; abstention can pass validation while discarding useful news. These are selected development cases, not an accuracy evaluation.

The final variant adds 51 native input tokens per request versus the preceding combined candidate: **10,575 → 10,983 total (+3.86%)**. Its tested inputs range from **1,167 to 1,609 tokens**, including system, user payload and chat framing. All responses, including regressions, remain retained. [Aggregate receipts](results.json) include all three variants.

## Verification and receipts

The deployed-source isolation is retained from the earlier trial to avoid concurrent classifier changes. The final source and regenerated fixtures passed **429 tests, 74 ignored** using `cargo test --offline --lib --examples --bin eval`. Thirty-one current quality fixtures were regenerated through production assembly; twenty-six unrelated requests stayed identical. Each recapture asserted unchanged target, evidence, field definitions, task, system rules, schema and settings. Formatting and scoped diff checks passed.

Private Mac receipts are under `/Users/scotty/scoracle-worker/experiments/` in `substantive-news-20261103/`, `item-newsworthiness-20261103/` and `skip-empty-teasers-20261103/`. Each retains fixtures, prompts, complete native requests, raw replies and failed checks. The final directory also retains the test log and operational replay scripts. No raw production packets or generated readings were added to this repository.

## Follow-up: exact user-requested cuts

Tested `n140-specific-news`, `insider-source-v41-specific-news` and `vibe-frame-v53-specific-news` with eight new Mac-only calls, compared with the eight saved outputs from the immediately preceding versions. An assertion verifies that the only payload changes are removal of the specified sentence and phrase. Target, evidence, task, system instructions, schemas and inference settings remain unchanged. Temperature is zero, thinking disabled, concurrency one, with no retries or publication. Archbox was not accessed.

Complete native input falls **10,983 → 10,711 tokens (−2.48%)**, or 34 tokens per case. New inputs range from **1,133 to 1,575**. Strict validation passes **4/8 versus 5/8**; one formerly accepted empty recap now contains reporting and fails a strict check, illustrating why this number alone does not measure quality. All eight calls completed without transport/admission errors.

Manual review finds one improvement, two regressions, one mixed result and four cases with no clear change. The invented jersey retirement disappears. The previously empty mixed packet now includes supported backlash reporting but still leads with the vague personal-news teaser. Another recap still lists nearly every headline. An invented contract-expiry/nonrenewal claim returns in Insider, while Influencer retains its mixed score for negative reactions and produces an incomplete hook. The shorter wording does not establish reliable filtering or a general quality improvement.

[Aggregate results and case reviews](specific-news-results.json) retain the comparison. Full evidence, outputs, strict failures, exact-cut parity receipts and a readable `review.md` remain private under `/Users/scotty/scoracle-worker/experiments/specific-news-20261103/`. This follow-up verified the isolated source match, offline replay-tool build, eight exact-cut comparisons and native token counts; it did not repeat the full Rust suite or deploy anything. Production remains paused.

Before committing, the final candidate and all 31 regenerated fixtures passed the isolated `cargo test --offline --lib --examples --bin eval` suite: **429 passed, 74 ignored**. An initial check found five fixture-drift failures because the temporary test tree still contained the preceding versions; synchronizing the already-regenerated workspace fixtures resolved all five. No source or parser changes were needed. This verifies the writer change against the deployed-source baseline, not integration with concurrent classifier edits. Test logs remain at `/private/tmp/scoracle-final-prompt-tests.log` and `/private/tmp/scoracle-final-prompt-tests-synced.log`.
