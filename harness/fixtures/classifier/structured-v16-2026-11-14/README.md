# Classifier nested attributes: 15-case review

The Classifier now uses the Journalist’s structured database-record loader and recorded-attribute formatter. `recorded_attributes` is a nested JSON object. Legacy prose-only records remain under `background_note`; arbitrary prose is not parsed into invented fields. The task now begins “You will be given an entity…” and retains task-first ordering.

[All 15 full prompts, native outputs, errors and comparisons](FULL-PROMPTS-AND-OUTPUTS.md) · [Comparison JSON](comparison.json) · [Raw receipts](results.jsonl)

[Player payload: Virgil van Dijk](13/state.json) · [Team payload: Tampa Bay Buccaneers](07/state.json)

## Outcome

**15/15 requests completed; zero transport or decoding errors. 413 library tests passed, 75 ignored. No deployment, retries, production writes or post-run prompt tuning.**

| Case | Last run | Current | Assessment |
|---|---|---|---|
| Eagles commentator story | Admitted, Journalist + Influencer | Subject 27.7; not admitted | Improved incidental-mention filtering |
| Seahawks broadcast tension | Admitted, Journalist + Influencer | Subject 51.7; not admitted | Improved; close to 55 admission threshold |
| Tatum historical trade | Admitted, Journalist + Influencer + Insider | Subject 44.8; not admitted | Improved admission; movement still incorrectly high |
| Chelsea personnel interest | Journalist + Influencer + Insider | Movement 77.7; Journalist + Influencer only | Regression: misses Insider cutoff of 80 |

The other 11 routing decisions are unchanged. Admission falls from 15/15 to 12/15. The three rejected cases match the relevance concerns identified in the earlier review. This is an improvement on this selected set, not an independently measured general accuracy gain.

Remaining problems include neutral Krejci playing-time reporting still routing to Influencer; historical Tatum movement still scoring 85.4 even though admission blocks it; uncertain ownership of emotional framing for Roma and Mayfield; and weak Arsenal availability sensitivity despite the red-card headline. Chelsea’s missed Insider route shows that the new presentation also weakens a useful positive signal. No thresholds were adjusted (subject 55, emotion 15, movement 80, availability 85).

## Comparison scope

Baseline is the **last live v14/v5 run**, before the task-first sequencing change. Current is **v16/v7**, including task-first ordering, moving relationship guidance alongside native questions, nesting attributes, and removing “sports-related” from the task. There was no intermediate live task-first run. Therefore these results cannot attribute gains specifically to nested JSON or the shorter qualifier.

Same 15 targets (eight teams, seven players), headlines, publishers, recorded attribute values and model fingerprint. Cases now carry their original frozen structured database record instead of relying on its text rendering. Every resulting nested attribute object was checked against the previously embedded JSON values, and every current exported request was checked against the actual submitted receipt. Compared with the immediately preceding task-first payload export, only the requested task qualifier and attribute representation changed; native scoring questions remain identical.

Native reported input tokens average 2,188.4 → 2,249.2 versus the last live run (2.8% higher). One pass per case, same d3 fingerprint, shared server; timings are not a dedicated benchmark. No gold labels were supplied, so the runner’s zero label mistakes is not an accuracy result. The 75 ignored tests, including database-dependent integration tests, were not run; live replay uses frozen data and does not test production database acquisition.

Exact source snapshots, model inventory, manifest and per-case current/previous native requests and responses are retained here. Previous experiment artifacts remain unchanged. The temporary test tunnel is closed after the replay.

## Per-case review

- **01-charlotte-hornets:** Correct relevance and booing signal remain; no route change. Absence of Ball has unclear cause in this headline, so the low availability score is not independently adjudicated.
- **02-philadelphia-eagles:** Improvement: incidental Smith/Jones commentary no longer admitted. Subject falls below 55, blocking both Journalist and Influencer even though emotion increases.
- **03-seattle-seahawks:** Improvement: broadcaster tension no longer admitted as a Seahawks development. Subject 51.7 is close to the 55 cutoff, so this one pass does not establish robust separation.
- **04-arsenal:** Match result remains relevant. Unsupported movement decreases but red-card availability remains weak; no route change.
- **05-roma:** Remains admitted and emotionally classified. The relationship between Allegri’s anger and Roma remains insufficiently resolved; no route change.
- **06-miami-heat:** Teammate criticism remains relevant. Unsupported movement decreases; emotion falls to 18.5, close to the 15 cutoff despite explicit criticism. No route change.
- **07-tampa-bay-buccaneers:** Concrete record remains relevant. Vague rare-feat wording still routes to Influencer without explicit feelings; no route change.
- **08-chelsea:** Regression: reported interest in Acheampong is personnel news for Chelsea, but movement falls from 88.7 to 77.7, below the Insider threshold of 80. Journalist/Influencer remain enabled.
- **09-luka-doncic:** Direct praise remains correctly recognized; no route change.
- **10-jayson-tatum:** Improvement at admission: historical 2013 trade now fails subject threshold, blocking all routes. Movement remains high at 85.4, so the movement question itself still mistakes historical trade material for current movement.
- **11-dak-prescott:** Correct relevant anger remains routed. Emotion falls substantially but stays above threshold; no route change.
- **12-baker-mayfield:** Unsupported movement decreases; availability remains low compared with pre-concise trials. Emotion still risks attaching Falcons-centered framing to Mayfield; no route change.
- **13-virgil-van-dijk:** Clear target disappointment remains strongly recognized; no route change.
- **14-gessime-yassine:** Direct target performance judgment remains recognized; no route change.
- **15-vit-krejci:** Playing-time result remains relevant. Emotion 22.4 still routes neutral playing-time reporting to Influencer; availability increases but remains below Scout threshold.
