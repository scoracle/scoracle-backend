# Copied evidence preparation — 2026-11-01

**Copied-evidence preparation reaches five clearly faithful packets out of six,
the best automatic result in this investigation, but still fails the reliability
gate.** The final writer continues to borrow details across source boundaries in
one packet. Preparation plus writing takes 156.493 seconds, versus 46.593 seconds
for the same-session control that copies headlines without a preparation model.

Same 29 real stored headlines, six development packets, Qwen3.5-4B-Q4_K_M and
byte-identical worked-example writer. Original headlines, metadata, tone, source
links, grouping, field order and writer output schema/settings remain intact.
These are experimental grouped packets, not a production pipeline replay.

| Arm | Clear packets | Clarity concerns | Failed packets | Calls | Call wall time |
| --- | ---: | ---: | ---: | ---: | ---: |
| Historical raw-source control | 2/6 | 1/6 | 3/6 | 6 | 44.383 s |
| Historical best generated preparation | 4/6 | 1/6 | 1/6 | 35 | 162.209 s |
| Same-session complete-headline copy control | 2/6 | 1/6 | 3/6 | 6 | 46.593 s |
| Model-selected copied evidence + writing | **5/6** | 0/6 | 1/6 | 35 | **156.493 s** |

The six packets have been used repeatedly for development. Counts are manual,
unblinded source-fidelity judgments, not general accuracy rates or independent
adjudication. A complete pass includes meaningful qualifications, actual cited
source ownership, titles and required anchors. Minor supported label-style
departures are disclosed in the review and are not treated as fabricated facts.
Historical controls were reused, not rerun or interleaved.

## What changed

The model preparer still returns about, speaker, kind, claim, qualifications and
unspecified. Its `claim` is now a contiguous exact source span, never a generated
paraphrase. `unspecified` holds copied expressions needing restraint, such as
“60 million” or “27-year streak”, rather than generated explanations. Both fields
are constrained to the source-span enum already used for names and qualifications.
Only the fixed kind labels are allowed non-source text. Detailed instructions
require whole propositions, question/negation preservation and qualification
binding; copying a complete headline is permitted when a shorter span is unsafe.

The writer prompt is unchanged, including its instruction that preparation is
fallible and original headlines remain authoritative. Changing what claim and
unspecified contain is explicitly part of the tested preparation contract. Code
does not fix the model's wording, labels, selection or relationships.

The cheaper control attaches only:

```json
{"statements":[{"claim":"<complete original headline>"}],"unspecified":[]}
```

It adds no invented role/type/qualification fields and makes no model call before
writing. This separates the benefit of model-selected annotations from simply
repeating evidence. It is an experimental control, not a proposed production
duplication policy. The full-copy control alone does not improve on the historical
raw-source pass count.

## What improved and what remains

All 29 automatic preparations pass an independent check that every textual value,
apart from kind labels, occurs exactly in its own source. The 39 prepared statements
include whole-headline claims in 24 sources. The earlier invented Juventus loss and
broadened substitution judgment do not appear in these copied claims. Pavlovic's
full assessment and qualification remain intact, and undefined amounts never
become generated fee/value explanations during preparation.

This does not make every annotation correct. Examples include a null speaker for
an explicit Bears announcement, redundant full streak claims, unnecessary unknown
flags and how-to headlines labeled listing. Exact words do not establish a correct
role, classification, selection or relationship. Every preparation has a separate
review entry; no semantic correctness is inferred from the copy check.

The automatic writer preserves all four prior clear packets and resolves the
previous live-show title concern, producing five clear packets. It retains the
two-factor return requirement, unspecified streak and illness/availability without
inventing a coach role, streak type or opponent.

The remaining failed packet is `ac-milan-known`. The original and copied report7
say Bournemouth 3–0 Milan, Maignan sees red and the Rossoneri implode on the south
coast. The final statement bound to report7 adds **Europa League** and **goalkeeper**.
Those details occur in neighboring reports, but not in report7 or its preparation.
The required anchor is now present and the substitution reaction is narrow, yet
individual statement attribution is still wrong. This locates that final error
after faithful copying, in the writer's source ownership.

[All 29 preparation and 12 writer reviews](review.json),
[frozen prompt](preparer.txt), [protocol](protocol.txt), [selection](selection.json).

## Processing cost

| Work | Calls | Total call time | Median per call | Input/output tokens |
| --- | ---: | ---: | ---: | ---: |
| Complete-headline copy control writing | 6 | 46.593 s | 6.561 s | 32,036 / 1,039 |
| Automatic copied-evidence preparation | 29 | 106.693 s | 3.144 s | 41,744 / 1,919 |
| Automatic writing | 6 | 49.800 s | 6.796 s | 33,115 / 1,038 |
| Automatic combined | 35 | **156.493 s** | — | **74,859 / 2,957** |

Automatic combined cost is **3.36×** the same-session complete-copy control and
**3.53×** historical direct writing. Combined per-packet time ranges 16.272–49.545
seconds, median 20.776 seconds. The most expensive packet remains the failed one.
No preparation is excluded from cost because its report went uncited.

This comparison adds **41 calls / 203.086 seconds**, bringing the source-preparation
investigation to **215 calls / 1,145.725 seconds** of measured call wall time.
Totals include native request/tokenization/schema/generation/handling work, but
exclude separate admission preflights, setup and manual review. Prefix cache usage
is retained in receipts. These are sequential observed timings, not a paired
speed benchmark or daily-capacity result; no six-character amortization or
persistent cache saving is assumed.

## Verification and stopping decision

The existing Rust harnesses were reused without code changes. New Python helpers
freeze cases/schemas, check copied strings before any automatic writer call, and
retain native receipts and original source attachment ledgers. A failed copy
check stops the automatic writer stage, with no replacement or fallback. Local
self-checks reject invented claims/unknowns and explicitly demonstrate that a
copied fragment can still change meaning.

All 41 native receipts pass complete-input, exact admission, schema, stable model
identity, EOS and nontruncation checks. The independent audit verifies new enums,
raw HTTP schema bytes, actual property order, source-span membership, unchanged
writer/settings and exact source/metadata/tone restoration after removing notes.
Builder reconstruction, Python compilation, local guard checks and whitespace
checks pass. No Rust build/test rerun was needed because those files were unchanged.

The predeclared gate requires at least five clear packets **and no factual or
coverage failure in the sixth**. That second requirement fails. No repeated writer
or reserved Liverpool/Boston/Green Bay validation calls were spent. No model,
runtime, database, publication, history or gender-decision behavior changed.

Reproduce the audits with `python3 analyze.py` from this directory; rebuild the
frozen inputs with `python3 build_inputs.py`. A generation rerun requires fresh
output directories to retain the existing evidence.

[Measurements and receipt audit](measurements.json), [cost comparison](comparison.json).

## Next target

Test source ownership at the point where the writer generates each statement:
bind a copied evidence quote to its selected report before producing the natural
clause. Evaluate this within the writer call, without the 29 separate preparation
calls. That is the next proposed experiment, not an implemented or qualified fix;
copy correctness alone still cannot validate the resulting paraphrase.
