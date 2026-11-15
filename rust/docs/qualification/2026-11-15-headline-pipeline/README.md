# Headline pipeline sample qualification — November 15, 2026

Three synthetic RSS items ran through the real Go intake, live Classifier, durable queue, live Journalist/Influencer/Insider, publication transactions, and Go API. They used an isolated PostgreSQL database (`classifier_test_coverage_samples2`), reserved `example.invalid` URLs, and the existing model servers. No sample reached production and no Google request was made.

The samples cover an injury announcement, supporter sentiment, and a transfer from Chelsea to Arsenal. Fixture entities include the player's current club. The first exploratory fixture omitted that context; the clean replay includes it.

## Results

- Three articles acquired; five target-specific Classifier spectra created.
- During coverage, seven Character jobs stayed pending and no Character product was generated.
- Simulated end-of-day transitions released only `narratives`, `vibe`, and `transfers`.
- Seven generations published under `scoracle.character`: three Journalist, one Influencer, three Insider. Two subjects without sentiment evidence received explicit empty outcomes.
- The final edition completed with zero recorded failures and advanced its publication pointer. Nine assignments for inactive skills remained pending without blocking completion.
- Real API responses passed source assertions: Journalist cited articles 1–3; Influencer cited article 2; Insider linked article 3 to Arsenal with incoming direction.

The sample uses simulated clock transitions; it does not establish completion time for a full day's production load or Google availability. It uses the unchanged production observation policy and expiry. Content observations remain visible in `content-observations.jsonl`; this is pipeline qualification, not a claim of perfect wording or model fidelity.

## Scheduler fix

The first run exposed a preexisting mismatch: the scheduler waited only for required skills but judged completion against every skill. Migration 334 makes the completion check use the same required-stage set. The regression fails on the old function and passes on the fixed function. All ten daily scheduler integration tests passed. No prompt, parser, model or retention policy changed.

`sample-intake_test.go` is the acquisition fixture used through a temporary Go overlay. API JSON files and `result.json` are from the clean replay. The worker used `classifier,narratives,vibe,transfers`, one claim at a time, local Classifier port 18081 and Character port 8081. The existing production binary was used; the runtime change is the database scheduler function.
