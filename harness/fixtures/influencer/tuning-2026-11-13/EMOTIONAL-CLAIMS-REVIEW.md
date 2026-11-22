# v58 emotional-claim admission: 15-case review

Applied only the user-approved admission and scoring wording. Compared with v57, the target, metadata, content context and evidence are identical in all 15 fixtures. Model Qwen3.5-4B-Q4_K_M, temperature 0, thinking disabled, context 10000, output limit 3500 and native schema are unchanged.

**All 15 completed and passed mechanical parsing.** No output truncation. The stored-fixture reassembly test passed. Manual review finds targeted gains but continuing actor/event/outcome errors and new regressions. Production remains paused; no subsequent prompt edits or deployment were made.

| Case | v57 | v58 | Finding |
| --- | ---: | ---: | --- |
| 01 Charlotte Hornets | 50 | 25 | **Mixed** — Publisher spelling improves: DAWN becomes DAZN. New hook makes fans the recipients of criticism aimed at the team. Five sentences remain in one paragraph. |
| 02 Philadelphia Eagles | 25 | 25 | **Still fails** — Negative score stays 25. Hook still invents fans mixing hope and misery. Body changes a player described as a disappointment into disappointment over a trade idea. |
| 03 Seattle Seahawks | 25 | 25 | **Mixed** — Restores the positive trade reaction omitted by v57 while retaining a defensible negative overall score. Distorts the Aikman/Buck reaction into a Troy Aikman moment of dislike and retains unsupported one-fan specificity. |
| 04 Arsenal | 50 | 65 | **Regression** — Body explicitly reports Arsenal’s defeat, but hook reintroduces upset win. Editorial suggestion is still attributed as Mamdani’s own desire. Adds Carragher’s former-player background absent from supplied packet. |
| 05 Roma | 50 | 45 | **Regression** — Hook adds some see Roma’s win, unsupported by the draw and Manchester United material. The literal injured/wounded mistake remains absent, but event/outcome ownership is still wrong. |
| 06 Miami Heat | 25 | 45 | **Regression** — Changes Bucks winning before Miami into Miami securing a win. Score remains below 50 (45), but the underlying fact is reversed. |
| 07 Tampa Bay Buccaneers | 50 | abstain | **Improved** — Correctly abstains instead of assigning 50 to a rare statistical feat. Directly fixes the intended emotional-claim admission failure. |
| 08 Chelsea | 25 | 25 | **Still fails** — Still asserts Manchester United’s victory and labels Chelsea as struggling while the quoted misery belongs to Manchester United. Adds transfer-window context not supplied. |
| 09 Luka Doncic | 50 | 75 | **Mixed** — Score returns from 50 to 75 and awesome-defense wording is no longer assigned to Redick. Drops the distinct MVP-ranking frustration thread, so improved polarity comes with lost emotional coverage. |
| 10 Jayson Tatum | 75 | abstain | **Improved** — Correctly returns to abstention rather than turning statistics into celebration and importing the Nets’ haunted framing. |
| 11 Dak Prescott | 50 | 25 | **Improved** — Two negative reactions now score 25 instead of 50. Drops unsupported head-coach specificity. Shouting and the contrast connector remain unsupported or misleading embellishments. |
| 12 Baker Mayfield | 25 | 45 | **Still fails** — Still attaches Falcons’ horrific-loss framing to Mayfield, adds lack of progress and ties Chiefs’ collapse into his emotional reading. Hook still refers to a no-pass loss. |
| 13 Virgil van Dijk | 25 | 25 | **Stable acceptable** — Preserves explicit Van Dijk disappointment and score 25. Body correctly states Liverpool lost to Arsenal. |
| 14 Gessime Yassine | 75 | 75 | **Stable acceptable** — Keeps a narrow, attributed positive publisher framing and score 75, without inventing player feelings. Hook now names the publisher too. |
| 15 Vit Krejci | abstain | abstain | **Stable acceptable** — Correctly abstains on playing-time statistics. |

## What the approved wording fixed

- Buccaneers and Tatum return to appropriate abstention.
- Dak’s two negative reactions score 25 rather than 50.
- Hornets preserves DAZN’s name; Doncic’s score is positive again, but loses the MVP-frustration thread.

## What remains unresolved

- Who experiences the feeling, who is being described, and how that entity relates to the target.
- Editorial suggestions are still converted to a named person’s intent.
- Winners and losers can flip, including between body and hook.
- Source labels and emotion alone do not reliably preserve actor/event binding.

These results support testing evidence presentation, but do not prove a presentation change will fix the failures. Any next experiment should hold these instructions fixed and vary only how evidence and its relationship to the target are represented. Manual relationship annotations would be a diagnostic upper bound, not a deployable upstream capability.

All raw requests and outputs: `emotional-claims/`. Exact fixtures: `emotional-claims-cases/`. Approved wording snapshot: `emotional-claims-instructions.rs`.
