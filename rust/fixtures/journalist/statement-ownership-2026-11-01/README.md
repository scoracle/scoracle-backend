# Statement-level source ownership — 2026-11-01

Follow-up: [worked examples and explicit evidence gaps](../statement-calibration-2026-11-01/README.md)
tests these remaining issues in 30 calls, including three new subjects, without
changing the production prompt or increasing model size.

**A source-bound response format fixed specific attribution errors without an
extra model call, but did not make the writer reliable.** More detailed prose
instructions alone did not resolve the tested failures. The structured format
improved topic coverage, preserved some previously lost distinctions and avoided
claiming fixture pages represented completed games. It still invented meanings
for ambiguous details, produced misleading labels and exposed a grammatical
problem in joining attribution forms to generated clauses.

This is a local experiment on the same Qwen3.5-4B-Q4_K_M and real stored headline
corpus as the [grouping experiment](../topic-grouping-2026-11-01/README.md).
Production prompt, parser, model configuration, database and publication path
were unchanged. Manual topic preparation is still an unresolved production
requirement, not free automated work.

## Comparison and controls

Six packets, three arms, two sequential passes: **36 model calls**. Three known
packets exactly reproduce the prior bounded inputs: Milan match/Tresoldi/
Pavlovic, Bears trade/live show/Ditka, and Clippers availability/fixture pages.
Three reserved combinations use additional headlines from the same previously
inspected 120-report corpus:

- Milan: boycott support and retaliation, possible recall with qualifications,
  and Rabiot contract interest.
- Bears: Gonzalez available despite illness, an unspecified Vrabel streak,
  and a possible stadium move.
- Clippers: separate Acuff and Murray performances, and parking/jersey coverage.

“Reserved” means these combinations were not generated or tuned against before
this experiment. It does not mean independent unseen subjects or independently
verified news. Every source object, original database ID, publisher, headline,
meta value and tone value is retained. The original report keys are preserved.
There is no history injection, grouping inference, source rewriting or output
repair. Inputs and prompts were frozen before the first call.

Arms:

1. **Baseline:** the existing grouped writer and ordinary paragraph schema.
2. **Detailed prose:** adds explicit single-source statement ownership, no
   blanket publisher attribution, unspecified-number handling, and the rule that
   a fixture/page title does not establish a completed event.
3. **Structured statements:** equivalent evidence rules, with statements before
   the title. Each statement generates `report_key`, `attribution` and `clause`,
   in that order. Code retrieves that report's publisher and renders
   `publisher + attribution + clause`, then joins statements into a paragraph.
   It derives the legacy citation list from those statement source keys.

The renderer rejects missing assigned topics/anchors, wrong-topic source keys,
invalid attribution forms and empty clauses. It never changes a generated clause
or repairs its meaning. A valid source mapping is **not** semantic entailment.
The model still chooses the source ID, claim, attribution form and title. Natural
readability of the rendered result is evaluated separately.

The structured comparison changes instructions, schema and emission order
jointly; it does not isolate which component caused an improvement. Worked
examples preserve their source evidence while expressing the new contract.
Both passes run all six packets; arm order reverses in pass two. Temperature 0,
thinking off, native caching on, context **7,900** and output reservation **2,100**
are constant. The previous 10,000 setting was an admission budget; these bounded
inputs fit the production default. All three known baselines exactly reproduce
the previous parsed outputs.

[Protocol](protocol.txt) · [Manifest and groups](manifest.json) ·
[Ordinary cases](cases-prose.jsonl) · [Structured cases](cases-structured.jsonl)

## Source review

| Issue | Baseline / detailed prose | Structured statements |
| --- | --- | --- |
| Milan match account | One combined account attributed to several publishers with different evidence | Match claim remains with its actual report/publisher |
| ESPN/FOX boycott coverage | FOX's retaliation warning is also assigned to ESPN | ESPN owns support; FOX owns the warning |
| Clippers fixture page | “Played” or “past game” added to a summary-page title | Page is listed without asserting completion |
| Parking and jerseys | Combined wording attributed jointly to Yahoo and KTLA | Yahoo alone owns the details it supplies |
| Unspecified 60 million | Interpreted as a valuation | Still interpreted as a valuation |
| Unspecified 27-year streak | Invented Thanksgiving-game explanation | Different invented explanation: consecutive NFC North games |
| Topic coverage | Bears' additional topics collapse into one roundup | All three topics retained in every packet |

The structured Bears streak puts `NFC North` into a claim even though it appears
only in team meta, not in that packet's fresh headlines. The head-coach role is
also absent from the selected source. Correct source IDs do not prevent a clause
from importing background or invented detail.

The structured Milan recall body keeps a possible recall and the early-exit-
unlikely qualification, but omits the explicit two-factors-needed condition.
Its title groups Chukwueze and Athekame under “injury concerns,” although the
source describes Chukwueze's lay-off and Athekame's possible recall. This is a
misleading title implication; the body does not literally say Athekame is injured.

One rendered line is grammatically defective: “LiveScore discusses Pavlovic
insists ...”. The model chose a clause that does not fit `discusses`; deterministic
joining cannot resolve that without changing generated content. Some other lines,
such as “RotoBaller lists Max Strus ruled out ...”, are stiff but understandable.
The design is therefore not yet satisfactory as natural prose either.

Conservative full-output review, counting unique packets rather than repeats:

| Arm | Clearly faithful | Additional source-format concern |
| --- | ---: | ---: |
| Baseline | 1/6 | 0 |
| Detailed prose | 1/6 | 0 |
| Structured | 2/6 | 1/6 |

The additional structured case preserves trade uncertainty and the Ditka plan,
but changes the Establish The Run “Live Show” into a “broadcast.” That could mean
the show or suggest game coverage; it is explicitly left as a clarity concern,
not counted as either a proven fabrication or an unequivocal pass. This review
is by Codex, not blinded or independently adjudicated. These selected stress
cases do not establish a general accuracy rate.

All **12 structured renders** preserved valid source mappings and all assigned
topic anchors. Baseline and prose had full anchor/group coverage in 10/12 runs.
Every arm had 4/12 mechanical acceptances, illustrating why this is a separate
metric. The parser accepts the fabricated Thanksgiving explanation and the
structured Milan title/qualification problems. It also rejects some faithful
canonical team names and the legitimate Korean-named publisher `조선일보` under
its non-Latin prose rule. No guard was relaxed to improve the results.

[Complete per-case review](review.json)

## Cost and integrity

| Arm | Model calls per packet | Median wall time | Range | Input tokens | Output tokens |
| --- | ---: | ---: | ---: | ---: | ---: |
| Baseline | 1 | 6.24 s | 4.95–18.28 s | 3,451–3,860 | 109–212 |
| Detailed prose | 1 | 7.47 s | 4.98–19.51 s | 3,726–4,135 | 109–223 |
| Structured | 1 | 6.33 s | 5.83–17.23 s | 3,912–4,321 | 133–236 |

The structured format added **no inference stage**. It used 461 additional input
tokens per packet, about 13% more input tokens across this sample, and about 3%
more total output tokens. Measured median latency was close to baseline, with
substantial variation from cache reuse. These are observed timings, not a claim
that overhead is always 0.09 seconds. Native cached-token counts ranged from one
to roughly 3,000–3,500 across arms; receipts retain prefill/decode timings.

Wall time includes generation, deterministic rendering, evaluation and response
capture. It excludes manual preparation/review. All 36 calls totaled **326.1
seconds** of measured call wall time. No compiler or other experiment ran during
the benchmark. The same existing model and physical slots served every arm.

All 36 calls consumed their complete admitted input, ended at EOS and were not
truncated. Maximum input plus reserved output was **6,421 tokens**, below the
7,900 budget. All 18 repeat pairs used identical requests and produced identical
parsed output. Model identity was unchanged before/after (excluding only the
server's response-time `created` field).

This keeps the experiment within the existing hardware envelope and avoids the
extra grouping call's cost. It does **not** establish daily throughput: packets
were manually prepared, the sample is small, and no mixed-character daily load,
retry policy or parallel saturation was measured. The writer remains unqualified
for rollout on factual and readability grounds regardless of latency.

[Measurements and receipts summary](measurements.json)

## Artifacts and verification

- `baseline.txt`, `prose.txt`, `structured.txt`: immutable prompt variants;
  production `src/plugins/journalist/instructions.txt` was not changed.
- `*-pass-*`: assembled requests, raw responses, native receipts, parser verdicts
  and, for structured outputs, `rendered.json`. No failure was removed or repaired.
- Original DB IDs/URLs are in each case's parts and the linked
  [source manifest](../packet-size-2026-11-01/manifest.json).
- `build_inputs.py`: rebuilds fixtures and verifies source preservation and exact
  known-input equality. `run.py`: executes the frozen, sequential matrix with
  model-identity checks. `analyze.py`: reproduces metrics and independently checks
  rendering, token accounting, source boundaries, schema identity and repeats.

The existing assembly trial gained optional frozen-input/schema overrides and
a statement-rendering mode. The renderer is experiment-only. Focused tests
cover exact publisher/claim joining, wrong-topic IDs, missing anchors/topics,
invalid attribution forms and schema emission order, alongside the existing
assembly-preservation test. Both tests and the example build passed. Formatting,
diff and deterministic artifact checks passed. No production deployment or
full-library retest was needed for these example-only changes.

The evidence supports further work on the structured source contract while
keeping semantic fidelity and natural prose as separate requirements. It does
not support treating a valid source mapping—or a larger instruction block—as a
truth guarantee. Ambiguous details, source-format distinctions and titles remain
unresolved.
