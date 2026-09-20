# Read-error cleanup inventory

Production aggregate snapshot: **September 20, 2026, 12:13–12:15 EDT**. Counts change as workers continue. This inventory combines current queue counts with retained quality findings; it does not add retries, article dispositions and experimental outputs into a misleading grand total.

## Current failed work: 1,196 items

One item is one stage/sport/entity queue row, not one model attempt or one published bad reading. **1,169 were last updated before today; 27 were updated today.** Update time does not establish when the first failure occurred.

| Character / stage | Failed items | Last updated today | Retained error classification |
|---|---:|---:|---|
| Insider / transfers | 897 | 6 | 29 pair infrastructure/persistence; 868 unclassified by the bounded aggregate check |
| Editor | 222 | 6 | 218 context limit; 4 incomplete model outputs |
| Influencer / vibe | 31 | 0 | 27 missing labeled output fields; 3 parsing/decoding; 1 prompt echo |
| Oracle / sigil | 26 | 0 | Unclassified by the aggregate check |
| Scout / rating | 16 | 14 | 10 percentile/band errors; 4 unsupported claims; 2 unclassified |
| Investigator | 3 | 1 | HTTP/transport failures |
| Analyst / momentum | 1 | 0 | Unclassified by the aggregate check |
| **Total** | **1,196** | **27** | |

These are first-match text classifications of retained queue errors, not verified root causes. In particular, the 868 unclassified Insider items must not be assumed to share one cause. Journalist and Graph had no failed queue rows in this snapshot. Another 17 pending rows retain an error; those remain outside the failed total. The application outbox was empty.

## Article acquisition: a separate population

For articles stored September 20 EDT, 2,274 Editor dispositions existed at capture: 1,249 success, 539 irrelevant, 13 duplicate, and **473 acquisition problems**:

| Disposition | Articles |
|---|---:|
| Blocked | 195 |
| Empty body | 171 |
| Fetch failed | 106 |
| Paywall | 1 |

All 473 have `parser_outcome=no_call`: the model did not read a usable article. Irrelevance and duplicate dispositions are not automatically errors. Earlier source examples include NYT/247Sports HTTP 403, FanDuel empty bodies and BeSoccer fetch failures; those earlier examples do not establish the current domain distribution.

**Six failed Editor queue items concern articles stored today:** four context-limit failures and two incomplete outputs. They are already included in the 222 Editor failures, and must not be added again. Article 724370 reproduced 4,787 input tokens against a 4,096-token slot despite being below the article character cap. Whole-request token budgeting remains the concrete issue.

## Quality cleanup: failures that queue counts cannot capture

The following are retained observations, not a count of all flawed published cards. Rejected attempts, accepted offline replays and published products are explicitly distinguished.

| Workstream | Concrete evidence | Cleanup target |
|---|---|---|
| Numeric grounding and direction | Scout NBA player 4: parser-accepted frozen replay invented rating 3.71, delta 0.44 and percentile 67.1. Published NBA team 6 product 44137 describes a below-median negative delta as above median. Rejected Scout attempts: NBA players 79, 17896062, 38017697 reverse or invent facet movement. | Make supplied measurements and comparison dates unambiguous; verify every numeric/directional claim against frozen evidence. |
| Unsupported explanations | Rejected Scout attempts: NBA 73/17896076 and NFL 115 infer reduced playing time; NBA 56677826 equates percentile movement with ability; NBA 1028026974 invents height. Browns/Chelsea offline replays still infer physical/tactical causes. FOOTBALL 38 replay repeatedly failed the xG-to-creation guard. | Distinguish observed statistics from unknown causes; retain working guards. |
| Percentile bands | Rejected Scout attempts for NBA 1028025261, 1057263194, 1057266649 and 1057384156 disagree with supplied bands. Current queue has 10 percentile/band-classified failures. | Compare exact assignment, response and guard before deciding whether model or contract is wrong. |
| Analyst input contract and time windows | Current adapter supplies compact numeric fields, raw multi-season memories and an upstream “final” direction. It does not yet provide the intended Scout/Influencer readings plus relevant memories. Latest trial misdates seasons, mixes competitions, treats unmeasured form as flat and misreads the NFL 46→85 sentiment rebound. | Simplify to the Studio contract; replace overlapping labels with a clearly dated study where useful. |
| Output surface and prompt leakage | Latest Analyst trial: 4/12 outputs fail the surface check—3 overlong bodies and 1 truncated invalid JSON. Earlier Analyst NBA 1028028244 exposed “Vibe” in prose. Historical Influencer queue errors include missing fields and prompt echo. | Check current form/voice and generation limits against frozen cases; establish whether old failures reproduce under the current contract. |
| Editor request budget | 218 retained context-limit failures; article 724370 is reproduced. Four other retained Editor errors are incomplete output. | Budget the complete serialized request and output allowance, not article characters alone. |
| Identity, persistence and transport | Insider NBA team 21 reported absent player 521 through `essential memory identity`; a later drain retained pair progress and reported no infrastructure errors. Current queue has 29 pair infrastructure/persistence failures and 3 Investigator transport failures. | Inspect nested causes and retained progress before targeted retries; classify the remaining historical debt. |
| Acquisition quality | 473 current stored-today dispositions above. | Review sources and extraction separately from model quality. |

The earlier Scout replay set had 11 runs/21 attempts: 10 parser-accepted runs and one exhausted correction sequence. Acceptance did not prove factual correctness. The later Analyst experiment had 12 first-pass calls with no correction loop or publication: 11 valid JSON responses, 8 passing surface checks, and no reliable grounding improvement from added Vibe history. Neither experiment's outputs were published. These populations overlap in subjects and must not be added to queue totals.

## Already addressed, or not established as bugs

- Thin-sample context retaining historical cohort movement was fixed; Analyst eval now uses the production memory loader. Both changes have retained evidence in the quality findings. They do not establish general prose quality.
- Stale NFL cohort publication and missing publication receipts were addressed by the subsequent DuckDB maintainer deployment. Earlier comparisons established PostgreSQL/DuckDB arithmetic parity. Do not reopen those historical counts as current arithmetic defects without new evidence.
- FOOTBALL 390 has no eligible cohort rating; NFL 17 has only one eligible season. These are missingness/eligibility controls, not proven data failures. The later experiment also found no Analyst corpus for FOOTBALL 390 and correctly skipped inference.
- Fifteen reserved Rating jobs are intentional holds, not failures. Preserve them until the existing evaluation/release conditions are met.

## Proposed cleanup order

1. Fix the reproduced Editor request-budget issue and simplify Analyst inputs to the agreed Studio contract.
2. Reuse the retained numeric, direction, thin-sample and unsupported-cause cases to test grounded output. Avoid adding another prompt layer or a growing list of wording bans.
3. Classify historical Insider/Oracle failures and investigate identity/acquisition problems. Retry only confirmed, still-relevant work after its cause is addressed.

No production mutation, retry, hold release or model call was performed for this inventory. Production queries were bounded read-only aggregates. Automatic approval review rejected exporting individual failed-job records as broader than the requested counts and potentially sensitive; this report uses aggregate results and already-retained examples instead.

Evidence: [original handoff](../../run_docs/QUALITY_HANDOFF_2026-09-20.md), [September 20 quality findings](../../run_docs/quality-2026-09-20/findings.md), [Analyst history experiment](2026-09-20_duckdb-vibe-history-test.md), and private local artifacts under `logs/quality-20260920/` and `logs/duckdb-vibe-study-20260920/`.
