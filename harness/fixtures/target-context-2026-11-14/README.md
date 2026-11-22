# Target context: 15 paired Journalist and Classifier cases

Completed November 14, 2026. **60/60 model calls completed without transport, timeout or malformed-response errors. No deployment or production writes.** Production prompts were not tuned during this trial.

**Finding: no consistent overall improvement from the target description alone.** There are useful isolated gains, but attribution errors, metadata leakage and repetition remain; several outputs regress.

- [Every Journalist output, before/after, with exact parser errors and editorial review](JOURNALIST-OUTPUTS.md)
- [Every Classifier output, score, route and review](CLASSIFIER-OUTPUTS.md)
- [Machine-readable comparison](comparison.json)
- [Frozen case manifest](manifest.json) and [run design](run-design.json)

## Results

| Check | Without context | With context |
|---|---:|---:|
| Journalist requests completed | 15/15 | 15/15 |
| Journalist strict parser accepted | 3/15 | 1/15 |
| Journalist strict parser rejected | 12/15 | 14/15 |
| Classifier requests completed | 15/15 | 15/15 |
| Classifier headline admission | 15/15 | 15/15 |

Do not read strict acceptance as quality accuracy. The baseline Chelsea output passes despite reversing the match result and duplicating a paragraph. The identical, faithful Van Dijk output fails both times because the parser treats “Liverpool’s Arsenal” in the hook as an unsupported name. The first parser error may also mask other problems.

## Improvements and remaining failures

- **Tatum:** the new context drops the historical Nets trade and avoids blending the Bulls near-triple-double into the Magic-result hook. Some game-story blending remains in the body.
- **Classifier / Eagles:** emotion falls 16.04 → 14.06 and the Smith/Jones story stops routing to Influencer. Subject remains 91.90, so the incidental Eagles mention still routes to Journalist. Seahawks broadcaster tension remains admitted at 95.55 subject.
- **Mayfield:** baseline consistently reports a Tampa Bay win. New context says “Week 8 loss” and then says the team won.
- **Seahawks:** new context eliminates baseline repetition but invents Minnesota Vikings where reporting names the Bears.
- **Miami:** metadata leakage occurs in both; new context repeats the same paragraph eight times in a ten-paragraph output, versus one baseline paragraph.
- **Roma:** both replace Gasperini with another coach—Inzaghi without context, Conte with context—and interpret “wounded Red Devils” as physical injuries.
- **Classifier / Tatum:** historical trade stays falsely admitted, with subject 79.58 → 82.14 and movement 85.48 → 87.63.
- **Classifier / Mayfield:** a no-pass result remains misread as availability (74.93 → 81.56); movement also rises 28.75 → 49.35, despite no personnel move being reported.

The only routing change among all 15 Classifier pairs is the Eagles-to-Influencer change. Scores are native model scores; the runner had no gold labels, so its zero label mistakes is not an accuracy result.

## Scope and reproducibility

This is a controlled **context-only ablation of the current workspace prompts**, not a comparison against every deployed configuration difference. In particular, Classifier uses the workspace v13 criteria in both variants. Fifteen frozen targets from the Influencer relevance trial cover eight teams and seven players across NBA, NFL and European football. Journalist receives each complete packet through its current evidence selector. Classifier receives one preselected headline per target, using canonical identity plus available recorded attributes as background. No affiliations or gender were inferred.

Both variants have identical inputs and options apart from the description; paired payload equality was checked. Contract version labels are deliberately kept equal in the baseline fixtures to satisfy the harness, with the variant identified by directory. Baseline binaries were built in an isolated temporary source copy; no production source was reverted. Current instruction, criteria, routing and shared-description snapshots are in `source-snapshot/`. Raw requests, responses, prepared token coverage, timing, generated output and errors are under each run directory. Exact model inventories are in `journalist-model.json` and `classifier-model.json`.

Single generation per variant, temperature zero for Journalist, no retries. Shared production model servers were used; timings are not dedicated benchmarks. The packets were originally selected for Influencer and contain difficult/vague evidence; this is not a population-level quality estimate. Classifier labels and editorial notes are reviewer judgments against the supplied evidence, not independently adjudicated gold labels.

Setup error: local tunnel port 18081 was occupied. A separate temporary tunnel on 28081 was used successfully; no model request failed as a result. The tunnel is closed after the trial. Production workers and deployment configuration were unchanged.
