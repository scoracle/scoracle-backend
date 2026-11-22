# Real-data packet-size experiment — 2026-11-01

The unchanged n119 prompt and local Qwen3.5-4B do **not** show a reliably safe
packet size in this sample. Small packets still resolve ambiguity without
evidence. Larger packets introduce source-link errors and unsupported links
between reports. No prompt, production selection, model, parser or deployment
was changed for this experiment.

## Inputs and controls

The production loader was called through `journalist_capacity_audit` in a
read-only database session. It returned 40 selected headlines each for AC Milan,
Chicago Bears and LA Clippers. Its capture now retains `prompt::Parts`, including
actual database article IDs, stored source URLs and metadata. The earlier
workload fixtures had replay-local IDs; these inputs preserve real IDs.

These are real **stored publisher headlines**, not independently verified news or
full articles. Some delivered items are streaming advertisements or fixture
pages. They were deliberately retained, exposing the actual upstream selection
quality. No headline was rewritten or cleaned for the experiment. History stays
absent; the current DB identity record and production tone remain unchanged.

Each subject has one unchanged anchor moved to the first position:

| Subject | Database article ID | Publisher | Anchor |
| --- | ---: | --- | --- |
| AC Milan | 1276620 | LiveScore | Tresoldi: “60 million as early as January” |
| Chicago Bears | 1341401 | Da Windy City | A Donte Jackson trade *might* improve the defense |
| LA Clippers | 1289440 | RotoBaller | Max Strus ruled out for the road trip |

All 40 headlines were inspected before inference for another report qualifying
or denying each anchor. None was found; each named anchor occurs once. Other
headlines retain loader order. Thus 1, 4, 12 and 40 are exact nested prefixes,
including unchanged punctuation and complete headline qualifications. This does
not test multi-report denials; it avoids silently cutting one out of a baseline.

There are **12 unique subject/size cases, each run twice**, sequentially in
opposite size orders. The Bears start with descending sizes; the other subjects
start ascending. Model, system instructions, temperature 0, thinking off,
JSON/body-first assembly and native prefix caching are fixed. Production schema
and budget behavior are retained: up to one entry/900 reserved output tokens for
one report; up to three entries/2100 reserved tokens otherwise. Legal citation
keys expand with the packet. This measures production behavior across packet
sizes, not token length in isolation. See the predeclared [protocol](protocol.txt).

## Results

Manual fidelity review checks every entry against its **cited visible evidence**.
It accepts unambiguous Bears/Chicago Bears and Clippers/LA Clippers identity
expansions as the same entity. The unchanged mechanical guard rejects those
names. Unsupported opponent, event, cause and publisher/source connections still
fail manual review. Topic-order and style violations are recorded separately.
This is one Codex review, not blinded independent adjudication.

| Reports | Manually faithful whole outputs¹ | Mechanical acceptance² | Median call time³ |
| ---: | ---: | ---: | ---: |
| 1 | 4/6 | 2/6 | 2.8 s |
| 4 | 4/6 | 2/6 | 6.4 s |
| 12 | 0/6 | 0/6 | 8.1 s |
| 40 | 0/6 | 0/6 | 11.2 s |

¹ Each row is three subjects repeated, not six independent examples. Across all
sizes this is 4/12 unique whole outputs, reproduced twice. The unchanged anchor
itself is factually faithful in 8/12 unique cases: Bears and Clippers at every
size. Milan's ambiguity problem persists at every size.

² Parser plus the nonempty/anchor-name expectations. All four mechanically
accepted runs still call Milan's unspecified amount a “valuation.” All eight
manually faithful runs fail the literal team-name guard. Passing the current
validator is not a useful stand-in for honesty in this sample.

³ Includes model preflight and prefix reuse on the live Mac. Observed ranges are
1.8–3.3, 5.5–7.4, 6.6–8.5 and 4.5–13.3 seconds, respectively. Repeated packets can
reuse more prefix tokens. Total generation-call wall time was 160.67 seconds.
These are neither a cold-cache benchmark nor a daily capacity forecast.

All 24 responses used their full admitted input, ended at EOS and were not
truncated. Inputs were 3,099–5,704 tokens; actual outputs were only 46–188 tokens.
The largest input plus its reserved output was 7,804, within the 7,900 context
budget. Every pair had identical requests and identical parsed output, despite
opposite run orders. There is no evidence here of truncation or output-budget
exhaustion causing these errors.

The wrapper's final whole-model-response equality assertion initially failed
because llama-server's `created` field changed with response time. Both raw
identity responses are retained. The analysis compares all remaining model
metadata, which is unchanged; no inference was rerun or discarded.

## What the source review found

- **Milan, even at one report:** “60 million” becomes “a valuation of 60 million.”
  The headline does not identify a valuation, asking price, bid or agreed fee.
  This is a conservative ambiguity finding, not proof that the amount is false.
  No currency is invented in these new runs.
- **Milan at 12/40:** text from other reports is attributed to, or cited through,
  the wrong report object. At 40, the Christie reflection belongs to report_6,
  but the entry cites report_7 and report_8. The LiveScore Chukwueze lay-off claim
  belongs to report_34, but the injury entry cites only Football Italia reports
  22 and 23. All those headlines are available; source binding is lost.
- **Bears:** Donte Jackson remains a conditional trade opinion at every size.
  The 1/4-report summaries are factually faithful. At 12/40, the model inserts
  the Patriots opponent into a Ditka-honor entry whose cited headline does not
  supply it. This is a citation gap from other supplied material, not invention
  of a nonexistent opponent. Larger packets also skip/reorder early topics.
- **Clippers at 12:** two NBC headlines about different named players facing the
  Clippers become performances in “the same matchup.” Neither headline supplies
  that relationship. At 40, “several notable players out” becomes “ruled out due
  to injuries”; the cited headline does not give a cause. A separate five-injured
  headline does not explicitly establish that these are the same players.
  Max Strus remains faithfully summarized, but moves to the second entry at 40.
- **Validation:** canonical team names cause false rejections. Conversely,
  lexical validation accepts the unsupported “valuation” interpretation. These
  are distinct shortcomings; weakening validation would not repair the writer.

The larger packets contain more topics and permit different selections. These
observations do not prove packet size alone causes the errors, or that all
40-report packets fail. They do show why truncating everything to four reports
would be an unjustified fix. No player-count, stadium, coach or date from meta
was promoted into a news claim in this sample; identity expansions are reported
separately rather than counted as fabricated news.

## Next controlled comparison

Test the same evidence with **explicit topic grouping and preserved report-to-
publisher links before prose generation**, retaining every relevant denial or
qualification. Compare against the frozen mixed-packet baseline here. Require
that the complete path preserve source ownership, uncertainty and total cost;
do not assume grouping solves the one-headline “valuation” error. The wording
contract also needs to preserve unspecified numbers and avoid treating
co-occurrence as evidence of a shared event or cause.

Use the current 4B and measure preparation plus generation together. Do not
silently introduce one model call per headline or discard reports to obtain
better scores. Test the identity-name guard independently with real name
collisions before changing it. No such production changes were made here.

## Reproduce and inspect

- [Manifest](manifest.json): exact anchor IDs, all source URLs, original positions
  and packet-local report keys. URLs are the stored Google News RSS URLs.
- [Live capture](live-capture/results.jsonl): exact production-loader material,
  metadata, dispositions and admission receipt.
- `*-cases.jsonl`: complete frozen nested inputs and review expectations.
- `*-runs/`: raw assembled input, native request/response, output and mechanical
  verdict for every generation; no rejected output was repaired or omitted.
- [Review](review.json): all 12 unique cases, evidence and classifications.
- [Measurements](measurements.json): complete receipt checks, timing, token counts
  and repeat comparisons.

From the Rust workspace, `python3 fixtures/journalist/packet-size-2026-11-01/build_cases.py`
rebuilds the cases and manifest from the immutable capture, with prefix/identity
assertions. `python3 fixtures/journalist/packet-size-2026-11-01/analyze.py` verifies
all 24 saved receipts and reproduces measurements without network access.

For an additional model run, use the existing `journalist_assembly_trial` with a
fresh output directory (it refuses to overwrite one):

```sh
target/debug/examples/journalist_assembly_trial \
  fixtures/journalist/packet-size-2026-11-01/ac-milan-cases.jsonl \
  fixtures/journalist/packet-size-2026-11-01/instructions.txt \
  /tmp/new-milan-packet-replay \
  http://127.0.0.1:8081 Qwen3.5-4B-Q4_K_M baseline
```

The existing pilot remains unqualified for rollout. Other character plugins,
full articles, independent real-world verification and a full daily workload
were outside this experiment.

Validation completed: both replay examples build; the assembly example's focused
schema/source-preservation test passes; the capture example has no unit tests.
Frozen inputs and measurements reproduce byte-for-byte through their assert-based
scripts. `cargo fmt --all -- --check` and `git diff --check` pass. The full library
suite was not rerun for this capture-only change.

The proposed grouping comparison has now been completed: see the
[topic-grouping follow-up](../topic-grouping-2026-11-01/README.md) for 28 real-data
calls, automated grouping failures, a bounded three-group diagnostic and the
full processing-cost breakdown. The pilot remains unqualified.
