# Classifier spectrum comparison — October 7, 2026

**Latest decision:** Laya is back in the comparison at the user's instruction. The GPU baseline below is complete; further qualification inference is paused while we investigate public Jev/Laya classification implementations. No model is promoted. See the [current launch ledger](scoracle_classifier_plugin_launch.md#execution-ledger).

Laya can implement a continuous Classifier spectrum without generating text. The installed English checkpoint is not yet a reliable sole gatekeeper: this comparison found negation, target and time errors, including with the documented alternative question format. Compact emotion specialists were much faster in this CPU configuration, but their document emotion vectors do not solve relevance or attribution by themselves.

**Decision:** carry Horizon small and SamLowe forward as emotion baselines; keep Laya as a candidate for a small, reviewed set of scoped predicates. Select relevance/topic separately. No model has established Scoracle accuracy or calibration, and no production Classifier stage has been deployed. Development and the bench are on `main`.

## What was measured

Forty canonical retained articles from 35 publishers across FOOTBALL, NFL and NBA, selected by source/sport/body length without a previous model verdict. They include injuries, transfers, recaps, opinions, German reporting, galleries, stats and low-relevance stories. Three emotion specialists processed every non-whitespace byte through the existing production windows: 374 windows, each limited to 100 words/1,200 bytes. Eight fictional controls separately probe negation, opposing emotions, historical quotes, unrelated teams, sarcasm and different speakers.

All four models also received **identical opening windows from 20 articles**, plus the same eight controls. These excerpts are explicitly not whole articles. Every model emitted 28 continuous emotion scores; no routing thresholds, generated summaries or score-to-probability calibration were added. The specialists use independent sigmoid outputs; Laya used 28 independently worded `noul` predicates. This compares one candidate implementation of the spectrum, not every possible Laya schema.

Archbox Intel Core i7-7700 (3.60 GHz, four cores/eight hardware threads), two PyTorch threads, float32; specialist window batch size eight, eager attention. Runtime: torch 2.14.0, transformers 5.17.0, huggingface_hub 1.33.0, Laya 0.3.20. Downloads/model loading are excluded from inference totals and recorded separately. First inference is included; there is no controlled warm-up, dedicated host isolation or repeated-run confidence interval. All tested models are non-generative encoders.

## Measured cost and useful role

Real opening excerpts only; the controls are excluded from these timings. p95 uses nearest rank over 20 measurements.

| Candidate | Measured parameters | Median/excerpt | p95/excerpt | Total, 20 excerpts | Useful role |
|---|---:|---:|---:|---:|---|
| Horizon multilingual-emotions-small | 140.65M | **159 ms** | 233 ms | 3.31 s | Fast, fixed 28-label emotion baseline |
| SamLowe RoBERTa GoEmotions | 124.67M | 254 ms | 380 ms | 5.35 s | Emotion reference; useful mixed-emotion control response |
| Pradeep ModernBERT-GoEmotions | 149.63M | 321 ms | 530 ms | 6.82 s | Second fixed-label emotion baseline |
| Installed English Laya, 28 questions | 421.29M | **24,827 ms** | 43,166 ms | 559.46 s | Flexible, question-conditioned predicates and ordinal scores |

The median Laya bank was about 156 times Horizon's in this run. Laya processes question-conditioned inputs rather than one fixed 28-output emotion head. This CPU bank cost is not a claim about Laya's GPU speed or a smaller question bank. Five scoped questions on the eight short synthetic controls took a median 2.83 s with `noul`, or 3.61 s with the two-option `choice` variant; these are separate inputs and experiments.

October 7 batching inspection: the archived replay calls `agent.predict(text, questions)` once per source window with all 28 questions. The installed SDK's `predict` delegates to `predict_batch([state], questions)`. `_encode_state` tokenizes the source once but builds a separate source-plus-question sequence for each question; `collate_items` packs those 28 sequences into separate tensor rows, and `_infer` passes all rows to the model in one forward call. Thus the bank is already question-batched, while encoder computation still operates on 28 source/question rows. The fixed emotion heads receive one source row and return 28 scores. Repeated encoding, larger weights, question lengths/padding and CPU execution plausibly contribute to the gap; their individual latency contributions have not been profiled. This is not evidence of 28 serial SDK calls or proof of a particular speedup from changing batch configuration. No inference or production change was made for this inspection.

For the **40 complete retained sources**, Horizon took 60.22 s, SamLowe 97.82 s and Pradeep ModernBERT 136.37 s. A complete-article Laya attempt took 57.41 s and 89.46 s for its first two articles and was stopped with receipts preserved. Do not extrapolate these samples to the daily 8,000–9,000-item load.

At this CPU phase, the installed CUDA build failed an actual GTX 1070 Ti kernel probe. The later isolated CUDA 12.6 run below resolves that runtime incompatibility without changing production dependencies.

## What the controls exposed

Raw, uncalibrated scores rounded to three decimals. Compare responses within a model; their numerical scales are not interchangeable probabilities. These designed controls are diagnostics, not an independently annotated accuracy set.

| Supplied text / inspected labels | Laya | Horizon | Pradeep ModernBERT | SamLowe |
|---|---:|---:|---:|---:|
| Alex: “relieved and happy” — joy / relief | .945 / .910 | .823 / .010 | .379 / .127 | .540 / .097 |
| Alex explicitly denies both — joy / relief | **.832 / .715** | .099 / .003 | .008 / .004 | .010 / .005 |
| Alex hopeful and worried — optimism / nervousness | .887 / .772 | .029 / .104 | .369 / .079 | .092 / .343 |
| Old interview, no new feeling — optimism | .877 | .852 | .878 | .583 |
| Maple's Robin delighted, no Cedar information — joy | .899 | .749 | .054 | .205 |

Laya's broad predicates coactivated many unsupported emotions, especially in the denial control. The specialists generally reduced joy/relief on denial, but their relief response on the positive control was weak. Opposing emotions also varied by label and model. No arbitrary cutoff has been used to call those vectors correct or incorrect.

Detecting hope in the old interview or joy in Maple reporting is legitimate **text-level** detection. Treating those scores as current Cedar emotion would be a downstream attribution error. None of the fixed emotion heads emits speaker identity, quote time, target relevance or an explanatory span. Exact byte-bound input windows are inspected evidence, not proof that a particular phrase caused a score. One supporter's sarcasm also cannot establish a whole fanbase mood.

### Scoped Laya predicates and the documented workaround

Four extra questions asked about Cedar relevance, Alex's current optimism, Alex's current relief and expressed intensity on a 0–3 ordinal scale. Neutral was rescored alongside them. Laya's current [model documentation](https://huggingface.co/convaiinnovations/laya) warns that English `noul` option labels can dominate the state and recommends a two-option `choice`. That variant was run on all eight controls, preserving both option scores rather than using the winning label as a gate.

| Scoped test | `noul` score | `choice` affirmative score |
|---|---:|---:|
| Explicit current relief | .859 | .931 |
| Explicit denial of relief | .180 | .220 |
| Old interview only: current optimism | **.630** | **.704** |
| Other club only: Cedar relevance | **.798** | **.602** |
| Routine Cedar schedule: Cedar relevance | .097 | .563 |

Scoping improved relief negation substantially, but did not resolve historical/current confusion or reliable entity relevance. The ordinal score also gave the explicit mixed-emotion control only .232 on the 0–3 scale. Changing the question schema alone did not establish a trustworthy gatekeeper.

The original Laya **neutral** predicate contained conflicting answer descriptions. Its original score is retained but excluded from quality conclusions. Corrected wording was run separately on all 28 matched inputs. Even then, the routine schedule scored .216 for no expressed emotion, while Alex's happy/relieved quote scored .757. With `choice`, the corresponding scores were .957 and .745. The corrected replay was preserved as `classifier_replay.neutral-v2.py`; Laya helpers were subsequently removed from the active launch bench. SDK confidence, answer confidence and vendor calibration claims are not Scoracle reliability measurements.

## Source quality and what follows

The retained extracts contain navigation, paywall/social prompts, gallery furniture and related-story links. Complete coverage of an extract does not certify a clean publisher article. Opening-only judgments would be particularly misleading on those items. Canonical source IDs and duplicate references were retained; the duplicate control represents two source references to one measured item, not two independent witnesses or a tested deduplication algorithm.

Next: independently review the planned 300–500 source items, including relevance, topic, speaker, time, supporting spans and unknown/ambiguous cases. Keep source groups and time-separated held-out items out of training. Evaluate per-label quality and calibration before picking a model or routing cutoff. Benchmark the actual daily corpus after selecting the useful dimensions. Then implement the smallest useful Classifier slice and cut over directly on `main`, as authorized; a prolonged shadow rollout is not required.

## Receipts and reproduction

- [Updated launch plan](scoracle_classifier_plugin_launch.md), [source manifest](../fixtures/classifier/corpus-manifest-2026-10-07.jsonl), [controls](../fixtures/classifier/controls.jsonl), and [machine-readable measurements and full control vectors](../fixtures/classifier/comparison-2026-10-07.json).
- [Read-only SQL export](../examples/classifier_export.sql), [production window reuse](../examples/classifier_windows.rs), [replay](../examples/classifier_replay.py), and [runnable contract check](../examples/test_classifier_replay.py).
- Full private input/output receipts, requests, token IDs, raw specialist logits, Laya SDK responses, scripts and logs: Archbox `/mnt/data/backup/scoracle/classifier-launch/2026-10-07/` (directory mode 0700). File SHA-256 identities are in the machine-readable measurements. Original publisher bodies remain in that private run directory; the tracked manifest contains hashes and byte bounds.
- `full-encoder-results.jsonl`: 144 verified receipts (3 × 48); `matched-results.jsonl`: 112 (4 × 28); corrected neutral/scoped probe: 28; choice probe: 8. All final receipts passed complete declared-source binding, finite outputs and no-truncation checks. The initial incompatible tokenizer-overflow attempt remains preserved as failed receipts and was replaced by shared production windows, not silent source omission.

Pinned checkpoints, also recorded with actual tokenizer/weights identity:

| Model | Revision |
|---|---|
| [Horizon small](https://huggingface.co/Horizon-Labs/multilingual-emotions-small) | `1bc9627f189e5aa08b00cf525c3c86052067b81e` |
| [Pradeep ModernBERT](https://huggingface.co/Pradeep-mahato/ModernBERT-GoEmotions) | `e7f0a1ccbe10ba1113f39cc6f38733b065cdfb74` |
| [SamLowe RoBERTa](https://huggingface.co/SamLowe/roberta-base-go_emotions) | `d75048347613a25d77de8cf6412eaae9fa7b26be` |
| English Laya local checkpoint | `55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851` |

The installed Laya weights have SHA-256 `891102d372688fc2a094dac56a384bc537b87c63f21f9f3dac0be2b7cbc8d86c`. Replay commands from the isolated directory, using the existing runtime:

```sh
PY=/mnt/data/backup/scoracle/harvester-runtime/venv/bin/python
export HF_HOME="$PWD/cache"
$PY classifier_replay.original.py matched-inputs.jsonl fresh-matched-results.jsonl \
  --model laya@55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851 \
  --laya-model-dir /mnt/data/backup/scoracle/harvester-runtime/model/english \
  --model Horizon-Labs/multilingual-emotions-small@1bc9627f189e5aa08b00cf525c3c86052067b81e \
  --model Pradeep-mahato/ModernBERT-GoEmotions@e7f0a1ccbe10ba1113f39cc6f38733b065cdfb74 \
  --model SamLowe/roberta-base-go_emotions@d75048347613a25d77de8cf6412eaae9fa7b26be \
  --device cpu --threads 2 --batch-size 8 --max-tokens 512
```

That archived script intentionally reproduces the original neutral wording. The archived `laya_scoped_probe.py` and `laya_choice_probe.py` preserve the separate follow-up experiments and exact question files; their `classifier_replay` import refers to the preserved corrected-neutral helper version. Choose fresh output filenames before repeating them. The current tracked replay restores Laya with corrected neutral wording and independent binary questions.

## GPU baseline and GitHub investigation — October 7 continuation

An isolated environment at `/mnt/data/backup/scoracle/classifier-launch/2026-10-07/gpu-v1/venv` uses torch 2.14.0+cu126, the existing transformers/Laya versions and the same pinned checkpoints. A real CUDA kernel succeeds on the GTX 1070 Ti. Production cognition stays disabled; RSS discovery continues. The replay rejects source/question truncation and device fallback and retains complete vectors, token identities, configuration and raw responses.

| Candidate | Median/opening excerpt, GPU | Total for 40 complete sources, GPU |
|---|---:|---:|
| Horizon small | 12.0 ms | 2.80 s |
| SamLowe | 11.4 ms | 3.62 s |
| ModernBERT-GoEmotions | 18.0 ms | 4.92 s |
| English Laya, 28 independent `noul` questions | 791.7 ms | 338.99 s |

Opening measurements use one warm-up and three measured repetitions; each article contributes its median. Complete-source measurements use one warm-up and one measured repetition, batching up to four windows within each article. All models use float32. The separate 28-binary-`choice` Laya opening comparison measures 792.5 ms median. Receipt counts are 112 matched, 192 complete-source and 28 choice, all marked measured; each complete-source model covers 374 real windows plus eight controls. The machine-readable comparison records receipt hashes. These are latency baselines, not independent accuracy results or daily backlog capacity estimates.

**Two important benchmark limits:** the replay explicitly disables Laya autocast, whereas upstream CUDA defaults use mixed precision (FP16 on capability below 8). Also, the opening benchmark calls inference for one article at a time: setting batch size eight does not batch eight articles when each supplies one window. Cross-article batching, length grouping and precision/output parity remain unmeasured. GPU Laya is roughly 31 times faster than its earlier CPU median, but this configuration does not establish its optimized ceiling or hosted Jev performance.

Public repositories were read, not executed:

| Repository / inspected revision | Useful finding | Scoracle limit |
|---|---|---|
| [Laya](https://github.com/NandhaKishorM/laya/tree/3cf26cbcb18725dbc2d127bb8bb2c4c43243ae63) | Its own T4 table reports English latency rising from 39.5 ms for one question to 771 ms for 50. `predict_batch` supports many states, length grouping and mixed precision; its batch benchmark checks output consistency. | The advertised 33 ms is a one-question multilingual measurement. Different hardware/checkpoints and input lengths prevent direct timing equivalence. Our installed SDK is 0.3.20; inspected upstream code can differ. |
| [Jev cookbook](https://github.com/nexibeo/jev-cookbook/tree/3e0207dfcbab0043ef317df73ddc2343a094ef9c) | Multi-label tagging uses one independent `noul` per label in one call, as our bank does. Extraction uses code-owned candidates and a Choice over IDs plus `none`. | The tagging demo uses 24 short title/summary inputs. Its forced minimum one tag and true-count top-k are unsuitable for valid no-signal cases. Regex field candidates do not establish complete semantic claim extraction. |
| [jev-mcp](https://github.com/jkudish/jev-mcp/tree/86eae7861c60ea99a1b2eab2881db0fb1164fcc0) | Shares the classification catalog in state; verification decomposes evidence relation and same-subject judgment, composed in code. | Review thresholds need local calibration. Its optional input clipping cannot be copied into complete-source Classifier processing. |
| [sqlite-jev](https://github.com/mgaitan/sqlite-jev/tree/1ac946cdc0d80fd2d77ba0b64deafb456521fdd4) | Batches retained rows with stable row IDs and shared state. | Reuse that batching principle in existing tooling; no SQLite extension or new framework is needed. |
| [Probability-aware Jev benchmark](https://github.com/AbdelStark/jev-benchmarks) | Reports accuracy, calibration, selective risk and latency separately. On its 100-item six-emotion task, Jev accuracy is 0.48; local fixed-task comparisons can be faster. | Remote Jev versus local Apple CPU is not a hardware-controlled speed comparison; these are external task results, not Scoracle qualification. |

[TypeSafe's Jev documentation](https://docs.typesafe.ai/models) describes ingesting shared state once and evaluating questions in parallel. Local Laya constructs a separate encoder row per source/question pair. Both return typed decisions, but they are different implementations: the local result cannot validate or refute hosted Jev's latency claim. No hosted Jev call was made.

The next controlled experiment should measure question counts 1/4/28, states per batch 1/4/8 (memory permitting), grouped versus ungrouped lengths, and float32 versus supported default mixed precision. Separate per-item latency from throughput; retain output deltas, decision changes and every source byte. Then test source-candidate selection/verification against open extraction on the frozen controls and independently reviewed real sources. Candidate IDs can eliminate invented quote text or occurrence numbers, but candidate recall, attribution, time and qualification still require evaluation. Do not reduce the required spectrum, force a dominant emotion or add a preliminary admission gate to make the timings look better.

Real-model qualification remains incomplete. The initial Qwen generation completed but failed native source binding; a follow-up also emitted schema-invalid label values despite requesting structured output. Failed replies remain retained. The prepared 12 fresh reports and eight fresh adversarial controls have source-only AI provisional references, not independent gold labels; the 12 older fictional fixtures remain a separate diagnostic cohort. Fix and verify the inference/structured-output boundary before continuing that model bank, then conduct independent semantic evaluation. No model or production route has been promoted.

## Hugging Face open-model shortlist

The user excludes proprietary/hosted Jev. The following public checkpoints have Apache-2.0 declarations and downloadable weight artifacts, checked through model cards and Hub metadata on October 8 UTC (October 7 Detroit). Their inference implementations are available publicly. None has been downloaded or executed in this research phase. Recommendations below are architectural fit judgments, not Scoracle accuracy findings.

| Candidate | Classifier role and material limit | Inspected Hub revision |
|---|---|---|
| [GLiNER2.5 base](https://huggingface.co/fastino/gliner2.5-base-v1) | 194M English boundary model: source spans, structured records, relations, classification and span-conditioned attributes. Strong first extraction candidate. | `ca906247640776a07753514055be9726f9080ead` |
| [GLiNER2.5 multi](https://huggingface.co/fastino/gliner2.5-multi-v1) | 287M multilingual counterpart. Its per-person sentiment example directly addresses mixed speakers, but does not prove news attribution or 28-emotion coverage. | `2ca71aafb3446d9014e1c55c7ff51c9bc7209c47` |
| [GLiNER2.5-Decide](https://huggingface.co/fastino/GLiNER2.5-Decide) / [multi-Decide](https://huggingface.co/fastino/GLiNER2.5-multi-Decide) | Runtime-defined single/multi-label classification and several tasks per pass; decision-focused alternative to Laya. Retain full scores before applying any threshold. | English `5a7adf72a23b4d311abae6ce050d7f0012bb3416`; multi `a35a0cd3b7a0f00f2effc576f454cd48fa98aa5f` |
| [Kev-0.8B](https://huggingface.co/jaredpalmer/kev-0.8b) | Typed choice/boolean/ordinal scores; computes and caches shared state before independent question rows. Closest architectural experiment for the question-count speed issue. Requires Qwen base weights plus adapter and pointer head; out-of-domain quality trails larger Kev sizes. | `bf75a6a8848ea6960ff2ed108d9ed44c2941174f` |
| [Intern-Decision-0.8B](https://huggingface.co/internlm/Intern-Decision-0.8B) | Scores decision placeholders in one forward pass without generation. Wrapper accepts 1–16 questions, up to 62 options each, and rejects input over its default 8,192-token budget. Our larger bank would require explicit partitions. | `85a0cc5a99d67ea8d56dfe98115689212867171d` |
| [Laya multilingual](https://huggingface.co/convaiinnovations/laya-multilingual) | 322M mmBERT checkpoint rather than the tested 421M English model. Same typed primitives; broader language coverage. Default 1,024-token input limit must be explicitly configured and verified for longer inputs. | `1720e3e3357cfe1e281542e223f8273b0890ca34` |
| [Verdict / RLCD ModernBERT](https://huggingface.co/heman10x/rlcd-modernbert-151m) | 151M GLiClass-based challenger with 24 substantive options plus an insufficient-evidence slot. Published intent/OOS results are narrow; abstention and news predicates still need independent testing. | `8af2496eb63c7fa66d7d234e1f62629380030eb4` |

GLiNER2.5 boundary configs declare `max_len=4096`; ordinary extraction can truncate. Its long-source helpers remap overlapping chunks, but preserve a relation only when both endpoints occur in one chunk. They cannot certify relationships across the whole report. Convert returned character offsets to native UTF-8 spans and verify literal matches; preserve unresolved cross-window relationships. Entity sentiment is distinct from quote-speaker identity, event time, denial and correction.

Hardware qualification comes before timing claims. Kev's documented fast CUDA route uses BF16/fused kernels; the GTX 1070 Ti needs a compatible measured route. Intern-Decision's inspected `inference.py` explicitly supports `float32` as well as reduced precisions. Hub metadata reports 486,444,053 stored parameters for English Decide, while its card advertises 340M; measure loaded weights/memory rather than relying on the headline. No current throughput estimate follows from vendor timings on newer GPUs.

First comparison recommendation: GLiNER2.5 base/multi for source extraction, Decide for the spectrum, and Kev-0.8B plus Intern-Decision-0.8B for typed predicates/candidate selection, retaining English and multilingual Laya as baselines. These are candidate implementations inside one Classifier; this does not authorize a new preliminary gate or require deploying several models. Select the smallest set that passes the complete downstream evidence contract.

Additional inspected alternatives: [GLiFormer large](https://huggingface.co/knowledgator/gliformer-large-v1) offers shared-encoder extraction/classification/relations but adds another 575.6M family; [GLiClass instruct](https://huggingface.co/knowledgator/gliclass-instruct-large-v1.0) offers flexible label scoring but does not by itself resolve source-span extraction. [Laya typed-decisions](https://huggingface.co/convaiinnovations/laya-typed-decisions) is specialized to synthetic operational workflows, not sports reporting. [NanoJev](https://huggingface.co/C-Tianyu/NanoJev) is currently trained for game actions. [Tev1 experimental](https://huggingface.co/togethercomputer/Tev1-0.8B-experimental) still uses next-token generation and its fine-tuned weight license is not finalized, so it is excluded from the initial bank. The [community Decision Index](https://huggingface.co/spaces/multimodalart/jev-decision-index) helped discover candidates; its private composite leaderboard is not reproducible Scoracle qualification.

## Full-source runtime and qualification experiment — October 8

**Result: five candidates executed, none qualified for production.** This tests implementations against the existing Classifier spectrum and qualification contract; no model becomes an acquisition or headline admission gate. Cognition remains paused. All 160 final model/source spectrum receipts completed without source clipping. Completion proves execution and output shape, not semantic accuracy.

The bank contains one candidate target for each of 32 complete retained bodies: 12 previously examined fictional diagnostics, eight fresh controls and 12 fresh real reports. The source-only reference was frozen before these candidate replies. Previously examined controls remain separate from fresh-cohort accuracy. The reference is AI provisional and not independent human gold.

The decision bank measures all 51 presence dimensions, three **coarse anchor choices with an unknown option**, acquisition usability and six target/time/denial/correction diagnostics. Anchor choices are not validated continuous target ordinals. GLiNER encodes the source once with five multi-label families, the other tasks and literal claim-record fields. Laya/Kev use their trained decision heads without generation; Intern partitions its documented 16-question limit into four complete-source calls. No predicted score is rescaled into a Scoracle probability or used to discard data.

| Candidate | Runtime / hardware | Median inference per real body | Fresh control predicates correct / 48 | Positive predicates found / 7 | False positives / 41 |
|---|---|---:|---:|---:|---:|
| GLiNER2.5 base | Native PyTorch float32, GTX 1070 Ti | 579 ms | 39 | 4 | 6 |
| GLiNER2.5 multi-Decide | Native PyTorch float32, GTX 1070 Ti | 979 ms | 35 | 1 | 7 |
| Intern-Decision 0.8B | Official HF wrapper, float32, GTX 1070 Ti | 2,456 ms | 41 | **0** | 0 |
| Kev 0.8B | GGUF Q8_0, upstream llama.cpp, Mac M4 Metal | 7,297 ms | 39 | 2 | 4 |
| Laya | GGUF Q8_0, upstream llama.cpp, Mac M4 Metal | 12,642 ms | 18 | 4 | 27 |

These are single-pass medians over 12 complete real bodies, not repeated latency trials or daily throughput. Hardware, precision, heads and outputs differ; this is not an architecture speed ranking. GLiNER timings include native record extraction/decode, while decision models measure their 61-question bank. Input/schema preparation and explicit token preflight are outside the reported inference timers; total wall timings are retained. Mac logs verify Laya 31/31 and Kev 25/25 layers offloaded to Metal, with Kev shared-prefix question copies. The stable Ollama installations remain 0.40.1; the isolated upstream test build is `b11496`.

The diagnostic threshold is 0.5, with full raw scores retained. There are only seven positive expectations among 48 control predicates: an always-absent answer already gets 41/48. **Intern's apparent lead is entirely that baseline; it misses every positive.** GLiNER base is fastest in this tested bank and finds more positives, but still invents or misses important relationships. Laya remains unreliable on historical/other-speaker emotion, quotation denial and withdrawn claims. Kev attributes current target emotion to the footer-instruction control. These small, provisional checks are not overall model accuracy or calibration.

### Actual runtime compatibility

- Updated Ollama imports both GGUFs, but `/v1/systemone` returns `unsupported decision encoding "laya"` and `"kev"`. Its bundled native runner also rejects Laya's tensor shape. Loading a model file does not prove its trained readout is supported.
- The isolated official upstream llama.cpp build executes both trained heads on Mac Metal. Its archive SHA-256 is `0eeb3bdef43d6b0ab28fb1b8aeacca8b2bb590cc76ca05c53ef0ac5e9850eebf`. Laya GGUF revision is `22265007700297ba9e128297e82540cf28c5d7d4`, with declared source revision `7b928d828b7b0e022f929d9bd2e44165aa270148`; this is distinct from the earlier Python baseline checkpoint. Kev GGUF revision is `e551e319d483ff57e1ff208924b349d397cffc1c`, declared source `bf75a6a8848ea6960ff2ed108d9ed44c2941174f`.
- Native Laya preflight refuses head/question clipping and compares rendered-token totals with actual server usage. Complete report rows require an 8,192-token microbatch rather than the embedding runner's default 512. Kev likewise checks every complete source/question budget and exact token totals. Neither drops options to satisfy a model limit.
- GLiNER2.5 needs Transformers 5-compatible tokenizers for these snapshots; installing its local extra had selected an incompatible Transformers 4 runtime. Only the isolated test environment was repaired. Its processor automatically appends a period to bodies ending without `.`, `!` or `?`; the first strict preflight retained nine failures. The corrected preflight records that exact suffix, forbids other source changes and still refuses evidence outside the original body. Both final banks complete. CUDA allocation retries appeared on long cases; successful inference is not a burst-memory qualification.
- Intern's official wrapper imports image/video processors even for text-only inference. The isolated environment now includes its Pillow/Torchvision dependencies. Its real float32 CUDA forward works on the GTX 1070 Ti and enforces the 8,192-token budget without truncation.

### Claim qualification and frozen-reference evaluation

GLiNER record outputs were converted from source character spans into literal quote/occurrence objects and passed through the existing Rust `classifier_qualify` boundary. No field was repaired from expected labels or inferred by the adapter. On the 20 fresh cases, base has 14 structurally accepted records and Decide has five. **Both miss all 22 frozen evidence/relationship/time checkpoints.** Base often returns a noun phrase such as “tomorrow's scan” instead of the qualified statement; other records omit required subject/target evidence or return no claims. Empty source-bound records can pass structural validation without covering the reporting. Proposed-record usability checks pass 16/19 for base and 3/19 for Decide among fresh cases with a known expectation; one real acquisition remains ambiguous. These are diagnostic findings, not a valid production gate.

The existing generative qualification runner also had a JSON Schema bug: its emotion `enum` was an object rather than an array. This is fixed and checked. An Ollama API attempt additionally replaced the model-bound runner and changed prompt-token counts; those failures are retained. The corrected native `/completion` path submits the exact preflight token IDs with a valid schema, rejects incomplete/context-truncated output and verifies the actual input-token count. Qwen3 1.7B completes five smoke cases through that path but all five fail native qualification, including nonliteral quotes, incorrect occurrences and unusable/incomplete review assertions. Structured generation does not solve qualification by itself.

The stronger Ministral 3B baseline completes four of the same five smoke cases; the remaining real report exhausts its 2,048-token output reservation and stays a failure. All five fail native acceptance; completed replies still contain absent quote occurrences. Its setup exposed model-manifest normalization during loading. The runner now freezes the resolved manifest after load and still requires unchanged GGUF path, template and requested context before inference. Initial and resolved identities are both retained. Earlier identity-mismatch attempts are runtime failures, not semantic accuracy observations. This does not justify extending a small smoke result into an overall model-quality ranking.

The full 51-label spectrum, continuous ordinals, calibration, claim precision/recall and independent human adjudication remain unqualified. The next experiment should test native source-candidate selection and explicit speaker/claim/time links against the same fixed contract, rather than relaxing evidence checks or promoting the fastest model. Fine-tuning requires the planned independently reviewed source groups and held-out splits. No production Classifier, routing or publication change is justified by this round.

### Retained artifacts

Durable run directory on Archbox: `/mnt/data/backup/scoracle/classifier-launch/2026-10-07/flow-v1/`. `run-manifest.json` records checkpoint revisions, source/reference/request hashes, hardware, precision, complete case counts and measured timings. Final spectra are `laya-decisions.jsonl`, `kev-decisions.jsonl`, `intern-decisions.jsonl`, `gliner-base-v2.jsonl` and `gliner-decide.jsonl`. `decision-accuracy-v2.json` retains every diagnostic expectation, raw score, confusion count and absent-only baseline. Native GLiNER receipts and accuracy files are retained separately from spectra, along with all rejected preflight/runtime attempts and generative smoke replies. Repository metadata is in [comparison-2026-10-07.json](../fixtures/classifier/comparison-2026-10-07.json).

Runners: `examples/classifier_decision.py`, `examples/classifier_gliner.py`, `examples/classifier_decision_accuracy.py` and the corrected `examples/classifier_infer.py`; existing Rust source-binding and Python frozen-checkpoint evaluation are reused. Boundary checks cover invalid scores, source changes, Unicode occurrences, historical/other-target attribution, schema enum identities and incomplete native generation.
