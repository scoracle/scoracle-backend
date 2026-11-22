# Harvester training annotations

`seed-labels-v1.jsonl` contains provisional Codex annotations of the 120 real sweep
headlines/openings used in the September 24 trial. These are **not human-reviewed
gold labels**. Existing Editor decisions and Laya/Decider predictions were not copied
into the label fields. Earlier inspection of some diagnostic cases means this is an
exploratory pilot, not a blinded benchmark.

Only `entity` and `content` are supervised. Each row binds its annotation to a SHA-256
of the exact Rust-prepared request, including the question text and option descriptions.
Missing labels mean unknown, never a negative. Two entirely ambiguous rows have no
labels and are excluded from the model dataset. The full corpus and derived training
inputs remain in ignored local experiment storage; this file contains IDs and annotations.

Annotation scope is the visible headline/opening. Full-article relevance may differ.
`subject` includes directly reported club personnel and affairs; `opponent` covers
participation as the opposition; `mention` covers incidental/background references;
`absent` means this input provides no supported involvement, including namesakes.
Concrete reporting and meaningful commentary count as `reporting` despite dramatic
headlines. Pure navigation, related-link lists, viewing schedules, and score tables
count as `listing`; ticket/subscription sales pages count as `advertisement`.
Mixed cases have uncertain labels omitted. These distinctions need independent review,
especially related-story extracts, video descriptions, and club/sub-team identity.

There are no confidently annotated `clickbait` examples in this seed. Do not claim
clickbait performance from this run. Character usefulness and emotional tone are not
supervised or evaluated here.

Related reports share `story_group` (for example Manchester United's finances,
Clippers leadership, and Luton–Ipswich). The data builder assigns whole groups to
train/calibration/test using a fixed hash seed. Review grouping when extending the set;
hashing alone cannot detect syndicated or paraphrased reports. The corpus is from one
day and overrepresents already-readable Editor inputs, so collect additional days and
sources before estimating deployment coverage.

These labels describe the retired `entity`/`content` pilot. Its dataset builder,
trainer, evaluator and throughput script were removed during the September 27
cleanup. The source-free labels remain as historical evidence and require the
original archived requests to interpret. Current
`examples/context_harvest.rs` prepares headline relevance and seven scalar theme predicates;
its output is not compatible with these labels. Current training needs new reviewed,
source-bound labels and a matching dataset builder; never replace hashes to bypass
the source/question drift check.

## September 26 cutover evidence

The two `cohort-20260926-*.index.json` files are source-text-free historical
prediction indexes, including model/runtime/file provenance and original trace
hashes. The RSS-first index is a failed architecture regression record.
`cohort-20260926-annotation-queue.jsonl` holds 84 **unlabeled** review tasks;
acquisition failures have no labels and must not be treated as negatives.
`cohort-20260926-ai-provisional.jsonl` adds 71 provisional Codex reviews to a
separate copy of that queue. They are not human gold and cannot enter a data split.
`cohort-20260926-disagreement-priority.json` identifies 28 selected AI/Laya
entity disagreements for human review. It is not an unbiased quality estimate.

See [cutover groundwork](../../docs/harvester-cutover-groundwork-2026-09-26.md)
for packet semantics, annotation format, split policy, limitations, and offline
verification commands. These files do not license or preserve the publisher bodies.
`examples/harvest_annotations.py` checks annotation integrity against the exact
ignored trace and can build an ignored, source-containing reviewer packet without
including Laya predictions.

## V7 routing development fixtures

`routing-v7-development.jsonl` contains 24 wholly synthetic team cases spanning
FOOTBALL, NBA and NFL. Labels were written before their first model run. This set
was subsequently used to diagnose entity-description syntax and multisport wording,
so it is now a **development set**, not a holdout or human-reviewed benchmark.
`routing-v7-development-report.json` records the frozen candidate's 33 true routes,
57 true negatives, three unwanted routes and three missed routes. All 24 headlines
name the intended subject; they do not test headline-negative accuracy.

Extract each row's `article` to a local JSONL file and run `context_harvest` against
the Laya endpoint. Score its output with:

```sh
python3 examples/harvest_routing_score.py fixtures/harvester/routing-v7-development.jsonl REPLAY.jsonl
```

The scorer rejects duplicate/missing articles, inference errors and changed source
identity/body hashes. It evaluates the plugin's final routes, including missed
routes caused by headline rejection. It does not reimplement routing thresholds.
See the [v7 frame report](../../docs/harvester-frame-2026-09-27.md) for limitations.
