# Journalist assembly investigation — 1 November 2026

**Unresolved: no tested variant qualifies for rollout.** The task is natural,
faithful summaries on existing hardware. Larger models, extra repair passes,
parser acceptance and synthetic-only successes do not establish that outcome.
History stays deferred; team eligibility stays with the decision model.

## What changed in the legacy path

The pre-plugin Journalist did not receive today's evidence. Its
`load_packet_corpus` consumed Editor-extracted claims, grouped by article with
publisher provenance and explicit disagreement markers. Separate desk framing
described the storyline. It bounded storylines and source bytes, recording
exclusions. See the retained [loader excerpt](legacy-packet-loader.txt).

The specific removal is commit **7a6b6215, 28 September 2026**, after the plugin
migration. Its [commit record](cutover-commit.txt) says it replaced the Editor
packet path with natural articulation over plugin-prepared reporting. It also
records unresolved added details, headline qualifications and source-order
drift despite structurally accepted output. Later changes switched delivery to
native spectrum receipts (`5fe62918`) and headline-only input (`8a9a5507`).

Today, source interpretation and grouping are part of the writing call. This is
a confirmed change in responsibilities, **not a controlled demonstration that
restoring the old Editor would fix everything**. This investigation did not
replay the complete old pipeline or independently qualify its historical prose.
Do not restore old generated claims as if they were publisher evidence.

## Controlled assembly trials

Four frozen cases cover metadata leakage, conflicting reports, an unspecified
speaker in a quoted example, and 24 retained Chelsea headlines. All local trials
use Qwen3.5-4B-Q4_K_M, temperature zero, thinking disabled, one attempt per
variant, and no publication. The inputs, full replies and receipts are retained.
`run-index.json` counts mechanical outcomes only; missing checks are null.

- `layout-order-complete`: JSON versus source blocks, and body-first versus
  references-first schema order. Changing order changes output, but does not
  eliminate metadata leakage or source reassignment. Source blocks alone do not
  restore the legacy claim-preparation stage.
- `metadata-role`: moving database context into the system message does not
  establish a reliable reporting boundary.
- `attributed-input`: explicitly wrapping each source as a publisher statement
  still leaks metadata and changes speakers in titles.
- `compact-layout-order-complete`: a shorter contract permits correct abstention
  on the profile case, but misattributes the quotation and larger-packet claims.
- `evidence-first`: copying exact source text before summarizing still produces
  unsupported prose. Exact copies are not proof of faithful paraphrases.
- `unconstrained`: removing the grammar retains factual failures and produces
  malformed/duplicated output in three short cases.
- `chat-endpoint`: the server's chat endpoint also leaks metadata and changes
  attribution. Prompt token counts match the native controls; cache handling
  differs, so endpoint timings are not a clean performance comparison.
- `n118-baseline`: the accumulated prompt rules help short bodies, but a title
  makes Club Ledger the denier instead of the club. The larger case misattributes
  reports and ends its body mid-thought at the schema length limit.
- `summary-contract`: changing the model-facing keys to `summaries` and `topic`
  passes all four mechanical checks. It is **not a fidelity pass**: the Chelsea
  body changes games Emi Martinez will miss into games Chelsea will miss.

The failed `layout-order` block variants and `compact-layout-order` runs were
preflight failures: the template trimmed trailing whitespace. Corrected runs
have separate directories. These failures made no inference call and must not
be scored as model quality failures. `rendered-baseline.txt` verifies the actual
system/user/assistant boundaries and disabled-thinking prefix; there was no
observed omission of the supplied contexts.

None of these experimental renderers or output schemas is in production code.
The longer n114–n118 prompts remain artifacts. At the end of this investigation,
the working pilot retained n113, also unqualified. The subsequent
[detailed-instruction pass](../detailed-instructions-2026-11-01/README.md) records
the n119 working candidate and its remaining failures.

## Hardware and the actual workload

The read-only snapshot concerns the still-open 23 October edition, not a normal
new day's workload forecast. There were 1,589 pending Journalist assignments;
their frozen source memberships had median 4 and maximum 426. Memberships are
not delivered reports or inference calls.

The production loader was then exercised on the ten largest assignments, with
read-only database sessions and exact native tokenization. **Every one delivered
40 reports**, reflecting the existing selection policy in migration 323. All
fit the configured 10,000-token request limit and 10,240-token physical slots:
2,944–3,156 input tokens plus 2,100 reserved output, totaling **5,044–5,256**.
This uses frozen n113 instructions and real database metadata. No generation or
publication occurred. See [capacity-summary.json](capacity-summary.json), the
full `live-capacity/results.jsonl` receipts, and the audit SQL files.

Thus the 426-membership count does not establish an oversized current prompt.
This sample also does not establish daily completion time: other characters,
actual no-call/reuse rates, retries, queue backoff and concurrent throughput
remain part of the nightly budget. The existing October 20 capacity/replay
measurements are historical evidence, not a new end-to-end qualification.

## Durable changes and remaining work

The pilot's parser now checks the exact evidence visible to the writer:
headline-only input cannot be validated using a hidden excerpt, and excerpt-only
input cannot be validated using a hidden title. Publisher provenance applies to
every sentence of its source. Positive and negative regression tests cover these
boundaries. Evaluation exclusions now cover titles as well as bodies.

These are evidence-boundary corrections, not semantic truth verification.
The numeric guard still rejects some faithful combined clauses, and mechanically
accepted prose can still be wrong. No heuristic relaxation was made to get a
green score.

The next useful comparison is the **evidence-preparation responsibility** that
was removed: the same source corpus, same model, explicit source-bound meaning,
and complete accounting of preparation cost. Any upstream work must be measured
once per reusable source, including its errors and reuse across characters;
adding a pass per character or silently dropping evidence is not an established
solution. The untouched qualification cases in the sibling summary-fidelity
directory remain available after a candidate passes development review.

## Reproduce

Run `journalist_assembly_trial` with `cases.jsonl`, a saved system file, a new
output directory, endpoint and model. Its optional mode selects a named trial;
omitting it compares layout and property order. It never accesses the database.

`journalist_capacity_audit` takes `largest-subjects.jsonl`, the saved system file,
a new output directory, endpoint and model, with
`JOURNALIST_REPLAY_DATABASE_URL` in the environment. It enforces read-only
connections, calls the production loader, and only prepares/tokenizes requests.
It does not run inference, settle deliveries, or modify the paused edition.

## Code validation

388 active library tests, 14 evaluation CLI tests and the assembly runner test
passed; 74 environment-dependent library tests were ignored. All targets compile;
formatting and diff checks passed. These checks validate the implementation, not the factual
quality of generated prose or completion of the full daily workload.
