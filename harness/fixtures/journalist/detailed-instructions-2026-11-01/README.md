# Detailed Journalist instructions — 1 November 2026

**Working candidate: `n119-detailed-summary`. Not qualified for rollout.**
The user requested thorough instructions while retaining natural summaries and
the existing hardware. This pass changes the instruction contract and enables
native prefix reuse in the working client. The model, input packet, output schema,
validators, retries and deployed services remain unchanged.

## One instruction source

[`src/plugins/journalist/instructions.txt`](../../../src/plugins/journalist/instructions.txt)
contains the complete contract, compiled through `prompt::instructions()`.
Runtime, input hashing and evaluation use that function. The file matches the
frozen `instructions-v5.txt` after trimming file whitespace. Quality fixtures use
the new prompt version; their input contexts are unchanged.

The approximately 1,940-word contract defines:

- Meta as database identity/background; fresh as reporting evidence; tone as
  wording. History remains absent, and eligibility belongs to the decision model.
- Publisher, speaker, subject and action as separate roles, with explicit rules
  for unnamed speakers, original outlets, corrections and conflicting reports.
- Questions, possibilities, plans, denials, missing information, publication
  dates, numbers and descriptive attributes without strengthening their meaning.
- Separate topics, short attributed bodies, and a `Named subject: specific topic`
  title format that is intended to prevent an extra assertion in the title.
- Six complete input/output examples: profile topics versus database answers,
  disputed claims, independent topics, unspecified quotation speakers, separate
  numerical statements, and an outlet correcting its own report.
- A final source and attribution check, with the existing JSON form and bounds.

These are instructions to evaluate, not guarantees that the model obeys them.

## Iteration and frozen evaluation

All inference used Qwen3.5-4B-Q4_K_M on the Mac, temperature zero, thinking off,
and one attempt per case/variant. No database writes or publication occurred.
The existing replay runner retains full instructions, input, output and receipts.

`v1-development` tests the first prose-heavy manual. `v2-development` adds complete
worked examples; profile leakage and some grouping improve. `v3-layout` compares
field order/layout on the retained 24-report case; references-first produces
large, inaccurate roundups, so the production order is retained. `v4-development`
adds numerical/correction examples. `v5-development` makes the topic-label format
and final procedure explicit. `v6-development` changes selection wording and an
example for compressed headlines; it regresses on the larger packet, so v5 is
the working candidate. Every run remains available in `run-index.json`.

After selecting v5, its instructions were frozen for eight previously uncalled
qualification cases and three archived production-loader packets of 40 reports
each. The qualification cases were not used to revise the selected instructions.
They are now consumed test evidence, not a fresh holdout for future tuning.

| Candidate run | Responses | Mechanical acceptance |
| --- | ---: | ---: |
| Development | 8 | 7 |
| Held-out qualification | 8 | 6 |
| Frozen 40-report workload | 3 | 0 |

Mechanical acceptance is not a factual score. See [review.json](review.json) for
the assistant's review of every candidate title/body against its cited reports.
No human factual approval or independent verification of the underlying news is
recorded. These are single responses, not repeated reliability estimates.

The shorter n113 control passes 3/8 mechanical checks on those same held-out
cases, versus 6/8 for n119. It imports database answers into the profile, drops
publisher names from the conflicting reports, and also loses the shared outlet
in the repeated-source case. N119 improves those first two behaviors, but does
not fix the third. The control's numerical summary passes where n119's combined
sentence fails; there is no uniform improvement on every case.

## What improved and what still fails

The profile cases describe topics without importing database answers. The
quotation case no longer makes the subject the speaker of an unspecified false
statement. The correction case keeps City Wire as the outlet correcting its
report. Planned talks stay unheld, an unnamed player stays unnamed, and reversed
report/denial order does not create a completed move. Independent developments
are separated in the short test packet.

The 24-report Chelsea case still adds an adviser that the headline does not name
and changes French-born to French in a title. The held-out repeated-source case
loses the fact that Valley Press is repeating Hill Journal; it does not explicitly
claim independent confirmation, but the shared source chain is missing.

In the actual 40-report packets:

- **AC Milan:** the model reassigns the first publisher's score/red-card report
  to TNT Sports and adds an early lead and a euro currency absent from the cited
  text. It still combines unrelated topics. The shorter n113 control also fails,
  including replacing Maignan with Donnarumma; n119 avoids that particular error
  but does not make the packet faithful.
- **Chicago Bears:** a live-show topic is assigned to Yahoo Sports instead of
  Establish The Run. The cited reports do not supply the final entry's event date
  or venue. Several topics are combined.
- **LA Clippers:** the bodies largely follow their three Yahoo Sports headlines;
  no new event invention was identified in this review. The full name Kawhi
  Leonard occurs elsewhere in the packet but not in the cited injury headline.
  The parser first rejects `LA Clippers` where that entry's source says
  `Clippers`. Keep citation completeness, canonical-name validation and factual
  invention distinct rather than labeling every rejection a hallucination.

Separately, three faithful numerical/venue summaries are mechanically rejected
because the existing guard combines all names and numbers in a clause. The
instructions request separate statements, but the model still combines some.
The validation rules are unchanged; these failures remain visible.

## Hardware cost

Exact tokenization of the ten previously captured production-loader packets
shows that v5 adds **2,548 input tokens per request** over n113. All ten contain
40 selected reports. Including the unchanged 2,100-token output reservation,
requests total **7,592–7,804 tokens**. They fit the 7,900 code default, the configured
10,000 request limit and the server's 10,240-token physical slots. The largest
sample has only 96 tokens of margin under the code default; this is not proof
that every possible packet fits. Complete-input admission remains active.

`check_capacity.py` uses saved loader packets and native template/tokenization
endpoints. It does not access the database or run inference. Its results are in
`v5-capacity.json`; v6 has a separate, unused capacity result.

On the same three 40-report packets, the shorter control produced 1,610 output
tokens and n119 produced 755. Input tokens rose from 9,217 to 16,861. Recorded
sequential wall times were 80.2 and 74.5 seconds, respectively, but this is **not a
speed qualification**: there is one sample per case, order was not randomized,
and compilation may have overlapped the first candidate call. Both variants
failed mechanical acceptance on all three. See `workload-comparison.json`.

The longer instructions consume real prefill time. Shorter answers can offset
some cost on large packets; these observations do not establish full-day
completion, concurrency behavior, retry cost or the typical daily case mix.

### Prefix reuse

The native client previously sent `cache_prompt:false` on every request. The
working client now enables native prefix reuse while still sending the complete
token list. Exact template/input verification, full context admission, model
identity checks, total input-token accounting and completion checks remain active.
This is server-side reuse of matching computation, not stored answers or omitted
instructions. It applies to native generation calls, not to the decision endpoint.

`cache_probe.py` replays all eight frozen n119 qualification requests with only
that flag changed. All eight parsed outputs match their uncached counterparts,
including their known failures; all have complete input/output accounting. The
first request is cold; seven reuse a matching prefix. Total measured time falls
from 86.3 seconds in the original runner to 32.9 seconds at the native endpoint.
The shorter n113 control takes 31.2 seconds. The native probe excludes the small
local template/metadata HTTP overhead included in runner time. This supports
prefix reuse as a way to reduce the added instruction cost, not a nightly SLA.

See `cache-summary.json` and the full probe receipts. Prefix caching changes no
evidence boundary and does not fix factual errors. Sustained workload and mixed
character scheduling remain unqualified.

The separate four-slot comparison uses three 40-report packets and the profile
case. An uncached batch is followed by four different qualification inputs to
warm every slot, then a cached repeat of the measured batch. All twelve requests
retain complete input/output accounting; all four cached parsed outputs match
the uncached concurrent controls. Thus this tests shared-prefix reuse after
different inputs, rather than only replaying an identical cached request.
Recorded batch times are 74.5 seconds uncached and 46.2 seconds cached, with a
32.7-second warm-up batch excluded from that comparison. Earlier compilation may
have overlapped the uncached batch, so these times remain exploratory. See
`cache-concurrency-summary.json` and `v5-cache-concurrency/` for every receipt.
Two final replays through the rebuilt native client also pass, with caching
enabled in their recorded requests, complete input accounting, and outputs
identical to the corresponding uncached qualification replies. These are retained
in `v5-native-cache-smoke/`; they verify the implementation rather than adding
new qualification cases.

## Reproduce and validation

Use a new output directory:

```sh
cargo run --example journalist_assembly_trial -- \
  fixtures/journalist/detailed-instructions-2026-11-01/qualification-cases.jsonl \
  fixtures/journalist/detailed-instructions-2026-11-01/instructions-v5.txt \
  /tmp/journalist-detailed-retest \
  http://127.0.0.1:8081 Qwen3.5-4B-Q4_K_M baseline
```

The current runner uses prefix reuse. Original uncached receipts are frozen;
the cache probe scripts compare the flag explicitly using those complete requests.

389 library tests and 14 evaluation CLI tests passed; 74 environment-dependent
library tests were ignored. Formatting, diff checks, and source/fixture version
consistency passed. The new native-client test verifies complete request tokens
and rejects a response that accounts only for the uncached suffix. The instruction
file remains a working candidate; other character prompts and deployed services
were not changed in this pass.
