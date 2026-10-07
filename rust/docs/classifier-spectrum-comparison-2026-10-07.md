# Classifier spectrum comparison — October 7, 2026

**Later launch decision:** the user excluded Laya from the launch bank after this comparison. The active replay now supports additional trained vector families; historical Laya scripts/receipts remain archived. See the [current vector contract and execution ledger](scoracle_classifier_plugin_launch.md#downstream-vector-contract--october-7-update).

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

For the **40 complete retained sources**, Horizon took 60.22 s, SamLowe 97.82 s and Pradeep ModernBERT 136.37 s. A complete-article Laya attempt took 57.41 s and 89.46 s for its first two articles and was stopped with receipts preserved. Do not extrapolate these samples to the daily 8,000–9,000-item load.

GPU performance remains unmeasured. The GTX 1070 Ti has compute capability 6.1; the installed CUDA build supports newer architectures, and an actual kernel probe failed. No runtime or production service was changed to overcome that limitation.

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

That archived script intentionally reproduces the original neutral wording. The archived `laya_scoped_probe.py` and `laya_choice_probe.py` preserve the separate follow-up experiments and exact question files; their `classifier_replay` import refers to the preserved corrected-neutral helper version. Choose fresh output filenames before repeating them. The current tracked replay excludes Laya.
