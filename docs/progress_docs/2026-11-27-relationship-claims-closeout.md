# Bayern Munich and Dallas Cowboys production comparison

## Decision

Retire the classifier-owned relationship-claims experiment. In this six-card replay, reusing saved claims made the writers faster, but lead coverage fell from 14/18 to 9/18. Both systems misidentified a Dallas player’s position, and the branch accepted a Bayern writer’s refusal as a finished card. The experiment did not meet the accuracy and coverage goals for a production rollout.

On November 27, 2026, the relationship branch produced two fully grounded cards out of six, compared with zero in the published production sample. It also dropped five lead stories that production covered and returned one unusable card. The clearest gain was keeping the Dallas Influencer’s Packers story separate from Dallas. The clearest regression was the Bayern Influencer rejecting the supplied evidence.

[Paired cards, source headlines, prepared evidence, review and timings](bayern-dallas-2026-11-27.json)

## Throughput

Six cards, one local Qwen3.5-4B-Q4_K_M server, sequential requests. Run order was production replay, branch with fresh claims, branch with saved claims, then production replay again. Times sum the measured work for the six cards. Cards per minute counts completed writer responses, including the Bayern refusal.

| Path | Preparation | Writing | Total | Cards/min |
| --- | ---: | ---: | ---: | ---: |
| Production requests, first replay | — | 96.2 s | 96.2 s | 3.74 |
| Branch, fresh claims | 68.7 s | 75.5 s | 144.1 s | 2.50 |
| Branch, saved claims | 0.05 s | 64.1 s | 64.2 s | 5.61 |
| Production requests, repeat | — | 124.3 s | 124.3 s | 2.90 |

| Three cards per team | Production replay 1 | Branch, fresh | Branch, saved | Production replay 2 |
| --- | ---: | ---: | ---: | ---: |
| Bayern Munich | 46.1 s | 92.4 s | 34.6 s | 62.7 s |
| Dallas Cowboys | 50.1 s | 51.7 s | 29.6 s | 61.5 s |

The 54 report slots contained 49 distinct source revisions. The classifier’s claim reader called the model for eight Bayern and two Dallas sources on the first pass, then zero on the second. The branch writers generated 1,210 tokens in each run. The production replays generated 1,964 each. Shorter copy and the refusal contribute to the writing-speed result. The live production ledger recorded 89.9 seconds across the six original calls on its own host.

## Accuracy

We reviewed the actual published headline and body against each card’s frozen packet. A clean card has no material unsupported actor, role, event, timing, or outcome. A mixed card contains usable reporting and at least one such error. An unusable card fails to deliver sports copy. These are card grades, not percentages of individual statements that are true.

The coverage check uses the first three distinct source leads in each packet, selected before inspecting branch outputs. Each gets one point if the card conveys its central event and actor. Extra unsupported details count against the clean-card grade. For example, “Dallas activates Thompson” earns coverage even if the writer invents his position. The Aikman lead specifically concerns two historical contracts, so reporting a recent ESPN signing earns no point.

| Team and character | Production → branch | Leads covered, out of 3 | Difference that matters |
| --- | --- | ---: | --- |
| Bayern Insider | Mixed → Mixed | 3 → 3 | Separates Eberl leaving January open from Freund ruling out a spree. Still identifies Manchester City as the party imposing a sanction without source support. |
| Bayern Journalist | Mixed → Mixed | 2 → 2 | Preserves Kompany’s view that the competition is still open and removes the claim that January recruitment ended. Still presents a planned €80m investment as underway. |
| Bayern Influencer | Mixed → Unusable | 2 → 0 | Production assigns an outlet’s injury criticism to Kompany. The branch rejects both the Lille match and Kompany’s role, despite the supplied reporting and a retained coaching relationship. Its headline is the literal text “null”. |
| Dallas Insider | Mixed → Clean | 2 → 1 | Correctly reports Thompson’s activation. Avoids production’s “recent” Aikman signing by dropping the contract story entirely. |
| Dallas Journalist | Mixed → Mixed | 3 → 2 | Both call Thompson a wide receiver. His roster positions are DB and Safety. The branch also turns a touchdown that extended a lead into a Dallas victory. |
| Dallas Influencer | Mixed → Clean | 2 → 1 | Correctly attributes Eagles fan anger and keeps the Parsons incident with his Packers teammates. Drops the Thanksgiving result and includes a pregame forecast. |
| **Total** | **0 → 2 clean** | **14/18 → 9/18** | **The two clean branch cards each cover one of the three leads.** |

## Scope and verification

All six production headlines and bodies matched the served API text. Five cards covered November 26. The latest served Bayern Influencer card covered November 25, and both sides received that card’s November 25 evidence. The comparison is against each character’s published card. Accumulated historical transfer-feed entries are outside the scored output.

The production timings replay the archived native model requests. The branch used commit `94e756d8`, a disposable Postgres database, the production source identities, the retained Kompany–Bayern relationship, and the normal character writers. Each writer received the same ordered publisher headlines as its published counterpart. The two branch runs returned identical cards. The Dallas Journalist packet contained zero prepared actions and zero coaching contexts. The retained Dallas roster establishes Thompson’s position for grading, but his position was absent from that writer packet.

Production timing covers the local model request. Branch timing includes source preparation, graph attachment and character execution inside the replay. Capture, initial classification and source ranking were held fixed. Publication, queue wait and concurrent production load were outside the test. One six-card sample measures these cases, and the spread between the two production replays shows why the speed figures should be read as a range.
