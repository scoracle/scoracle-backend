# Shared form: claim selection and synthesis omission

> Superseded: the claim-selection and story-structure instructions were restored at the user’s direction. See [Scout abstention](studio-abstention-2026-09-20.md). This report preserves the historical omission experiment, not the current contract.

## Change

The shared publishing form asked every character to “Choose the most meaningful claims supported by the evidence” and “Connect the selected findings, evidence and meaning into a coherent read.” These instructions add an analytical task through the presentation layer. They do not literally ask the model to invent a claim and find evidence afterward, but they are a plausible source of pressure to produce a consequential interpretation from insufficient material.

Removed `CLAIM_SELECTION` and the first sentence of `STORY_FORM`. The latter now concerns paragraphs only. Existing evidence boundaries, character scope, character briefs, headline requirements, JSON schemas and validation remain in place. This is a responsibility correction, not evidence that hallucination has been solved. No new guard, inference step or prescribed conclusion was added.

Versioned all six publishing prompts: Scout s51, Momentum momentum-s31, Vibe v35, Insider is14, Narratives n33, Oracle or23. Extraction prompts are unchanged. The existing shared-form integration test rejects the retired directives across all six composed prompts.

## Controlled comparison

Eight direct local calls: Granite 4.2 3B and Qwen3 4B, each with sparse and rich Scout evidence, baseline versus omission. Each pair retains the same character brief, evidence, schema and generation settings; only the two shared directives are removed. The replay retains whitespace around removed sentences; the production composer omits the empty block. The sparse baseline is the captured s50 request from the earlier omission experiment. The rich case uses that same system instruction with the captured rich finding-first request's evidence and generation settings. No finding-first instruction remains in either baseline.

Requests, raw responses, timings and runner are under `logs/studio-form-omission-20260920/`. Calls used the isolated loopback Ollama server on port 11435, not the production inference queue. All eight stopped normally and returned parseable JSON within the card's character ceilings. Shape compliance is not a semantic pass.

| Model and evidence | Baseline failure | After shared-form omission |
|---|---|---|
| Granite, zero goals / unknown xA | Invented disciplined execution, positioning/support role and unchanged profile | Removed that role, but still treated missing assists as limited creation and inferred broad low offensive contribution |
| Granite, rich NBA profile | Reversed poor turnover quality into good ball handling | Turnover direction corrected, but invented hesitation/decision-making causes and understated strong screen assists |
| Qwen, zero goals / unknown xA | Invented defensive/support role, treated missing xA as low creation, misstated 25 games as 2.5 | Still invented defensive/support role, low creation and potential development |
| Qwen, rich NBA profile | Added physical presence, positioning and ball handling under pressure | Still inferred pressure, called screen assists defensive transitions and imposed an overall defense/offense verdict |

None of the four candidate readings qualifies as a grounded quality pass. Removing the shared directives is insufficient; these results do not establish that this language was the sole or primary cause. Scout still explicitly requests a qualitative interpretation and relationships between measurements. Unknown measurements and metric semantics remain active failure cases. The next experiment should isolate those remaining demands rather than add another universal prohibition.

## Related cross-model evidence

Before this experiment, the finding-first candidate was run on all five installed models using the same isolated host. The ten captured results are under `logs/studio-model-comparison-20260920/`. Nine ended with `stop`; Ministral 3 8B's rich case hit the 700-token limit and returned incomplete JSON. All five models produced unsupported interpretations. No result was promoted as a quality pass.

| Model | Examples of unsupported output |
|---|---|
| Granite 4.2 3B | Unknown xA rendered as absent production; offensive boards inferred from defensive rebounds; turnover causes invented |
| Qwen3 4B | Defensive/support role from sparse offensive data; screen assists interpreted as defensive versatility |
| Ministral 3 3B | Usual playmaker role and tactical shift; reversed poor turnover quality; invented comparison cohort |
| Ministral 3 8B | Tactical/positional redefinition; rich response truncated while adding tactical and psychological explanations |
| Ornith 1.5 9B | 25 games called a quarter campaign; invented center cohort, offensive rebounding and blocks from the broader rim-protection label |

Model metadata returned no default system field for any of the five. Earlier alternate-model timeouts on port 11434 occurred on a production server sharing its single loaded-model slot with active worker traffic; they were execution failures, not quality results. The isolated server was stopped after the comparisons. Production routes, processes and databases were not changed.

The planned body-only diagnostic was not executed: its request-edit assertion failed before inference. The user's form-specific hypothesis superseded it; it supplies no evidence about headline effects.
