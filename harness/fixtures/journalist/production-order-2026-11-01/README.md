# Production-derived field order — 2026-11-01

Follow-up: the [source-preparation investigation](../source-preparation-2026-11-01/README.md)
tests source-scoped claims with separate subject/speaker fields, complete original
evidence and full preparation-plus-writing cost.

**The experimental serializer was changing the model's input order. Restoring
captured production order fixes the known Milan speaker substitution and its
cross-publisher match detail errors in this run, but does not make complete
summaries reliable.** Future experiments should derive their wire input from the
production capture/assembler and preserve ordering; canonical sorting is suitable
for comparisons/hashes, not a replacement for the delivered prompt.

This is the final six-call check after the
[context-boundary matrix](../context-boundaries-2026-11-01/README.md) and
[typed-record/diagnostic trials](../metadata-fields-2026-11-01/README.md).
Across the three experiments: **45 calls, 330.327 seconds of measured call wall
time**, comprising 42 real-metadata calls and three explicitly altered metadata
diagnostics. They are repeated selected stress packets, not 45 independent
subjects. No candidate earned production promotion or new-subject qualification.

## Correction and controls

Earlier grouped fixture builders serialized with Python `sort_keys=True`.
That changed top-level order to fresh/meta/tone, metadata's internal order and
report-field order. Production uses meta/fresh/tone and its Rust struct-defined
field order. The initial four-way matrix isolated top-level order/section labels;
this final check also restores the nested production order.

`build_inputs.py` reads the original production-loader capture, first asserting
that its JSON round-trips byte-identically. It retains captured metadata/report
object order while constructing the already frozen groups. Because the earlier
packet-size test promoted an anchor to slot1, the builder joins by **durable
article ID**, then retains this experiment's report keys. It verifies equality
of every stored report, source URL, metadata value, group membership and parsed
input value. It does not match articles by incidental array position.

The first builder attempt refused a report-order mismatch and made zero inference
calls. The corrected durable-ID join is documented in [preflight-note.txt](preflight-note.txt).
No earlier fixture or output was rewritten. This builder is the concrete
correction for future controlled comparisons.

The existing examples system, source-bound statement schema, group wrappers and
source membership remain experimental and unchanged. **This is not an end-to-end
production replay:** it tests production-derived serialization within the same
grouped statement task. Production still uses its own prompt/output contract.

## Full-output review

| Packet | Result |
| --- | --- |
| Milan, known | Pavlovic and each publisher's match details preserved; unspecified 60 million still becomes a value |
| Bears, known | Body retains live show; title calls it game broadcast, leaving a clarity concern |
| Clippers, known | Absence and fixture-page listings remain faithful |
| Milan, reserved | Two required factors and unlikely exit preserved; shared injury title remains misleading |
| Bears, reserved | Streak type stays unspecified; unsupported coach role remains |
| Clippers, reserved | Separate performances and Yahoo-only opening-night information remain faithful |

Conservative whole-packet review: **2/6 clearly faithful/readable, 1/6 clarity
concern, 3/6 unfaithful**. This is below the same experiment's sorted control at
4/6; eliminating the speaker substitution is not a net reliability improvement.
Both mechanical acceptances contain defects: the Milan injury title and Bears
coach role. All six source mappings are valid. [Complete review](review.json).

Judgments are by Codex against supplied headline evidence, not blinded or
independently adjudicated. The source news and database attributes are not
independently verified. “Faithful” means faithful to that supplied evidence.

## Cost and verification

Same Qwen3.5-4B-Q4_K_M, temperature0, thinkingoff, cache on, context7,900,
output reservation2,100. Input4,926–5,335 tokens; output137–238. Median wall time
**6.09 seconds**, range5.78–9.91; total **43.365 seconds**. All six calls reused
4,610 prefix tokens. No extra inference stage is proposed. These are warm-prefix
measurements, excluding manual grouping/review, scheduling and mixed-character
load; daily capacity is not established.

All six receipts consumed the complete admitted input, ended at EOS, were not
truncated and retained unchanged model identity metadata. Frozen input, source,
schema and deterministic-rendering checks passed. The updated experiment harness
has three passing focused tests, and its build, formatting and diff checks pass.
Production prompt, parser, metadata loader and database are unchanged.

[Protocol](protocol.txt) · [Manifest](manifest.json) · [Measurements](measurements.json)

## What the combined evidence establishes

1. Metadata content and context serialization can change source attribution in
   this model. Typed JSON fields and section labels do not enforce an evidence
   boundary. Counterfactual names were not blindly copied, so the mechanism is
   more specific than always substituting whichever coach is supplied.
2. Source interpretation errors survive without the background record. The
   model still invents the meaning of a number and transfers details between
   publishers. Removing metadata cannot be treated as the full fix.
3. New tests must preserve the production wire format when they claim to test
   production behavior. Old experimental outputs remain valid for their exact
   requests, but cannot establish what the differently serialized runtime does.

The next bounded investigation should address source-scoped claim preparation:
keep the original actor, claim, uncertainty and source together before prose
composition, and count any preparation cost once per reusable source. The legacy
pipeline previously had an Editor-prepared claim stage; restoring it is a
hypothesis to test, not a proven solution. Avoid promoting another layout on the
strength of a corrected example alone.

Liverpool, Boston and Green Bay remain frozen and unused for generation because
none of these candidates passed development. No performance or accuracy claims
are made for that untouched set.
