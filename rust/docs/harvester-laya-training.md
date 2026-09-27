# Laya tuning for Harvester

The initial goal is **retain at least 75% of useful, entity-relevant news openings**
while reducing the amount of irrelevant or unusable material sent downstream. Recall
is the proportion of labelled useful openings retained, not classification accuracy,
the percentage of all articles accepted, or demonstrated coverage of unique stories.
Precision and forwarding rate are reported alongside recall. Passing everything is
an explicit comparison baseline, so it cannot masquerade as an effective filter.
The evaluator also counts retained positive story groups separately, so two articles
about the same labelled event do not inflate measured story coverage.

Laya's [official training guidance](https://github.com/NandhaKishorM/laya#fine-tuning)
describes domain fine-tuning as a major source of accuracy improvement. The
[official notebook](https://github.com/NandhaKishorM/laya/blob/23a17522aa4942da6cce53a995a275760320b691/notebooks/laya_finetune_typed_decisions_2xT4_kaggle.ipynb)
uses an RLCD proper-scoring-rule reward with policy-gradient training and
cross-entropy guidance, followed by temperature calibration. The first Harvester
zero-shot trial was a baseline; it did not establish the attainable quality of a
domain-trained checkpoint.

## Implemented local training path

1. The Rust Harvester prepares the same headline/opening and questions used at inference.
2. `examples/harvest_training_data.py` joins source-bound annotations and assigns entire
   story groups to train, calibration, or test. A source or question change invalidates
   the annotation hash. Missing annotations remain unknown.
3. `examples/harvest_laya_train.py` trains locally on CPU, MPS, or CUDA. The default trains
   the decision head with a frozen encoder; `--encoder-layers 2` also adapts the final
   two encoder layers. It uses the upstream RLCD-plus-cross-entropy objective with
   four exploration samples. It does not require a hosted teacher or remote trainer.
4. Temperature and intake thresholds are fitted on calibration data only. Test data
   never supplies gradients, temperatures, or threshold cutpoints.
5. The run saves a native `laya.load` checkpoint, model/config/tokenizer, source-weight
   hash, dataset hash, training arguments, dependency versions, baseline/trained logits,
   predictions, and reports. Output directories must be new; prior runs stay intact.

`examples/harvest_laya_evaluate.py` can re-evaluate saved logits without retraining.
Its v2 calibration uses empirical cutpoints and four-decimal scores matching Laya's
public API. The first runs' coarse-grid reports remain preserved: their grid skipped
narrow score ranges after temperature scaling. Compare candidates using the same v2
report, rather than mixing those two evaluation methods. This methodological adjustment
and repeated pilot inspection mean a fresh evaluation set is still required.

Laya inference and weight training run locally. The training scripts use network access
only during setup to retrieve official code/configuration and pinned open weights, and
do not upload the corpus or publish checkpoints. Seed annotations were produced by
Codex in this task; label creation itself was not an entirely offline process. A fully
sovereign labeling workflow would use local annotators or human review.

## Seed data and limits

The [seed annotations](../fixtures/harvester/README.md) cover 120 articles from the
September 24 sweep, with 224 labelled decisions. After omitting two wholly uncertain
examples, the fixed split contains 62 training articles (117 decisions), 27 calibration
articles, and 29 test articles. Only 26 calibration and 25 test articles have both
labels needed for the joint intake metric; each contains eight positive news openings.

These are provisional Codex source-review annotations, not independently human-verified
gold labels. The previous Editor's classifications were not used as target labels.
Some cases had already been inspected during the original trial. Related reports were
grouped manually; paraphrased stories and template similarity need further review.

Only entity involvement and content category are trained. No confidently labelled
clickbait cases exist in this seed. Neither clickbait removal nor sentiment/character
assignment quality is established by these runs. Because the decision head is shared,
training can affect those other outputs; the resulting checkpoint is experimental and
must not silently replace the seven-question production capability.

The corpus contains already-readable Editor inputs from one day. Many stored openings
are navigation, related-story cards, scoreboards, or author biographies. Labels describe
the actual model input, not the reporting hidden elsewhere on the source page. Better
article-body extraction is a separate way to increase end-to-end coverage. This pilot
does not measure the percentage of all swept news successfully acquired or represented
in final character output.

## First completed experiments

All three checkpoints trained successfully on the M4, with verified weight updates:

| Checkpoint | Trainable parameters | Epochs | Local elapsed time |
|---|---:|---:|---:|
| Multilingual, decision head | 14,770,945 | 4 | 92 s |
| Multilingual, head + final 2 encoder layers | 24,801,025 | 8 | 175 s |
| English, head + final 2 encoder layers | 50,762,753 | 8 | 325 s |

Elapsed times include baseline evaluation, training, final inference/calibration, and
checkpoint writing after setup; they exclude downloads and initial model loading.
The fixed split has 29 positive openings among 55 completely labelled training cases,
versus eight among 26 calibration cases and eight among 25 test cases. This population
difference is another limitation of the small seed.

Selecting the most precise calibration policy that retains at least 75% of calibration
positives did not generalize: multilingual retained 3/8 test positives, and English
retained 1/8, both before and after training. Entity classification improved modestly,
but the pilot did not establish a useful recall gain from weight training.

After inspecting these results, an **exploratory operating curve** also checked
calibration targets of 87.5% and 100%. These are post-hoc diagnostics, not a new clean
validation result. With the 100%-calibration-recall setting:

| English candidate | Useful openings retained | Positive stories retained | Articles forwarded | Precision |
|---|---:|---:|---:|---:|
| Untuned, calibrated thresholds | 6/8 (75%) | 5/7 | 12/25 | 50% |
| Trained, calibrated thresholds | 6/8 (75%) | 5/7 | 13/25 | 46% |
| Pass everything | 8/8 (100%) | 7/7 | 25/25 | 32% |

The trained exploratory cutoffs are `subject + opponent >= 0.3858` and
`reporting >= 0.5719`, with temperature 2.0. They are saved proposals, **not installed
Harvester policy**. The 6/8 recall estimate has a Wilson 95% interval of roughly
41%–93%, even before accounting for provisional labels and repeated exploration.
Story recall is 5/7 (~71%), not 75% proven story coverage.

The useful finding is that a local, non-generative screening path can retain much of
this tiny positive set while forwarding about half the inputs. The experiment does
not yet show that fine-tuning improves on the English base, nor that these thresholds
transfer to a fresh day. Original reports and all candidate operating curves are
retained, including unsuccessful runs.

## Reproduce the pilot

The model environment is `/private/tmp/scoracle-intake-runtime` (Python 3.12, Laya
0.3.20, PyTorch 2.14.0, Transformers 5.17.0). Runs use MPS FP32 on the local M4 Mac,
two CPU threads, batch size two, and four gradient-accumulation steps. Hardware timing
is not an Archbox benchmark.

Both English and multilingual checkpoints are pinned to
`convaiinnovations/laya@55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851`. The English checkpoint
is the repository root; multilingual is its subfolder. Both accept all annotated
openings without truncation; the largest English training sequence uses 444 tokens
against its 512-token limit.

```sh
cargo run --example context_harvest -- \
  logs/context-harvest-20260924/articles.jsonl /private/tmp/new-prepared.jsonl

python3 examples/harvest_training_data.py \
  --prepared /private/tmp/new-prepared.jsonl \
  --labels fixtures/harvester/seed-labels-v1.jsonl \
  --output /private/tmp/new-training-dataset.jsonl

HF_HOME=/private/tmp/scoracle-intake-hf HF_HUB_OFFLINE=1 USE_TF=0 \
  /private/tmp/scoracle-intake-runtime/bin/python examples/harvest_laya_train.py \
  --dataset /private/tmp/new-training-dataset.jsonl \
  --model-dir /private/tmp/scoracle-intake-models/laya/english \
  --revision 55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851 \
  --output /private/tmp/new-harvester-checkpoint --device mps \
  --encoder-layers 2 --epochs 8 --lr 0.00003

python3 examples/harvest_laya_evaluate.py \
  --checkpoint /private/tmp/new-harvester-checkpoint \
  --output /private/tmp/new-harvester-policy-report.json

# Separate exploratory, higher-recall operating point; does not change model weights.
python3 examples/harvest_laya_evaluate.py \
  --checkpoint /private/tmp/new-harvester-checkpoint \
  --output /private/tmp/new-harvester-high-recall-report.json --calibration-recall 1.0

python3 -m unittest discover -s examples -p 'test_harvest_training.py' -v
```

Actual runs, source snapshots, pinned upstream notebook, and reports live under the
Git-ignored `logs/laya-training-20260924/`. The native checkpoints can be served by
`examples/harvest_laya_server.py` with a unique local experiment revision. Serving a
checkpoint does not import its proposed thresholds into Rust; those remain reviewable
evaluation artifacts. Production activation and queue admission are unchanged.

Validation: seven training-contract tests pass, covering source drift, invalid labels,
story leakage, missing labels, recall accounting, narrow calibrated scores, and duplicate
story counts. Weight inspection confirmed an unchanged frozen first encoder layer and
changed final encoder/scorer weights. The trained English checkpoint reloaded through
the existing Laya HTTP adapter and completed three real Rust Harvester calls with valid
seven-question responses (one accept, two reviews under the unchanged Rust policy).
Those three calls check integration, not classification quality. All three resulting
excerpts were verified byte-for-byte against their retained source bodies.

## Next evidence needed

Expand beyond the seed using fresh sweep days and source families, review labels
independently, add genuine clickbait and hard entity negatives, and track source-body
availability separately. Hold out related stories together and reserve a fresh final
test set before further model selection. Measure story/entity coverage and downstream
usefulness in addition to opening-level recall. The 75% target can guide a practical
Harvester without demanding perfect classification; a tiny positive denominator cannot
yet demonstrate that rate for the daily news stream.
