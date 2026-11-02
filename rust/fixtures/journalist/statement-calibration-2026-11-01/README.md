# Worked examples and explicit evidence gaps — 2026-11-01

Follow-up: the [metadata and serialization investigation](../production-order-2026-11-01/README.md)
tests ordering, section boundaries, typed metadata and causal interventions. It
also identifies recursive fixture-key sorting as a difference from production
delivery. The outputs below remain preserved for their exact experimental inputs.

**Worked examples improve specific distinctions but do not make the 4B writer
reliably honest or readable.** A missing-detail annotation field makes coverage
worse. Neither candidate is promoted. The experiment preserves complete inputs,
original source IDs/URLs, raw outputs and failure receipts.

This follows the [statement-ownership experiment](../statement-ownership-2026-11-01/README.md).
It uses the same Qwen3.5-4B-Q4_K_M, source-bound statement renderer and existing Mac.
Fresh/meta/tone remain intact; history remains deferred. No production prompt,
parser, model configuration, database records or publication path changed.

## Frozen comparison

[Protocol](protocol.txt), [development hashes](development-manifest.json),
[new-subject groups and hashes](validation-manifest.json), and
[candidate selection](selection.json) retain the sequence and decision criteria.

- Six previously tested packets, unchanged control versus general worked examples
  versus those examples plus an `unspecified` string array: 18 calls.
- Repeat the selected examples candidate on the same six packets: 6 calls.
- Compare control and examples on Arsenal, Tampa Bay and Cleveland packets frozen
  before development inference: 6 calls.

Total: **30 calls**, one model call per packet, temperature 0, thinking off,
native cache enabled, context budget 7,900 and reserved output 2,100. No retries,
post-generation claim repairs, keyword exception lists or prompt changes after
seeing validation results. Validation subjects came from a fresh read-only
production-loader capture, with 40 original reports per subject retained in
`live-capture`; the writer sees only the reviewed topic groups. Group preparation
was manual and is not qualified as an automated production step.

The control exactly reproduces the previous structured prompt, input and six
parsed outputs. The examples candidate adds general guidance and synthetic
worked examples for unspecified numbers/streaks, grammatical attribution,
conditional returns and titles that keep distinct roles separate. The third arm
asks the model to name missing details before writing each claim. The renderer
validates that optional array's shape, ignores it in prose, and never treats it
as proof. It attaches the cited source's exact publisher without rewriting the
model's clause or title.

## What improved, what failed

Development review counts unique packets, not repeated generations:

| Arm | Clearly faithful | Additional clarity concern | Failed |
| --- | ---: | ---: | ---: |
| Structured control | 2/6 | 1/6 | 3/6 |
| Worked examples | 4/6 | 0/6 | 2/6 |
| Examples + missing-detail field | 1/6 | 2/6 | 3/6 |

Worked examples preserve the Bears live-show wording, Milan's two required
return factors and the unlikely-exit caveat. They remove the false joint injury
title, correctly label Gonzalez's illness, and stop inventing the type of the
27-year streak. They still add an unsupported coach role to the streak report.

The serious regression is Milan: **Pavlovic's assessment becomes Filippo Galli's
assessment in both title and body.** Galli appears only in the database background.
The same output still interprets an unexplained 60 million as a valuation. Valid
statement-to-source IDs do not prevent either error. All six candidate repeats
produced identical parsed output, including these mistakes.

The missing-detail field fails topic coverage in three of six packets. One
response labels Tresoldi, January and Maignan as missing even though those details
are supplied. Another invents an NFC North streak while emitting an empty gap
array. This extra model-generated self-check is not dependable.

The three new subjects expose additional limits:

| Subject | Control | Worked examples |
| --- | --- | --- |
| Arsenal | Assigns timeline confirmation to Arsenal; source leaves confirmer unspecified and frames it as transfer rumours | Same unsupported agency and lost rumour framing |
| Tampa Bay | Splits one trade topic, names an unidentified star as Vita Vea, drops Mayfield; renderer rejects | All topics restored, star stays unnamed, practice condition preserved; malformed “bill point” phrase remains |
| Cleveland | Facts stay source-bounded, but two `discusses` clauses are ungrammatical | Same grammar failure despite explicit guidance |

**Neither arm produces a fully satisfactory packet on all fidelity and
readability criteria for any of these three subjects.** This does not mean every
sentence is false. Cleveland preserves the supplied facts; Tampa's candidate is
a material coverage/identity improvement with a remaining clarity defect.
Arsenal demonstrates an actual unsupported assignment of agency that the parser
accepts. The distinction is recorded rather than folded into a parser pass rate.

This is Codex's manual source-by-source review, not blinded or independently
adjudicated. It checks fidelity to the supplied headlines, not whether the stored
news or database metadata is itself true. These selected cases are not a general
accuracy estimate. [All 24 unique output reviews](review.json) include titles,
qualifications, source ownership, coverage and naturalness; six repeated outputs
share their original reviews.

## Processing cost and integrity

| Run | Calls | Median wall time | Range | Input tokens | Output tokens |
| --- | ---: | ---: | ---: | ---: | ---: |
| Control, development | 6 | 7.88 s | 5.79–17.16 s | 3,912–4,321 | 133–236 |
| Examples, development | 6 | 8.12 s | 5.87–20.24 s | 4,925–5,334 | 133–237 |
| Gap field, development | 6 | 8.96 s | 5.71–25.35 s | 5,152–5,561 | 127–278 |
| Examples, repeat | 6 | 8.09 s | 5.91–19.97 s | 4,925–5,334 | 133–237 |
| Control, new subjects | 3 | 10.70 s | 7.33–15.93 s | 3,924–4,168 | 148–251 |
| Examples, new subjects | 3 | 10.98 s | 9.72–11.13 s | 4,937–5,181 | 152–254 |

Worked examples add exactly **1,013 input tokens per call**, about **25.1%**
across the six development packets. The gap contract adds another 227 input
tokens and extra generated annotations. No candidate adds an inference stage.
All 30 calls totaled **336.64 seconds** of measured request-to-result wall time,
including rendering and response capture. Lower output cost from dropping topics
is a failure, not an optimization.

Cache reuse differs across calls; these wall times are observations, not a stable
0.24-second overhead claim. The first control validation request had one cached
token, while the first examples validation request reused 3,408. Native receipts
retain cache, prefill and generation timings. Ordinary system activity was present;
no compiler or other experiment was launched during inference. See
[timing limitations](timing-limitations.txt). Manual topic preparation and review,
production scheduling, retries, parallel saturation and other characters are
excluded. **Daily-workload capacity remains unmeasured.**

All 30 requests consumed the complete admitted prompt, ended at EOS, and were not
truncated. Maximum input plus reserved output was 7,661, below 7,900. Frozen-input,
schema, renderer, source-boundary and model-identity checks passed. The six repeat
requests and their parsed outputs match exactly. Source mapping succeeds in all
15 examples calls, 8/9 control calls and 3/6 gap calls; this metric is separate
from semantic correctness. Mechanical acceptance is 7/15, 3/9 and 1/6 respectively.
No parser guard was relaxed for either false positives or false negatives.

## Reproduction and next decision

- `build_development.py`, `build_validation.py`: rebuild the immutable fixtures
  from prior inputs and the captured production-loader data.
- `run_development.py`, `run_validation.py`: execute the frozen matrix, exact
  context admission and model-identity checks. Output directories must be new.
- `analyze.py`: independently validates requests, receipts, rendering, repeats
  and hashes, and writes [measurements.json](measurements.json).
- Each run directory retains `input.json`, raw `output.json`, native
  `receipt.json`, parser results, and `rendered.json` only when mapping succeeds.

The example-only harness change accepts and validates the optional gap field and
preserves its generation order. Focused tests cover that order, unchanged prose
rendering, and existing source/topic rejection behavior. The two harness tests,
build, formatting, diff checks and deterministic artifact checks passed.

The next useful optimization is a controlled test of **how metadata is separated
from source evidence in the assembled request**, keeping its actual attributes
and the model constant. The observed metadata leakage supports testing that
boundary; it does not yet prove which assembly change will resolve it. Keep the
known Milan failure as a regression case and reserve fresh subjects again before
choosing a new prompt. Adding more examples alone has not earned rollout, and
correct source links cannot substitute for source-faithful claims.
