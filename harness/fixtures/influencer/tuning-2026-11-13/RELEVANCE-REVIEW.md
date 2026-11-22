# v60 user relevance edits: final retest

Preserved the user’s instruction edits and the user-added evidence context explaining that a decision model flagged emotional charge. No autonomous prompt trimming or further tuning was applied.

Same 15 frozen player/team cases; unchanged target metadata, evidence, schema, temperature 0, model Qwen3.5-4B-Q4_K_M, thinking disabled, 10000-token context and 3500-token output allowance. Unlike v59, this run uses the compiled isolated production code and Rust parser. All 15 completed and passed mechanical parsing. This is not a factual-quality certification.

A single preflight Hornets call is retained in `relevance/`; the complete retest is `relevance-final/`. The first fixture comparison detected the additional user-authored content-context sentence. Snapshots were refreshed before the complete run. Release library tests: 407 passed, 74 database-dependent tests ignored.

| Case | Score | Observation |
| --- | ---: | --- |
| 01 Charlotte Hornets | 45 | DZN publisher spelling returns; mixed reading retains unrelated/ambiguous Hornets items. |
| 02 Philadelphia Eagles | 25 | Drops TV-seating distraction; hook is now about NFL anger and season criticism. Trade disappointment is still attached imprecisely. |
| 03 Seattle Seahawks | 25 | Retains trade enthusiasm and negative reactions; group wording and publication/speaker attribution still warrant review. |
| 04 Arsenal | 65 | Drops Mamdani editorial; correctly mentions Arsenal defeat, but changes the recipient of praise in the hook to Arteta. |
| 05 Roma | 45 | Shorter reading; assigns an unstated coaching relationship and compresses separate events. |
| 06 Miami Heat | 25 | Correctly preserves Bucks winning before Miami; negative score 25. |
| 07 Tampa Bay Buccaneers | abstain | Correct abstention on a statistical feat. |
| 08 Chelsea | 25 | Manchester United victory interpretation remains; mixed reading scored 25. |
| 09 Luka Doncic | 50 | Retains praise and MVP frustration; score 50 and invented Lakers-fan attribution remain. |
| 10 Jayson Tatum | abstain | Correct abstention on statistical coverage. |
| 11 Dak Prescott | 50 | Negative reactions again score 50; known scoring inconsistency retained for observation. |
| 12 Baker Mayfield | 45 | Separates Chiefs-related coverage more clearly, but keeps the loss/critical framing attached to Mayfield. User accepted this style of reading; attribution/outcome interpretation remains a corpus review item. |
| 13 Virgil van Dijk | 25 | Accurate single-source negative reading, score 25. |
| 14 Gessime Yassine | 75 | Narrow positive publisher framing, score 75. |
| 15 Vit Krejci | abstain | Correct abstention on minutes played. |

The user authorized production observation. Remaining output errors are retained for the planned 48-hour corpus review rather than silently repaired or discarded.
