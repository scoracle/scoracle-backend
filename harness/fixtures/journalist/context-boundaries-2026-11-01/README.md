# Metadata and source boundaries — 2026-11-01

**Ordering changes attribution, but none of the four layouts improves complete
output reliability.** Moving metadata before reports restores Pavlovic in the
known failure; it also loses qualifications and introduces other unsupported
details. No layout qualifies for promotion or new-subject follow-up.

This investigation continues with [typed metadata and causal diagnostics](../metadata-fields-2026-11-01/README.md)
and a [production serialization check](../production-order-2026-11-01/README.md).
All use the same Qwen3.5-4B-Q4_K_M and frozen real source evidence.

## Controlled comparison

The previous fixture builder used `sort_keys=True`, producing fresh/meta/tone
order. Production's assembler emits meta/fresh/tone. Sorting also reordered
fields inside metadata and reports. This is an experimental-delivery difference,
not evidence of a production omission or truncation.

The 24-call matrix crosses two factors on six unchanged packets:

| Layout | Order | Clearly faithful/readable | Clarity concern |
| --- | --- | ---: | ---: |
| Control JSON | fresh, meta, tone | 4/6 | 0 |
| Context-first JSON | meta, tone, fresh | 2/6 | 1/6 |
| Separate sections | fresh, meta, tone | 3/6 | 0 |
| Context-first sections | meta, tone, fresh | 2/6 | 1/6 |

The remaining packets fail source fidelity. Section labels restate existing
permissions: metadata identifies the subject, fresh contains publisher reporting,
and tone controls style. Segmentation and labels are tested jointly. Metadata
values, sources, groups, IDs/URLs, tone, system instructions and output schema are
unchanged. The context-first arms are not exact production serialization: tone
moves before fresh, and nested values retain their prior field order.

The source-first layouts still replace Pavlovic with database coach Filippo Galli.
Both context-first layouts restore Pavlovic. However, they drop Milan's explicit
two-factors-needed condition and use misleading shared injury/lay-off titles.
They also invent the meaning of the Bears' unspecified streak: Thanksgiving games
or not playing in TNF against the Bears. Some Milan match statements import
facts from neighboring publishers while citing just one report. None meets the
predeclared requirement to retain at least four faithful packets while fixing
the speaker error without new metadata leakage or topic loss.

All 24 outputs have valid source mappings and topic anchors; that is not semantic
entailment. The six controls exactly reproduce the prior examples outputs. No
candidate repeats were run because none qualified. [Full review](review.json),
[selection](selection.json), and [protocol](protocol.txt) distinguish manual
fidelity judgments from mechanical parser outcomes. Review is by Codex, not
blinded or independently adjudicated; the underlying news is not independently
verified. These selected stress cases do not establish a general accuracy rate.

## Cost and checks

| Layout | Input delta vs control | Median wall time | Range |
| --- | ---: | ---: | ---: |
| Control JSON | 0 | 6.23 s | 5.75–9.62 s |
| Context-first JSON | −1 token | 6.17 s | 5.77–10.14 s |
| Separate sections | +23 tokens | 6.37 s | 5.79–10.47 s |
| Context-first sections | +23 tokens | 6.34 s | 5.95–10.02 s |

All calls use temperature 0, thinking off, cache on, context 7,900 and output
reservation 2,100. Every receipt confirms complete input, EOS and no truncation.
Model identity metadata is unchanged. Total measured call wall time is
**174.678 seconds**. All calls reused 4,610 cached prefix tokens, so this is warm
writer cost, not cold daily throughput. No compilation or other experiment ran
concurrently. Manual grouping/review, scheduler behavior and mixed-character
workload are excluded. [Measurements](measurements.json).

Three new subjects—Liverpool, Boston and Green Bay—were captured through the
production loader in read-only sessions. All 40 headlines per subject were
inspected; reviewed groups and source URLs were frozen before inference. They
remain **unused for generation** because no candidate passed development.
See [new-subject source manifest](validation-sources.json).

The example harness now records the canonical source packet separately from the
exact delivered prompt. Its optional layout transform preserves each context's
JSON bytes; focused tests round-trip labels/order, quotes, newlines, non-Latin
publishers and all original values. Production assembly and parser are unchanged.

`build_inputs.py` reconstructs frozen fixtures; `run.py development` executes the
matrix; `analyze.py` validates hashes, wire inputs, schemas, receipts, source
mapping and deterministic rendering. New output directories are required for
reruns. The optional validation path requires a qualifying recorded selection;
the current selection deliberately does not qualify.
