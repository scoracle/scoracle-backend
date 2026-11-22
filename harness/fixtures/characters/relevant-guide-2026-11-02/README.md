# Relevant field guides: leaner inputs, mixed output fidelity

Continuation from committed baseline `6a87e76db7ef4b8c64bcd6ac4514efd42278bca0`. The selected implementation reduces complete native input by **4,980 tokens (14.4%)** on the same 12 development cases: **34,685 → 29,705**. This establishes input savings, not a general quality improvement. Some outputs remain materially wrong, including regressions from the baseline.

Read the [complete current system/user requests](prompt-preview.md), [all 44 outputs and manual source review](outputs.md), [machine-readable review](review.json), or [current-input receipt matching](selection-check.json). Earlier fixtures and their receipts in `context-guide-2026-11-02` remain unchanged.

## What became leaner

The user request still has `job → context_guide → meta → fresh → tone`. The opening task is derived from the authoritative character instructions and appears once in the model input, in `job`. The system message retains the remaining instructions and output contract. The repeated reading-order paragraph was removed from all six characters.

The shared assembler filters local `- dotted.path: meaning` lines against the actual serialized payload. Paths traverse array items; one definition covers all supplied occurrences. Ordinary prose contains the short shared interpretation rules. This is a small path-presence filter, with no interpolation, expressions, dependency or templating engine.

- Zero and false count as supplied values, including zero percentiles and false comparison eligibility.
- Missing/null, blank strings and containers with no meaningful values do not trigger field definitions. The general guide still explains unknown information and empty collections.
- Metadata definitions distinguish canonical target identity, separate coaches/affiliations, recorded observations and fresh reporting. Unknown attributes remain in the payload without invented definitions.
- Each character keeps its own evidence meanings: publisher versus speaker, publication versus event date, target sample versus peer cohort, measurement polarity, prepared interpretations and separate comparison windows.
- The existing five-key envelope is retained. Its small overhead did not justify breaking parsers or replay tooling. IDs, links, wording, qualifications, dates, source order and measurements were not shortened.

Across these cases the serialized user requests shrink from 73,013 to 54,869 UTF-8 bytes (24.9%). The headline token result includes the full system message, user message and native chat-template tokens; it is not a user-prompt estimate.

Current versions: Journalist `n126-relevant-guide`, Influencer `vibe-frame-v39-relevant-guide`, Scout `s86-relevant-guide`, Insider `insider-source-v27-relevant-guide`, Analyst `momentum-s49-relevant-guide`, Oracle `or37-relevant-guide`.

## Controlled comparisons

Existing local `Qwen3.5-4B-Q4_K_M`, `http://127.0.0.1:8081`; temperature 0, thinking disabled, concurrency 1, no retries. Production preparation, schemas, output reservations and parsers are unchanged. All trial receipts use context 10,240. No generation was truncated or held for context. Eleven frozen real-data cases and one explicitly synthetic dated-comparison case were reused; no new news or database retrieval occurred.

A baseline binary was built from the clean starting commit before edits and copied to `/private/tmp/scoracle-6a87e76d-character_outputs`; its SHA-256 is in the run receipts. Two batches each paired one baseline and one initial lean generation on six cases, alternating arm order. A 12-case refinement followed, then an eight-case follow-up on the four weakest characters. Every response, failure, native tokenization receipt and parser result is retained: **44 unique generations, 106,783 input tokens and 4,310 output tokens**.

| Tested arm | Generations | Mechanically accepted | Complete input tokens |
| --- | ---: | ---: | ---: |
| Committed baseline | 12 | 10 | 34,685 |
| Initial lean (`candidate`) | 12 | 11 | 29,799 |
| First refinement (`final` directory) | 12 | 10 | 29,535 |
| Second refinement (`followup`; four characters) | 8 | 8 | 12,764 |
| **Selected current variants** | **12 reused receipts** | **10** | **29,705** |

The selected row is not another 12 calls. Journalist and Insider use the first refinement; the other four use the shorter initial lean version. Selection is per character across both cases, never per individual output. Longer reminders failed to establish a useful improvement and sometimes introduced worse errors. The historical directory name `final` denotes the first refinement, not the final source selection. Exact current system/user bytes match the selected receipts even though prompt versions advanced during experimentation; see `selected-variants.json` and `selection-check.json`.

| Character, two cases each | Baseline tokens | Current tokens | Saved |
| --- | ---: | ---: | ---: |
| Journalist | 9,940 | 9,203 | 737 (7.4%) |
| Influencer | 5,723 | 5,059 | 664 (11.6%) |
| Scout | 3,926 | 3,035 | 891 (22.7%) |
| Insider | 8,845 | 7,879 | 966 (10.9%) |
| Analyst | 3,094 | 2,227 | 867 (28.0%) |
| Oracle | 3,157 | 2,302 | 855 (27.1%) |

`analyze.py` asserts identical evidence bytes, structured parts, provider fingerprints and generation settings after excluding prompt tokens. It also checks that every reported input-token count equals the length of the complete native tokenizer receipt. Shared workstation timings are retained but are not throughput estimates.

## Output-quality findings

- **Journalist:** the initial lean guide merged Demichelis's two discussions; the selected refinement restores two entries. Beal's injury and jersey-number stories still merge, as in the baseline. The unchanged numerical/source parser rejects one wording even though the source mentions Chris Paul's old No. 3; that rejection does not establish a new factual error.
- **Influencer:** De Zerbi's expressed reactions remain recognizable. The Jones case regresses: criticism directed at him becomes his own criticism. Both baseline and current versions wrongly frame the two negative reports as conflicting/mixed. Adding more relationship reminders did not fix it.
- **Scout:** the player case preserves amounts, separate bands/ranks, peer count and the five-appearance limit. The team case regresses from a correct baseline to “46.4 points” for rebounds. Later attempts label every measure as points; those versions are not retained. Correct ranks and parser acceptance do not excuse wrong units.
- **Insider:** the selected version removes the unsupported draft-bust characterization from the Neal signing summary. The Roma output no longer turns absences into departures or substitutes Roma for Leipzig as the retaining club, but it omits Leipzig, includes unrelated injury coverage and extends injury to Svilar without explicit support. The Neal source/alias rejection remains; parsers were not loosened.
- **Analyst:** the current LaPorta reading avoids the baseline's unsupported desire to leave and preserves the thin sample. The synthetic output no longer explicitly claims a common window, but omits the distinct dates and uses rating notability rather than its plain meaning. A later refinement invents two-week windows; it is not retained. The last follow-up recognizes separate windows but still omits dates.
- **Oracle:** the turnover reversal persists. Longer reminders additionally reversed elite three-point shooting; they are not retained. The player reading keeps the thin sample but still adds unsupported framing about the team's readiness to resolve the situation.

These are purposive development cases used repeatedly for prompt selection, not held-out accuracy results. The optimization is retained locally for its input savings; faithful generation is still unfinished. There is no claim that equal mechanical acceptance means equal factual quality.

## Verification and limits

All 52 current character quality fixtures were regenerated through `character_package` and the production assembler. Stored parts and the serialized metadata/evidence/tone suffix remain byte-identical. Focused tests cover zero, false, null, missing/blank values, empty collections, heterogeneous arrays, and target/relationship distinctions, including a competition target rather than only teams or players.

Required command: `cargo test --offline --lib --examples --bin eval`. **418 passed, 74 ignored** (0 failed). Final results and the initial sandbox-only local-socket failures are retained in `verification/`. Formatting and diff-whitespace checks pass. No parser, source selector, generation pass or output schema was changed.

The unchanged 24-report Chelsea preview falls from **10,323 to 9,929 native input tokens**, but still fails admission with 2,100 output tokens reserved under either 7,900 or 10,240 total tokens. This is a zero-generation measurement, not an admitted request. See [budget check](budget-check.json). The work does not solve oversized source packets by clipping evidence.

No historical context, larger model, download, deployment, publication or production write was added. The unrelated production-recovery planning document was left untouched.

## Reproduction

Build `character_package` and `character_outputs` with `cargo build --offline --example character_package --example character_outputs`. `build_cases.py` reassembles development and current quality fixtures; `verify_current.py` verifies the selected saved inputs and writes the complete prompt preview. `analyze.py` regenerates the token totals and reviewed-output report.

Trial runners require new output directories and retain immutable captures. Replaying the original baseline requires a binary built at `6a87e76d`; current code must not be mislabeled as that baseline. `run_comparison.py` runs the two original paired batches, and `run_final.py` / `run_final.py followup` describe the refinement workloads. Their historical instruction/guide snapshots and exact input receipts are retained; do not overwrite trial directories when running a new experiment.
