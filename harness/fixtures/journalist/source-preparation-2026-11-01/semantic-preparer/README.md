# Preparation meaning and emission order — 2026-11-01

**Neither candidate improves reliable whole-packet output.** More explicit
preparation instructions fix some individual errors but increase fragmentation.
Generating the claim before its labels reduces output volume, yet still invents
facts and changes uncertainty. Neither candidate passes its follow-up gate.

Both comparisons use the same 29 real stored headlines, six development packets,
Qwen3.5-4B-Q4_K_M and retained worked-example writer. Original sources, article
IDs/URLs, metadata, tone, grouping and writer settings remain intact. History,
gender decisions, runtime code, model configuration and production are unchanged.

| Automatic preparation arm | Clear packets | Clarity concerns | Failed packets | Preparation + writing |
| --- | ---: | ---: | ---: | ---: |
| Earlier worked-writer baseline | 4/6 | 1/6 | 1/6 | 162.21 s |
| Expanded meaning instructions | 3/6 | 1/6 | 2/6 | 178.14 s |
| Same instructions, claim emitted first | 2/6 | 1/6 | 3/6 | 157.61 s |

These are manual, unblinded fidelity judgments on repeatedly used stress packets,
not general accuracy estimates. Titles, source ownership, certainty and required
topic/anchor coverage count. Source-format concerns are not counted as clear
passes. The original 6/6 manual-reference result is reused as a diagnostic; it was
not rerun or treated as independently adjudicated gold.

## What changed

The first candidate adds explicit relationship, speaker, unknown-amount,
qualification and duplication instructions, with synthetic worked examples.
Only the preparer prompt changes from the earlier automatic baseline. Its exact
input grows to 2,073–2,090 tokens and fits the unchanged 4,096 context plus 800
output reservation. See [addition](instruction-addition.txt), [frozen full prompt](preparer.txt)
and [protocol](protocol.txt).

The second candidate changes only the source schema's property insertion order:
`about, speaker, kind, claim, qualifications` becomes
`claim, qualifications, about, speaker, kind`. Required membership and all
constraints are identical, as are both model prompts and every source input.
The emitted preparation fields retain that new order when attached to the writer.
Consequently, a final-output change alone cannot distinguish preparer effects from
writer sensitivity to nested field order; intermediate outputs are also reviewed.
See the [order comparison protocol](../claim-first/protocol.txt).

## Findings

- Expanded instructions preserve an undefined 60-million figure in the final
  summary and restore the required Milan match anchor. They also fix the prepared
  attachment of two requirements to the possible return. However, the preparer
  still turns a reaction to a missed substitution into a judgment of the entire
  performance, and the writer repeats it. The writer separately invents a coach
  role and expands TNF to Thanksgiving despite faithful preparation of that item.
- Claim-first emission makes the missed-substitution account narrower and reduces
  52 prepared statements to 41. Preparation output falls from 2,394 to 1,985 tokens.
  But it turns “Juventus denied at the death” into “Juventus lost the match” with
  no metadata or neighboring source present. It also upgrades a recall question
  to an assertion; the writer then calls it a reported plan.
- The claim-first writer again invents a valuation, borrows competition/goalkeeper
  details from other sources and misses the required match anchor. It changes a
  how-to question about perks into a question about whether the perks are available.
- All 58 preparations pass exact copied-span checks. Those checks cannot validate
  relationships, quote scope or the meaning of a freely generated claim. A good
  copy check is useful but is not an honesty check.

Every preparation and writer output has a review entry: [expanded-prompt review](review.json)
and [claim-first review](../claim-first/review.json). The stricter instructions and
changed ordering trade failures across packets rather than removing them durably.
This narrows the problem: metadata interference is real in earlier trials, but
some mistakes are introduced by the source-only interpretation step itself.

## Cost and verification

The two new comparisons make **70 generation calls**, totaling **335.747 seconds**
of measured call wall time, including every preparation whether cited or not.
Neither uses retries, corrections, fallback, source deletion or a larger model.

| Candidate | Preparation calls/time | Writer calls/time | Total input/output tokens |
| --- | --- | --- | --- |
| Expanded prompt | 29 / 124.160 s | 6 / 53.981 s | 93,954 / 3,439 |
| Claim first | 29 / 108.417 s | 6 / 49.189 s | 93,531 / 2,974 |

Combined times are approximately **4.01×** and **3.55×** the historical 44.383-second
direct-writing control. That control was not rerun or interleaved; observed prefix
cache usage differs. These figures do not establish a timing improvement under
matched conditions or daily throughput. No six-character amortization or durable
cache saving is assumed. Admission preflights, setup and manual review are outside
the call-time totals. Cumulative source-preparation investigation: **174 calls,
942.639 seconds**, including all earlier failed comparisons.

Both complete runs pass the existing independent receipt audit: exact full input,
model identity, reservation, EOS/nontruncation, schema and original source links.
`compare.py` additionally checks the raw HTTP request body preserves the intended
schema order, actual emission follows it, unchanged fields and constraints remain
identical, and attachment preserves notes without changing original packet order.
Builders contain assertions for source identity/schema equivalence; deterministic
reconstruction, Python compilation and diff checks pass. Rust harnesses were
reused without changes; their earlier six focused tests were not unnecessarily rerun.

[Combined measurements](comparison.json), [expanded-prompt receipts audit](measurements.json),
[claim-first receipts audit](../claim-first/measurements.json),
[first stopping decision](selection.json), [second stopping decision](../claim-first/selection.json).

No repeated-writer or untouched validation generation was spent because neither
candidate qualified. Daily capacity and other character plugins remain untested.

## Next experiment

Remove free-form paraphrasing from the preparation stage. Test whether copied
source clauses and their exact qualifications can provide the useful evidence
organization without introducing an extra invented claim before writing. Keep
natural summaries in the writer, retain original sources, and compare total cost
against direct writing. Copied spans still cannot prove their relationship, so
that experiment must retain the same semantic review and must earn its extra
calls. This is a proposed next test, not an implemented or qualified production fix.
