# Publisher palette rollout and Granite versus Smol probe — September 26

## Implemented boundary

The published prose paths for Scout, Analyst, Influencer, Journalist, Insider's wire wrap, and Oracle now use the Studio palette contract. Each plugin prepares finite factual statements and alternate approved phrasings. The model returns positional indexes. Studio rejects an extra field, missing slot, or out-of-range index; the raw model text is never served. The plugin also supplies each headline and numeric score. All live palette calls use temperature zero and a 160-token output reservation. The earlier free-text parsers remain for archived fixtures, whose evaluation task versions are pinned to their old prompts.

| Product | Plugin-owned output material | Numeric policy |
| --- | --- | --- |
| Scout | Measured percentiles and recent form | Existing measured rating |
| Analyst | Selected Scout and Influencer statements, computed direction | Existing trajectory computation |
| Influencer | Mood register and quoted packet details | Fixed register scale, averaged across packets |
| Journalist | Up to three attributed source items, cited by durable article ID | Existing volume, corroboration and recency impact formula, clamped to 1–99 |
| Insider wire wrap | Up to three recorded counterparty, stage, direction and heat rows | Maximum active heat, or 1 for an empty board |
| Oracle | One selected claim per available pillar plus computed omen | Rounded mean of available narrative impact, rating notability, mood and transfer heat; 50 when only directional material exists |

The Insider's transfer-pair product now bounds a positive model verdict to a direct source article containing the named player, team and an explicit move cue. The published summary copies the source title with attribution; stage comes from literal source cues; the model's summary, stage and probability are discarded. A positive model verdict with no matching source becomes UNKNOWN. This is a conservative English cue gate. It can miss paraphrases, aliases and non-English reporting, and the model can still suppress an eligible rumor. A later typed transfer-event feed should replace this transitional gate. Identity adjudication remains a separate state-changing review and is not covered by the palette.

Investigator's Wikipedia extraction is already exact-substring checked before its typed decision. Graph extraction and the ongoing Harvester cutover are separate internal evidence paths. Graph's relation predicates still depend on model classification; downstream products must treat them as candidate context, not verified source facts. The Harvester integration may change where Journalist and Influencer obtain their source material; it does not change the published palette boundary.

## Local model probe

`examples/palette_model_compare.py` used an isolated Ollama instance on port 11435 and the same schema, temperature, context window and prompt for both installed model blobs. Seven synthetic palette cases covered sparse and multi-measure Scout cards, an Oracle five-pillar card, a three-source Journalist edition, and an Insider wire. Three calls per case produced 21 calls per model. Raw calls were captured locally at `/private/tmp/scoracle-palette-models-20260926.jsonl`.

| Model | Valid choices | Median call | Oracle median | Journalist median |
| --- | ---: | ---: | ---: | ---: |
| `granite4.2:3b` | 21/21 | 0.969 s | 1.293 s | 0.866 s |
| `alibayram/smollm3` | 21/21 | 0.754 s | 1.046 s | 0.712 s |

Smol's median was about 22% lower in this local run and lower in every case. Both models made the same choice on each repetition of a case. Granite mixed phrasings in the Scout cases; Smol generally picked one phrasing index throughout each case. This measures palette compliance and local latency, not sports judgment or editorial quality. The installed Smol package is a community Ollama build. The user subsequently selected Smol for production; see the switch record for deployment state. The next quality improvement is better plugin-approved phrasing, especially the plain Oracle and Journalist cards.

## Verification

The Rust library suite passed with 564 tests and 64 ignored after the migrations. Focused prepared-creation tests verify Studio provenance, code-owned scores and source-bound pair verdicts. No database writes or production model-route changes were made by the benchmark.
