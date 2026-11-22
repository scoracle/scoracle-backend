# Source-scoped preparation — 2026-11-01

**Accurate preparation helps the 4B writer, but the automated preparer is not yet
reliable enough or cheap enough to promote.** With the retained worked-example
writer, manually reviewed source notes produced six faithful complete packets.
The latest copied-evidence arm produced five clear passes and one failed packet,
against two clear passes in the raw-source and complete-headline-copy controls.
Automated preparation plus writing took about **3.5×** the historical measured
time of direct writing. The remaining final error is cross-source attribution.

The investigation made **215 generation calls** across six disclosed frozen
comparisons, totaling **1,145.725 seconds** of measured call wall time. This includes
all failed comparisons and preparations, not just the best writer results. No
production prompt, metadata loader, parser, model configuration or database was
changed. No larger model, history injection or publication was used. The frozen
Liverpool/Boston/Green Bay validation packets remain unused for generation.

The [meaning/order comparisons](semantic-preparer/README.md) add explicit preparation
instructions, then change source-schema emission order. Neither improves reliable
whole-packet output. They account for 70 calls and 335.747 seconds of the totals.

The [latest copied-evidence comparison](copied-evidence/README.md) prevents invented
intermediate prose and reaches five clear packets. It adds 41 calls / 203.086 seconds,
including a zero-preparation-model control. One writer source-ownership error
remains, so the predeclared reliability gate still fails.

## What was tested

The same six real stored-headline packets and their 29 distinct articles were
served in production-derived field order. Every original headline, publisher,
article ID, URL, metadata value, tone and topic membership was preserved. The
writer receives original source evidence plus optional preparation; notes never
replace the source. These grouped inputs and statement output schema remain
experimental, not an end-to-end production replay.

The preparer sees exactly one article's headline, publisher, timestamp, evidence
scope and ID. It receives no team background, neighboring reports or tone. Code
attaches the result by durable article ID, retaining source provenance. The
writer then receives all three contexts and the complete original reports.

Three writer arms use the same writer instructions within each comparison:

- **Control:** original sources only.
- **Reference:** sources plus manually reviewed preparation. This is a diagnostic
  control, not independently adjudicated gold or a scalable production stage.
- **Automatic:** sources plus untouched 4B-generated preparation. Invalid copied
  fields block the affected packet; no report is dropped, note repaired or
  fallback silently invoked. Semantic errors that pass copying checks remain
  visible in the end-to-end trial.

All preparation and writer outcomes are reviewed against their exact original
headline evidence, including titles, qualification, source ownership and grammar.
No independent verification of the underlying stored news or metadata is claimed.
Repeated selected stress packets do not establish a general accuracy rate.

## Initial three frozen comparisons

### 1. Initial preparation contract: 45 calls

The first contract used `actor`, `kind`, `claim`, copied `qualifications`, and
`unspecified` details. It exposed two failures:

- `actor` conflated who an opinion concerns with who expressed it. A writer given
  manually prepared trade notes invented **Donte Jackson's opinion**, although
  the source only discussed a possible trade involving him.
- The new writer contract regressed even without notes, merging three assigned
  topics into one. It retained explicit prose rules but lacked the earlier
  writer's worked examples. This is a poor basis for concluding preparation
  itself cannot help.

All 29 preparation calls completed. Four failed exact-copy validation: an invented
“by Milan” qualification for Juventus, misspelled Athekame, and two altered
quotation-mark spans. Some also had semantic errors independently of copying.
Two Milan packets were blocked before automatic writing; the other four automatic
writers all failed topic coverage. None of the three arms had a fully usable
packet under the whole-output criteria.

Copied spans were valid in 25/29 preparations. That did not establish truth:
notes also treated a supplied draw outcome as unknown, presupposed an injury,
and described a substitution as though it had happened when the source said it
was forgotten. [Initial review](review.json), [protocol](protocol.txt),
[measurements](measurements.json).

### 2. Explicit roles and exact count: 12 writer calls

The second screen distinguishes `about` from `speaker`. An opinion about a
person does not make that person its speaker; null speakers cannot be filled from
metadata. The output schema requires exactly the three already assigned topics.
This is an input-derived structural constraint, not a truth check.

The Donte speaker error disappeared in the reference case, but the writer often
filled three slots by duplicating the same cross-topic roundup. Exact array
length did not ensure topic membership. Neither arm had a fully usable packet.
The predeclared reference gate failed, so **no additional automatic preparation
calls were spent on this writer contract**. [Role screen review](roles/review.json),
[protocol](roles/protocol.txt), [measurements](roles/measurements.json).

The role contract also defines source-derived enum choices for copied fields:
all contiguous source word spans, optionally stripping edge punctuation, with
every choice checked as an exact original substring. This avoids misspelled
names and quotation drift without entity heuristics or case-specific name lists.
It does not validate that a copied name is the right speaker or that a free-text
claim follows from its source. This extraction schema was only exercised in the
qualified third comparison.

### 3. Retained worked-example writer: 47 calls

The third screen restores the complete earlier worked-example writer and adds
only the optional-note authority and subject/speaker instructions. The same role
notes and exact topic-count schema are retained. Both writer arms use an output
reservation of 1,200 instead of 2,100 to fit full instructions plus notes within
the existing 7,900 context. This changes both instructions and the output cap;
it is not a single-variable prompt comparison. All calls ended at EOS, so no
output was clipped by the smaller cap.

| Arm | Clearly faithful complete packets | Clarity concern | Failed |
| --- | ---: | ---: | ---: |
| Raw control | 2/6 | 1/6 | 3/6 |
| Manual reference | 6/6 | 0 | 0 |
| Automatic preparation | 4/6 | 1/6 | 1/6 |

The reference result qualified automatic testing before any automatic outputs
were inspected. All 29 role-separated preparation calls then passed copy/shape
validation; their semantic content was still reviewed separately.

The automatic result preserves Pavlovic, keeps the Bears' streak unexplained,
retains both factors needed for the possible return and the unlikely-exit caveat,
and avoids turning Donte into an opinion holder. The remaining definite failure
is the known Milan packet: the amount becomes a value, the match statement
imports competition/goalkeeper details from other reports, and the required
match anchor is omitted. The article's preparation independently invents a
transfer value. Since the control also interprets the amount, this does not prove
that the note caused the writer's value interpretation.

The Bears known packet has faithful bodies but a title saying “Patriots game
listing” instead of identifying the live show. This is conservatively retained
as a source-format clarity concern, not counted as a proven fabrication or a
clear whole-packet pass. Four other automatic packets are faithful and readable,
although some wording remains mechanical.

Other prepared notes contain semantic problems that the final writer does not
necessarily repeat: broadening a missed-substitution reaction into a performance
judgment, detaching the trade condition in a redundant secondary statement,
blurring which team had ten players, and attaching return qualifications to the
wrong subject. **29/29 valid copied spans is not 29/29 faithful interpretations.**

Automatic performance falls below the predeclared five-clear-packet gate. No
repeat or new-subject qualification calls were spent and no candidate is promoted.
[Full worked-writer review](worked-writer/review.json),
[manual screen selection](worked-writer/screen-selection.json),
[automatic stopping decision](worked-writer/selection.json).

## Two preparation follow-ups: 70 calls

Expanded relationship/unknown/duplication instructions reach 3/6 clear packets,
one clarity concern and two failures. Holding those instructions fixed while
emitting claims before labels reaches 2/6 clear, one concern and three failures.
Statement count falls from 52 to 41, but the preparer still invents meaning with
only one source present. Final writing also retains independent inventions.
Both gates fail; neither arm receives repeat or untouched validation calls.
[Full comparison, failures, exact wire-order audit and cost](semantic-preparer/README.md).

## Copied-evidence follow-up: 41 calls

The preparer now selects exact source clauses and unresolved expressions, retaining
the original writer. All 29 outputs pass independent all-field copying checks, but
role/classification/selection concerns remain. Final writing reaches 5/6 clear
packets, versus 2/6 for a same-session complete-headline-copy control. A statement
still borrows competition/goalkeeper details from neighboring sources. Automatic
preparation plus writing costs 156.493 seconds versus 46.593 seconds for that control.
[Full comparison, stopping decision and audits](copied-evidence/README.md).

## Measured processing cost

Third comparison (worked writer), six packets and all 29 source preparations:

| Work | Calls | Total call time | Median per call |
| --- | ---: | ---: | ---: |
| Raw control writing | 6 | 44.38 s | 6.72 s |
| Manual-reference writing | 6 | 51.19 s | 7.37 s |
| Automatic source preparation | 29 | 112.60 s | 3.32 s |
| Automatic writing | 6 | 49.61 s | 7.40 s |
| Automatic preparation + writing | 35 | **162.21 s** | — |

Automatic combined packet cost ranges from **16.54–53.42 seconds**, median
**21.86 seconds**, versus direct writing's median 6.72 seconds. The most expensive
packet still fails. The combined aggregate is **3.65×** direct writing time,
using 63,138 input tokens and 3,153 output tokens versus 31,144 and 968 for direct
writing. The preparation source set includes every served report, even when the
writer never cites it. No useful source reuse across these six packets was
observed; repeated text from different publishers retains separate provenance.

Wall time includes native request preparation/tokenization, schema processing,
generation and response handling, with writer rendering/evaluation included. It
excludes separate admission preflights, compilation, setup, manual grouping,
manual reference creation and review. No compilation or other experiment ran
concurrently with inference. Cache-prefix reuse varies and is saved in receipts.
The totals are observed local timings, not cold-start or saturated throughput.

Reusing preparation across characters is a possibility to measure, not an assumed
six-way cost reduction. No production cache was implemented. Any future reuse key
must bind source content/scope, prompt/schema and model revision, not merely article
ID. Daily workload capacity, other characters and scheduler/retry behavior remain
unqualified. [Final measurements](worked-writer/measurements.json).

## Artifacts and verification

- `examples/journalist_source_trial.rs` adds a read-only per-source runner with
  native input/completion guards, original provenance, raw receipts and copy
  validation. It accepts both retained actor and revised subject/speaker forms
  so earlier experiments remain reproducible.
- `examples/journalist_assembly_trial.rs` adds an optional bounded reduction in
  output reservation; the default remains unchanged. It never raises the default
  budget or accepts zero/negative budgets.
- Builders freeze source and case manifests, preserve production field order,
  validate reference spans and keep manual notes distinct from model output.
- `run.py`, the two `run_screen.py` scripts and the worked-writer automatic runner
  retain every attempted generation and gate further work. New output directories
  are required to rerun. The automatic attachment ledger retains original URLs.
- `analyze.py [experiment-directory]` independently checks complete native token
  coverage, EOS/nontruncation, frozen wire input, source provenance, schemas,
  source-bound rendering, combined cost and unchanged serving identity metadata.

All **215 generation receipts** pass complete-input/EOS/nontruncation checks.
The four assembly tests and two source-runner tests pass; builds, formatting,
diff checks and deterministic artifact reconstruction pass. These checks validate
implementation and reproducibility, not semantic truth. The initial trailing-
newline admission failure made no generation call; see [preflight note](preflight-note.txt).

Copied source evidence improves the final count, but correct copying cannot stop
the writer borrowing from other reports. The next proposed test binds copied
evidence to each selected report during writer generation, avoiding separate
per-source preparation calls. It must still prove paraphrase fidelity and earn
its cost. No prepared-note sharing across characters is yet qualified.
