# Classifier concise instructions: 15-case review

Applied the approved wording and `target_context → target → task → evidence_context → evidence` layout. Native scoring questions remain separate from the ordered JSON string state. Versions: criteria v14, request v5. No further prompt edits after testing; no deployment.

[Full prompts, all four questions, native outputs, exact errors and per-case comparisons](FULL-PROMPTS-AND-OUTPUTS.md) · [Machine-readable comparison](comparison.json) · [Raw run receipts](results.jsonl)

## Result

- **15/15 completed, zero request or decoding errors.** Same eight teams and seven players, frozen inputs and exact d3 model fingerprint as the previous with-context trial. No retries.
- **411 library tests passed, 75 ignored.** Classifier-focused tests: 15 passed, 10 ignored. Database-dependent ignored tests were not run.
- **All 15 remain admitted.** Fourteen route sets are unchanged; Eagles now routes to Influencer again, a regression for its incidental commentator headline. No thresholds changed: subject 55, emotion 15, movement 80, availability 85.
- Native reported input tokens average **2,563.9 → 2,188.4** (14.6% lower). Scores are native probabilities scaled to 0–100, not measured accuracy. The replay runner had no gold labels; its zero label-mistake count is not evidence of correctness.

## What improved

Unsupported availability scores fell sharply on Mayfield (81.6 → 14.3), Miami (66.3 → 1.7), Prescott (42.0 → 1.5), and Krejci (25.3 → 2.6). These previously remained below the Scout cutoff, so the better scores do not change those routes. Mayfield’s unsupported movement score also fell 49.3 → 24.9. Clear target feelings remain recognized: Van Dijk emotion rises 75.5 → 93.3 and Prescott 68.5 → 78.0.

## What still fails or regresses

Eagles commentator coverage drops from 91.9 to 62.2 subject, but still clears admission, and emotion rises from 14.1 to 25.0, restoring Influencer routing. Seahawks broadcaster tension remains admitted at 80.7 subject. Tatum’s historical trade story drops to 56.7 subject, still just above admission, while movement remains 87.0 and emotion rises to 60.6. Neutral Krejci playing-time reporting still routes to Influencer at 22.8 emotion. Roma and Mayfield still show uncertainty about whose emotion is being classified.

The lower availability scores are not universally gains: Arsenal’s red-card headline drops from 67.5 to 12.6, which may lose a useful participation signal. Hornets’ “without LaMelo Ball” headline drops 79.5 to 1.8, but the headline alone does not establish the reason for absence. We need actual positive availability examples before concluding this dimension is calibrated.

## Scope

This compares the previous **v13 with-context** trial against the newly approved **v14 wording plus JSON layout**. Both changed together, so this cannot isolate wording from presentation. One pass per case, no independent gold-label adjudication, and this set comes from the earlier Influencer evidence corpus rather than a representative sample of news.

Input case bytes were verified unchanged. Target identity, supplied category, named-person data and sanitized background values are preserved; the legacy text background is now named `recorded_attributes`, and target `type` is now `entity_type`. Missing attributes are omitted. No extra interpretations were manufactured from background text. The shared target description is unchanged.

Every case directory contains the new request, decoded state and complete response, plus the previous request and response. `source-snapshot/` freezes the exact instructions and assembly used. Model inventory and manifest are saved alongside. The temporary SSH tunnel was closed; production workers were unchanged.

## Per-case findings

- **01-charlotte-hornets:** Correct subject and booing signal remain. Availability falls 79.5→1.8; headline says without Ball but does not explain whether this is injury, absence or personnel movement. This drop is not automatically a quality gain.
- **02-philadelphia-eagles:** Subject improves directionally (91.9→62.2), but incidental Smith/Jones commentary still clears admission at 55. Emotion rises above 15, restoring the unwanted Influencer route. Routing regression.
- **03-seattle-seahawks:** Broadcaster tension remains wrongly treated as a Seahawks development. Subject falls but stays well above admission; emotion also rises.
- **04-arsenal:** Match outcome remains relevant. Movement remains unsupported at 59.1, although below Insider routing. Red-card participation signal falls 67.5→12.6: possible lost availability sensitivity, not a blanket improvement.
- **05-roma:** Availability false signal falls 58.3→4.1. Emotion rises 55.3→76.1 despite an unresolved relationship between Allegri’s anger and the Roma target; relationship handling is not solved.
- **06-miami-heat:** Criticism of Heat teammates remains relevant. False availability falls 66.3→1.7, but unsupported movement rises 36.1→50.6. No route changes.
- **07-tampa-bay-buccaneers:** Concrete record remains relevant. Emotion falls 58.6→29.1, but the vague rare-feat headline still routes to Influencer without an explicit feeling.
- **08-chelsea:** Chelsea personnel interest remains correctly routed to Insider. Availability falls 39.5→6.2. Emotion rises 35.1→65.5: sweating over a future is being read more emotionally, although the headline does not establish a named speaker’s feeling.
- **09-luka-doncic:** Direct praise stays correctly attributed to the target dimension; subject/emotion remain high. No route changes.
- **10-jayson-tatum:** Historical 2013 trade still falsely admitted: subject falls 82.1→56.7 but remains above 55. Movement stays 87.0, above Insider’s 80 cutoff. Emotion rises 21.8→60.6. Historical context still treated as current classification material.
- **11-dak-prescott:** Prescott reaction and fan anger remain correctly relevant. Emotion strengthens 68.5→78.0; unsupported availability falls 42.0→1.5.
- **12-baker-mayfield:** False availability improves 81.6→14.3 and movement 49.3→24.9. Emotion rises 56.5→72.3 despite wording centered on the Falcons’ infamy/loss, leaving a target-ownership concern.
- **13-virgil-van-dijk:** Clear target reaction strengthens 75.5→93.3. Unsupported availability falls 26.2→1.9. Existing correct route preserved.
- **14-gessime-yassine:** Direct performance evaluation remains relevant; emotion is slightly stronger. Availability falls 17.1→0.9, consistent with no participation issue in headline.
- **15-vit-krejci:** Playing-time result remains relevant. Availability improves 25.3→2.6, but emotion remains 22.8 and still routes to Influencer despite no explicit feeling.
