# v59 target-description trial

Added the exact user-approved sports-entity description as `target_context` before `target`. All previous request fields, instructions, system message, evidence, model and generation settings remain unchanged.

## Execution and limits

Two Rust build attempts and a diagnostic sampler stalled and were stopped. The local model health endpoint responded immediately. The offline trial used a temporary Python reproduction of the existing llama.cpp native transport, retained as `target-context/native-trial.py`.

For every case, the runner reproduced the saved v58 rendered prompt and token IDs exactly before inserting the description. It verified unchanged serving-model/template metadata, used the same ordered JSON schema, temperature 0, thinking disabled, 3500 output allowance and 10000-token requested context ceiling. It checked that all input tokens reached the model and that output ended normally without truncation. All 15 completed and passed basic JSON shape/range/length checks. **The production Rust parser and updated Rust build were not validated.** This is a controlled model-input experiment, not a validated deployment.

Production remains paused. No additional prompt edits were made after the approved change. This is a small development set, not an independent holdout or a repeated-run estimate of reliability.

## Results versus v58

| Case | v58 | v59 | Finding |
| --- | ---: | ---: | --- |
| 01 Charlotte Hornets | 25 | 45 | **Improved hook; issues remain** — DAZN remains correct. Hook no longer makes fans recipients of criticism; score moves 25 to 45. Body still merges many threads into one five-sentence paragraph and accepts ambiguous Hornets coverage. |
| 02 Philadelphia Eagles | 25 | 25 | **Still fails** — Body separates TV-seating frustration from team mood, but hook turns it into fans calling the season exhausting. Trade-idea disappointment is still assigned to the wrong object. |
| 03 Seattle Seahawks | 25 | 25 | **Mixed** — Retains positive trade reaction and removes the garbled Troy Aikman moment phrasing. Still generalizes Bears Wire’s good/bad/ugly assessment to Seahawks mood, pools anthem wording, and adds one-fan specificity. |
| 04 Arsenal | 65 | 65 | **Mixed** — Hook no longer asserts an upset win. But Arteta praise is assigned to ArsenalNews.co.uk instead of Flashscore.co.za, going level top becomes a top-of-table finish, and Mamdani remains the purported speaker of an editorial suggestion. |
| 05 Roma | 45 | 45 | **Improved hook; issue remains** — Removes unsupported Roma-win hook and keeps anger/caution distinct. Adds unsupported temporal sequencing with later. |
| 06 Miami Heat | 45 | 25 | **Still fails** — Still says Miami secured a win, reversing the Bucks-before-Miami headline. Outlet/speaker presentation improves from ESPN’s Bam Adebayo to ESPN noted that Bam Adebayo stated. |
| 07 Tampa Bay Buccaneers | abstain | abstain | **Stable acceptable** — Retains correct abstention on statistics-only evidence. |
| 08 Chelsea | 25 | 25 | **Still fails** — Still asserts Manchester United’s victory and Chelsea’s broader struggles while the misery belongs to Manchester United. |
| 09 Luka Doncic | 75 | 50 | **Mixed** — Restores the omitted MVP-ranking frustration, but reverts 75 to 50 despite predominantly positive coverage and invents Lakers fans as praise-givers. The headline MVP fumes is awkward and imprecise. |
| 10 Jayson Tatum | abstain | abstain | **Stable acceptable** — Retains abstention instead of inventing celebratory feelings from statistics. |
| 11 Dak Prescott | 25 | 25 | **Stable with caveat** — Keeps negative score 25 and a supported main hook. Reintroduces coach specificity and retains shouting, neither established by the supplied packet. |
| 12 Baker Mayfield | 45 | 25 | **Still fails** — Still transfers Falcons’ horrific-loss framing and Chiefs’ implosion into Mayfield’s reading. Assigns his quote to the publishing team/organization and still calls it Mayfield’s no-pass loss. |
| 13 Virgil van Dijk | 25 | 25 | **Stable acceptable** — Preserves single-source disappointment and score 25. |
| 14 Gessime Yassine | 75 | 75 | **Stable acceptable** — Preserves narrowly attributed positive publisher framing and score 75 without invented feelings. |
| 15 Vit Krejci | abstain | abstain | **Stable acceptable** — Retains correct abstention on playing-time statistics. |

## Conclusion

The new description preserves the targeted abstentions and Dak polarity, and improves some hooks. It does not reliably solve entity/event binding: Heat, Chelsea and Mayfield retain winner/loser errors; Arsenal retains editorial-speaker and publisher-attribution errors; Eagles’ hook still broadens a TV-seating reaction to the whole season. Doncic restores a missing reaction but reverts to an overly balanced score and invents praise-givers.

These results do not justify claiming a general improvement in subject identification. Retain the exact trial for review before choosing another edit. Build/test validation remains outstanding independently of model quality.
