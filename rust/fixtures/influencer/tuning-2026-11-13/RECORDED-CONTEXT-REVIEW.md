# v57 recorded-context review

All 15 cases completed and passed mechanical parsing. This is not 15 factual passes. Evidence, temperature (0), model, context (10000), output allowance (3500) and schema were held fixed. This arm combines the cleaner context/evidence presentation, Journalist target formatting and the approved metadata instruction; changes cannot be causally attributed to one of those components.

Production remains paused. No further instruction edits or deployment were made after this run. The fixture reassembly regression test passed.

| Case | v54 | v55 | v57 | Review |
| --- | ---: | ---: | ---: | --- |
| 01 Charlotte Hornets | 50 | 50 | 50 | **Needs work** — DAZN becomes DAWN. Four sentences in one paragraph. Complete hook replaces the original fragment. |
| 02 Philadelphia Eagles | 75 | 25 | 25 | **Needs work** — Negative score corrected to 25 and body now attributes season from hell to PhillyVoice. Hook invents fans as the people tracking it. |
| 03 Seattle Seahawks | 75 | incomplete | 25 | **Improved, caveat** — Completes without the prior output-limit failure; no invented singer. Converts a group reaction into one fan and omits the positive trade reaction; the negative direction is defensible but coverage is narrower. |
| 04 Arsenal | 75 | 50 | 50 | **Needs work** — No longer claims Arsenal won the upset. Still turns the Guardian editorial suggestion that Mamdani should challenge Arsenal into Mamdani’s own desire, and relocates Gakpo’s frustration to a transfer claim. |
| 05 Roma | 50 | 50 | 50 | **Needs work** — No longer calls the Red Devils injured. Conflates the Napoli draw and Manchester United material; changes a warning to goal-crazed Roma about opponents into a warning about being goal-crazed. |
| 06 Miami Heat | 75 | 25 | 25 | **Needs work** — Negative score corrected to 25. ESPN’s Bam Adebayo blurs outlet and speaker, and the hook adds self-doubt not explicitly supplied. One paragraph has four sentences. |
| 07 Tampa Bay Buccaneers | abstain | abstain | 50 | **Regression** — Previously abstained; now recaps a rare statistical feat and assigns 50 despite no emotional evidence. |
| 08 Chelsea | 50 | 25 | 25 | **Needs work** — Still describes Manchester United’s victory where the supplied Chelsea headline says after Manchester United win. The hook is complete, but the winner remains reversed. |
| 09 Luka Doncic | 75 | 50 | 50 | **Needs work** — Assigns awesome wording to Redick without support for that attribution. Scores 50 despite predominantly positive praise; unchanged from v55, below the positive v54 reading. |
| 10 Jayson Tatum | abstain | abstain | 75 | **Regression** — Previously abstained. Now calls statistical coverage celebration, assigns 75 and includes the Nets’ haunted framing in a Tatum reading. |
| 11 Dak Prescott | 25 | 50 | 50 | **Needs work** — Omits the irrelevant touchdown recap, but still assigns 50 to two negative reactions. Adds shouting and head-coach specificity not supplied in the retained target/evidence. |
| 12 Baker Mayfield | 25 | 25 | 25 | **Needs work** — Still assigns the Falcons’ no-pass loss and horrific framing to Mayfield. Hook calls it Mayfield’s loss while also calling it a Tampa Bay win. Chiefs game is identified as separate but still drawn into the emotional context. |
| 13 Virgil van Dijk | 25 | 25 | 25 | **Acceptable** — Accurate single-source disappointment reading; score 25. Hook could more clearly say loss to Arsenal, but the body preserves the direction. |
| 14 Gessime Yassine | abstain | abstain | 75 | **Acceptable** — Narrow, attributed positive publisher framing: dominates. Score 75; no invented player feelings. Earlier abstention was also acceptable under the prewritten rubric. |
| 15 Vit Krejci | abstain | abstain | abstain | **Acceptable** — Correctly abstains on season-high minutes; no inferred mood. |

## Concrete error examples

- **Publisher spelling:** the Hornets source says `DAZN`; output says `DAWN`.
- **Speaker change in hook:** Eagles body names PhillyVoice’s tracker; hook says `fans track season from hell`.
- **Editorial suggestion becomes intent:** source says `Mamdani loves Arsenal. He should challenge them...`; output says `Mamdani’s desire for Arsenal to cut ties...`.
- **Winner reversal:** Mayfield’s packet says the Buccaneers beat the Falcons; output says `Mayfield’s historic no-pass loss`.
- **No emotional evidence:** the Buccaneers source supplies only a rare feat; output gives a score of 50.
- **Two negatives become mixed:** Dak’s angry-fans and toughest-loss reactions receive 50.

## Recommendation for the next reviewed edit

Retain the simpler payload, but do not call this a reliability improvement overall. The next change should focus on admitting an emotional claim only when its speaker, subject and reaction are explicit, then scoring those retained claims. Show the exact proposed wording before editing. Publisher spelling, actor/event binding and hook attribution remain separate unresolved issues.

Full outputs and requests are in `recorded-context/`; frozen source evidence is in `recorded-context-cases/`.
