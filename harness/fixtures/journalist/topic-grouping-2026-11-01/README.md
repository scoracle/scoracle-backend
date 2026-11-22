# Explicit topic grouping and source preservation — 2026-11-01

**Grouping labels alone did not make the 4B reliable.** Restricting the writer to
three reviewed topic groups improved topic selection and some source ownership,
but only the Bears output was fully faithful in this three-subject sample.
Automated grouping was semantically wrong for all three subjects and added cost.
This experiment does not justify adding that stage to production.

There were **28 actual model calls**: 22 for the main comparison and six for a
bounded follow-up. Every repeated request produced the same parsed output. All
responses completed with full admitted input and EOS, without truncation. The
model, runtime instructions in `src/plugins/journalist/instructions.txt`, parser,
database and production pipeline were not changed by this experiment.

## Main comparison

Use the exact three 40-headline packets and source IDs from the earlier
[packet-size experiment](../packet-size-2026-11-01/README.md). The anchor order,
meta and tone are unchanged. Reference groups were reviewed and frozen before
inference. Each writer receives all 40 reports:

1. **Baseline:** existing flat n119 input and instructions.
2. **Reviewed groups:** reports nested under explicit topic groups. A changed
   selection contract asks for the first three groups; worked examples use the
   same format without changing any example's evidence.
3. **Automatic groups:** one additional call to the same Qwen3.5-4B returns only
   arrays of report IDs. Code verifies coverage and IDs, attaches the original
   report objects, orders the groups by anchor, and invokes the grouped writer.

Grouping can reference a report in more than one topic. It cannot invent IDs,
repeat an ID within a group, repeat an anchor, or omit a report. No generated
claim, label or rewritten headline is passed to the writer. Stored article IDs
and URLs remain in the original parts/provenance; packet-local `report_N` keys
are never renumbered. This verifies identity preservation, **not** correctness of
the grouping or of the generated claims.

Each subject runs twice. The arm order reverses in pass two. Temperature is zero,
thinking is off, prefix caching is on, and the existing body-first output schema
is retained. All arms use context 10,000 and reserve 2,100 output tokens; the
increase from the earlier 7,900 window accommodates grouping markers without
clipping evidence, within the existing 10,240-token physical slots. The grouper
reserves 1,200 output tokens. This tests contract and layout together, not an
isolated formatting variable. The grouped serializer also places `fresh` before
`meta`, whereas the baseline serializer preserves `meta` first.

[Predeclared main protocol](protocol.txt) · [Inputs and reference maps](cases.jsonl)
· [Manifest and boundary notes](manifest.json)

## What failed

| Subject | Reviewed groups, all 40 visible | Automatic grouping |
| --- | --- | --- |
| Milan | Adds euros and a valuation; substitutes DB coach Filippo Galli for Pavlovic; misattributes parts of the match account | Omits reports 29/36, so writing is blocked; also merges transfers, results, interviews and unrelated injuries |
| Bears | Ignores the live-show group and borrows injury coverage from a later group; adds an uncited opponent to the Ditka story | Combines Donte Jackson trade opinion with a Super Bowl-preview headline, then merges most remaining topics |
| Clippers | Skips the two early fixture groups and turns unspecified absences into injuries | Places all 40 reports in one group, mixing players, dates, statistics and other topics |

The Bears/Clippers automatic maps passed structural coverage checks, but were
semantically incorrect. Their writers also violated assigned-group boundaries
or entry counts. The Clippers automatic prose largely preserves its chosen
injury/absence headlines; its failure should not be described as every sentence
being fabricated. Expanded `Kawhi Leonard` is a citation/name-completeness issue,
not invention of a new person.

Milan's grouping failure happened in both passes; no writer was called for those
two attempts. Thus 24 planned calls became 22 actual calls, with all failed work
retained and charged. No missing reports were silently restored as guessed
singletons and no outputs were repaired.

## Bounded follow-up

After the first all-40 pass failed, a separate follow-up served only the **first
three complete reviewed groups** in one writer call. It retained the grouped
instructions, original report keys and every member of those selected groups.
Legal output citations were restricted to the visible sources. Preparation still
verifies the full 40-report group map before selection. This changes evidence
scope and citation vocabulary; it is not a formatting-only test or a held-out
qualification. Manual grouping time is not a zero-cost production method.

The writer received 9 Milan, 5 Bears and 3 Clippers reports. Across both passes,
all three assigned topics were covered and all citations stayed in their groups.

- **Milan:** Pavlovic remains the speaker and euros disappears. “Valuation” still
  interprets the unspecified amount. The match summary attributes a combined
  result/red-card/Juventus-draw account to a list of publishers, although Yahoo's
  headline only supplies an early lead and TNT's concerns a substitution.
  Correct report IDs do not make blanket attribution truthful.
- **Bears:** all three entries are factually faithful: conditional Donte Jackson
  trade opinion, Establish The Run live show, and Yahoo's Ditka announcement.
  The unchanged parser rejects the canonical name `Chicago Bears` where a
  headline says `Bears`; that is not invented news in this controlled packet.
- **Clippers:** the fixture topics are now selected, but “Game Summary” becomes
  a game “played” on November 2. A page title does not establish that the event
  happened. The capture was November 1, but the error is unsupported completion
  even without using the clock. The Max Strus absence remains faithful.

Only **1/3 unique bounded outputs** was fully faithful, repeated twice. Human
review was by Codex, not blinded or independently adjudicated. Do not treat these
known stress cases or their repeats as a general accuracy rate. All mechanical
writer verdicts failed; some stop at canonical-name checks or an erroneous
numeric association with `44` in publisher `FOX 44`. The detailed review separates
those guard issues from genuine source-ownership and event-completion errors.

[Follow-up protocol](selected-protocol.txt) · [Source-by-source review](review.json)

## Processing cost on the existing hardware

Model: `Qwen3.5-4B-Q4_K_M`, approximately 2.73 GB model file; Mac16,10 with 16 GiB
RAM and 10 physical CPU cores. Existing llama-server has four 10,240-token slots.
No model download, extra model process, slot change or concurrent benchmark was
introduced. Measured times include native token-admission preflight; automatic
path times include grouping, verification/assembly and writing.

| Path | Calls per packet | Measured median | Observed range | Interpretation |
| --- | ---: | ---: | ---: | --- |
| Flat baseline | 1 | 13.0 s | 4.5–15.4 s | Current mixed packet |
| Reviewed groups, all 40 | 1 writer | 23.3 s | 22.4–26.3 s | Excludes human grouping; no fidelity improvement |
| Automatic grouping + writer | 2 | 35.1 s | 33.5–37.5 s | Four completed paths, all unqualified; two additional paths failed grouping at 12.0/12.5 s |
| Three reviewed groups only | 1 writer | 7.8 s | 6.0–17.5 s | Excludes human grouping; one of three subjects faithful |

The grouping call alone took a median **12.4 s**. Failed attempts are retained in
`measurements.json`; they are excluded only from the explicitly labeled median
of completed two-call paths. Across all 28 calls, measured path time totaled
442.0 seconds. Review, setup and compilation time are not inference throughput.

Cache behavior differs materially: baseline calls reused 2,774–5,578 tokens;
the all-40 grouped writers and groupers reused only one token in these runs.
The bounded follow-up reused 3,041 tokens after its first two calls. Therefore
these are observed execution costs, **not an intrinsic 2.7x algorithmic penalty**
or an optimized throughput benchmark. Even with potential caching improvements,
the extra model call, semantic grouping errors and additional processing are
real and must be evaluated together.

Input/output token ranges:

| Call | Input tokens | Output tokens |
| --- | ---: | ---: |
| Baseline writer | 5,575–5,704 | 126–188 |
| Reviewed all-40 writer | 6,326–6,457 | 120–204 |
| Grouper | 2,968–3,097 | 108–120 |
| Automatic grouped writer | 5,767–5,861 | 122–219 |
| Three-group writer | 3,451–3,860 | 134–208 |

The maximum input plus reserved output was 8,557; every call fit without
truncation. That exceeds the production default 7,900, so the all-40 grouped
format also cannot be dropped into the default budget unchanged. The bounded
writer fits that budget. All complete receipts are retained.

The refreshed read-only [workload audit](workload-audit.jsonl) still shows the
unfinished October 23 edition, 1,589 pending Journalist assignments with median
four source memberships, and 1,597 pending/failed Journalist queue jobs. Those
are different accounting views, not additive totals or a normal daily call
forecast. Assignment membership counts also differ from the loader's 40-report
delivery cap. These 40-report tests deliberately exercise heavier packets.

For scale only, 1,000 grouping calls at the observed 40-report median would add
about **3.4 serial processing hours** before writing. That is a normalization,
not a prediction for the actual source-count distribution. Four slots do not
establish a fourfold speedup. Other characters, retries, contention and the
actual daily mix remain unmeasured here. A daily-scale load test is premature
while this path fails correctness; no full-day capacity claim is made.

## Consequence for the next experiment

Do not introduce the extra grouping call or change production limits on this
evidence. Bounded topic evidence is a useful direction for selection, but it
needs a reliable, measured preparation method.

The remaining failures suggest a more focused next comparison: enforce
**individual statement-to-source ownership**, especially in multi-publisher
summaries, and distinguish a fixture/page title from a completed event. Test
that in a batched writer without assuming an additional model stage is necessary.
Preserving IDs is already verified; preserving the meaning attached to each ID
is the unsolved requirement. This is a proposed experiment, not a claimed fix.

## Artifacts and verification

- `runs/`: all original assembled requests, exact native requests/responses,
  outputs and errors for the main matrix, plus original parts with DB IDs/URLs.
- `selected-runs/`: equivalent evidence for the bounded follow-up.
- [Main measurements](measurements.json) and [bounded measurements](selected-measurements.json).
- [Hardware](hardware.json), `server-before.json` and before/after/final model
  identity responses. Model comparison excludes only the response-time `created`
  field; all actual model metadata remains unchanged.
- `baseline.txt`, `grouped.txt`, `grouper.txt`: frozen prompt variants. No runtime
  prompt version was bumped because production instructions were unchanged.

`build_inputs.py` rebuilds the exact fixtures from the previous capture, with
coverage and example-preservation assertions. `analyze.py` and
`analyze_selected.py` check source objects, metadata, tone, coverage, citation
vocabulary, exact token counts, EOS and timing using saved receipts only.

The experiment runner is `examples/journalist_topic_trial.rs`. It takes
`CASES.jsonl PROMPT_DIR NEW_OUTPUT_DIR MODEL_URL MODEL`; append `selected` for the
bounded diagnostic. Output directories must be new. Its focused test covers
missing/invented/duplicated IDs, cross-topic reuse, exact source-object identity
and whole-group selection. Both example build and test passed; formatting and
diff checks passed. The full library suite was not rerun because production code
was not changed. No database write, publication or deployment occurred.

The proposed statement-level comparison is now complete. See the
[statement-ownership experiment](../statement-ownership-2026-11-01/README.md) for
36 additional calls, deterministic source rendering, semantic/readability
failures and measured single-call cost. Production remains unchanged.
