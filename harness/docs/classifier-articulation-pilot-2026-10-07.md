# Classifier-to-expression pilot — October 7, 2026

**The independent Classifier replay can feed expression tests now. None of the three articulators is ready for promotion on the current raw emotion packet.** Smaller models generated prose, but source fidelity and attribution failed in both smaller candidates and the existing 3B baseline. The full spectrum alone did not fix those failures.

This is a diagnostic run with AI source inspection, not independently adjudicated accuracy or a production acceptance test. The 400-source review packet still has zero eligible training/dev/test units. Additional heads, validated claim extraction and production Classifier registration remain unfinished.

## What ran

Ten unchanged inputs: eight fictional controls and two previously measured retained reports, covering Melton availability and Daniels' absence. Each model received source-only input and the same source with Horizon small's complete 28-label emotion vectors: **60 calls**. No expected review notes entered the model input. All sources, windows and measurements were hash-bound; the original publisher text remained intact.

The packet names one candidate query target, retains publisher identity/date and source references, and supplies no history. Spectrum inputs declare document-window scope, raw uncalibrated scores, exact window byte ranges and checkpoint revision. Relevance, topic, discourse, event-time classification, claim qualifiers and target ordinals remain explicitly unknown. Scores describe text windows; they do not identify whose feelings are present or whether those feelings are denied or historical.

All trials used the same emotional-reading task and JSON schema, temperature 0, seed 42, context 4,096, output budget 400, thinking disabled and one call without repair. This prototype does not implement the existing Influencer publication schema or invent its missing valence score. No output was published.

## Model results

| Installed tag | Stored parameters / precision | Structurally valid replies | Material diagnostic failures |
|---|---:|---:|---|
| `alibayram/smollm3:latest` | 3,075,098,624 / Q4_K_M | 20/20 | Historical hope made current; another team's player assigned to Cedar; raw scores turned into percentages about club performance. |
| `smollm2:1.7b` | 1,711,376,384 / Q8_0 | 18/20 | Two spectrum replies reached the output limit with malformed JSON; one player's feelings generalized in headlines; task instructions leaked into prose; an unreported game appeared in the real-source output. |
| `qwen3:1.7b` | 2,031,739,904 / Q4_K_M | 20/20 | Another team's win assigned to Cedar in a headline; clear attributed emotions treated as ambiguous; invented fan/analyst discussion in the real-source output. |

The Qwen tag says 1.7B; the installed GGUF reports approximately 2.03B stored parameters. Both new candidates are below the current baseline's measured size. Quantization differs, so this does not isolate model size as the cause of any result. Valid JSON includes semantically unfinished paragraphs; structural validity is not factual or editorial acceptance.

The [source-free receipts and provisional review](../fixtures/classifier/articulation-pilot-2026-10-07.json) contain installed model digests, exact file hashes, all 60 review records and fictional outputs. Complete real-source packets, requests and responses remain private on Archbox under `/mnt/data/backup/scoracle/classifier-launch/2026-10-07/articulation-v1/`, with local copies under `/private/tmp/scoracle-classifier-launch-20261007/articulation-v1/`. The inspection records concerns rather than claiming a sports accuracy percentage; absence of a flagged concern is not independent approval.

## What the controls revealed

- **Relevance and abstention:** none of the models returned null for the routine, explicitly emotion-free schedule. Qwen correctly abstained on the unrelated entity with source-only input, then assigned the unrelated win to Cedar after adding the spectrum. SmolLM2 correctly abstained on that case only with the spectrum. There is no stable relevance boundary yet.
- **Speaker and subject:** all three made attribution errors. On the two-speaker control, the baseline and Qwen assigned Maple's Robin to Cedar with spectrum input. SmolLM2's spectrum answer preserved both identities, while its source-only answer added doubt about clearly quoted feelings.
- **Negation:** denial was usually retained as wording, but Qwen added unsupported outcome uncertainty. SmolLM2's spectrum denial answer hit the output limit. None of this validates a learned claim-qualification head.
- **Time:** the baseline's spectrum headline made old hope current and its body presented raw scores as club emotion percentages. Qwen's source-only version invented a publication date and current-team interpretation. SmolLM2's spectrum version retained the old date and absence of a new statement.
- **Mixed feelings and sarcasm:** several outputs preserved the individual speaker and opposing feelings. Others changed the object of worry or added collective expectations and team disengagement. These distinctions need exact claim support, not just more emotion labels.
- **Real reporting:** the availability report acquired unsupported player emotional intent, author opinion became player emphasis, and several outputs ended mid-sentence despite valid JSON. The absence report exposed reassigned team hope, misplaced injury timing and an invented opponent/game. No real-source acceptance claim follows from two inspected articles.

Raw spectrum inputs also increased prompt size: source-only maxima were 709, 758 and 702 tokens; spectrum maxima were 2,191, 3,239 and 3,150 respectively. All were below the configured context limit, including the reserved 400 output tokens (maximum combined budget 3,639 tokens). SmolLM2's two malformed replies reached the 400-token generation limit, not the input limit. A usable packet should select supported measurements and their attributed evidence while keeping full vectors stored for inspection; passing every raw score to every character is not automatically useful.

## Runtime and verification

The cognition worker competed with the baseline and early SmolLM2 requests, causing repeated model loads. It was temporarily stopped to finish the isolated comparison, then restarted. Cognition, API and Laya services all reported active afterward; production model routes and configuration were unchanged. This was a benchmark pause, not the Classifier cutover.

Median decode times were approximately 956/942 ms for baseline source-only/spectrum, 714/969 ms for SmolLM2 and 922/1,369 ms for Qwen. Residency, precision, tokenizer and generated length differ; these are configuration receipts, **not a fair model-speed ranking or production throughput result**. Baseline median load times exceeded 10/22 seconds under contention, so total wall time is particularly unsuitable for that comparison.

The new [stdlib replay](../examples/classifier_articulation.py) imports no Harvester/Editor code and calls no Laya protocol. Shared character `SourceContext` now lives in `tools::source`, with all consumers updated and no Harvester compatibility re-export. Live delivery queries and stage ownership still await replacement.

The Rust library passes **368 tests, 64 ignored**. All four Python contract checks pass in Archbox's existing model runtime; local Python skips the torch-dependent check. They verify source binding, complete vectors, unknowns, evidence/head identities, consistent abstention and masking of unknown labels. They do not prove model accuracy. The replay exits nonzero when a model reply is malformed, retaining both failed receipts alongside the 58 structurally valid replies.

## Next decision

Keep the smaller candidates available, with no winner or deployment change. Independently review extraction and source groups in the 400-source packet, train/compare target relevance/topic and document discourse/time heads, and validate exact-span speaker/subject/negation/time relationships. Then repeat this same diagnostic on a compact world assembled from supported claims and useful measured signals, followed by held-out real-source evaluation. More dimensions alone cannot repair attribution; the character must receive correctly qualified evidence.

The Classifier remains a separate plugin. Its production stage replaces Harvester and Editor directly once the selected measurement slice is usable; it does not call their gate, thresholds, model server or delivery reader.

Candidate capabilities were checked against the primary [SmolLM2 model card](https://huggingface.co/HuggingFaceTB/SmolLM2-1.7B-Instruct), [Qwen3 model card](https://huggingface.co/Qwen/Qwen3-1.7B) and [Ollama chat API](https://docs.ollama.com/api/chat). Those descriptions support testing the candidates, not a Scoracle fidelity claim.

The user's research handoff adds two unmeasured future candidates: [Pleias-RAG-1B](https://huggingface.co/PleIAs/Pleias-RAG-1B), described as 1.2B, and [Pleias-RAG-350M](https://huggingface.co/PleIAs/Pleias-RAG-350M). Both cards declare Apache 2.0, source summarization/citation training, a custom special-token query/source format and official GGUF releases. The [paper](https://arxiv.org/html/2504.18225v1) reports 4,096-token training context, primarily multi-hop question-answer benchmarks, and persistent drift when requested information is missing. These are candidates for later evidence-synthesis evaluation, not a model swap or new run in this pilot. The handoff reports all current plugin contexts as 4,096; future tests must budget evidence, instructions, generated reasoning and final output together and use the model's documented input format.
