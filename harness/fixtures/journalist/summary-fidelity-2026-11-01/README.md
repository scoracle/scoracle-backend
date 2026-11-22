# Summary-fidelity development trials

These are unsuccessful development experiments, not a qualified prompt release.
The user redirected the investigation from model size to assembly and hardware
capacity. The resulting evidence is in the sibling
[assembly investigation](../assembly-2026-11-01/README.md).

The n114–n118 instruction iterations attempted clearer attribution, metadata
boundaries, source-sentence preservation and specific topics. Some short bodies
improved, but titles and larger packets still changed speakers, publishers,
subjects or certainty. All saved outputs, including mechanical rejections, must
be reviewed against their cited source text. Mechanical acceptance is not a
factual score. `run-index.json` is only an inventory of recorded results.

`before` and `after` denote frozen case variants (often without/with database
metadata), not old/new prompts. The case files and each run's input or fixture
record the actual comparison. Early trial contexts were 10,000 tokens; later
runner calls use the production code default of 7,900. Receipts record the
request. No clipping or publication occurred.

The initial held-out question exclusion incorrectly rejected the phrase
"will return" even within a faithful question. `challenge-cases.jsonl` records
the corrected expectation; that correction is not an improvement by the model.
The eight `qualification-cases.jsonl` cases were uncalled at the end of this
phase. They were subsequently used in the
[detailed-instruction evaluation](../detailed-instructions-2026-11-01/README.md).

The Qwen3.5-9B CPU and CUDA trials on Archbox were interrupted and do not establish
comparative quality or sustained capacity. Their server, runner and temporary
tunnel were stopped. The verified model file remains on Archbox's data disk;
the production model route was unchanged. Larger-model testing is not the
selected direction.

No n114–n118 prompt was promoted. Their exact instructions remain in the trial
artifacts; the next phase started from n113. The detailed-instruction evaluation
records the subsequent n119 working candidate; the assembly issue remains open.
