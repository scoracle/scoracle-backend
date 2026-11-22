# Insider explicit-index trial — November 1, 2026

**Explicit indices fixed source matching in this sample, but did not improve complete cards.** Wrong-source references fell from three to zero. All fourteen indexed findings quoted their cited report correctly. However, the indexed inputs introduced concrete meaning regressions, and mechanical acceptance fell from one of twelve cards to zero. The candidate is retained as `candidate.patch`; runtime and quality fixtures were restored to the pretrial `insider-source-v21-perspective` format. Earlier six-character implementation changes are preserved.

This was one bounded follow-up to the [six-character evaluation](../useful-perspectives-2026-11-01/README.md): twelve new packages, two versions, one generation per package/version. No additional prompt-tuning rounds, model judges, retries, deployment or production writes followed.

## Controlled change

The candidate adds a code-assigned, zero-based `report_index` to each report in the existing Insider input. It preserves all report fields, order, metadata, instructions, voice, schema, parser rules and output limits. It versions the input as `insider-source-v22-report-indices`. A unit test verifies indices follow the served order without changing source fields.

The paired inputs are byte-identical after removal of the new fields. `build_cases.py` asserts that property for all twelve pairs. `analyze.py` also verifies the actual request settings are equal after removing the tokenized prompt. Neither arm changes the publication contract or canonical identities.

The same existing endpoint, `http://127.0.0.1:8081`, served both arms: `Qwen3.5-4B-Q4_K_M`, Q4_K Medium, llama.cpp build `b11429-d81235049`. Model identity was checked before and after. The trial used temperature zero for a controlled input comparison, one request at a time, with six pairs control-first and six indexed-first. It retained the 7,900-token request envelope and full-input/completion checks. The server has four slots, but this was a sequential comparison, not a workload-capacity benchmark.

## Cases and capture

Four packages per sport were selected from source headlines before generation. They contain 61 reports after exact same-publisher/text deduplication and original D3 identity holds:

| Sport | Subjects | Reports |
| --- | --- | ---: |
| Football | Eddie Howe; Jean-Clair Todibo; Alex Scott; Roma | 3; 2; 5; 9 |
| NBA | Draymond Green; Donovan Mitchell; Jalen Johnson; Dallas Mavericks | 2; 2; 2; 7 |
| NFL | Dan Quinn; Justin Herbert; Evan Neal; Brian Thomas Jr. | 5; 5; 7; 12 |

The set includes an explicit trade-talk shutdown, a no-loan-termination report, reported signings, speculative proposals, conditional retirement intentions, coaching commentary and an unrelated on-court altercation. Empty findings can be correct; they are not scored as successful reference opportunities.

“New” means newly captured subjects/packages relative to the preceding eleven-subject workload, not newly published news or a random sample of routine production. The database's latest frozen edition is still October 23. All evidence retains original wording, IDs, links, dates, candidate lists and scope. Identity metadata comes from read-only production-loader captures. No current reporting is manufactured from old source timestamps.

West Ham United was initially selected, but every source lacked the original D3 target identity. Before any generation, it was replaced by Todibo's own frozen assignment containing the no-termination reports. The excluded capture and rationale remain in `selection.json` and the initial capture files. One Mavericks source was also identity-held; exact copies were recorded separately. No case was replaced after seeing outputs, and neither arm clipped an admitted package.

SQL ran with `default_transaction_read_only=on` and timeouts. The model runner itself has no database access and cannot publish. Capture errors/no-material outcomes are retained; Todibo's identity came from its Scout capture because its reporting deliveries were already settled.

## Results

Reference checks inspect every finding, even when the publication parser stops at an earlier error. An exact quote in another report, but not the cited report, counts as a wrong index. Exact membership does not establish the meaning of a move, the right subject, or reported status.

| Measure | Existing input | Explicit indices |
| --- | ---: | ---: |
| Generation calls | 12 | 12 |
| Findings emitted | 14 | 14 |
| Exact quote in cited report | 11/14 | 14/14 |
| Wrong-index findings / affected cards | 3 / 3 | 0 / 0 |
| Quotes absent from every supplied report | 0 | 0 |
| Counterparty present in cited report's candidate list | 11/14 | 13/14 |
| Cards with empty findings | 5 | 5 |
| Mechanically accepted cards | 1/12 | 0/12 |

The three reference corrections were concrete:

- Dallas Mavericks: the official Walter Clayton Jr. signing quote moved from wrong index 4 to correct index 5.
- Evan Neal: “Eagles sign OL Evan Neal” moved from wrong index 3 to a matching report at index 4.
- Brian Thomas Jr.: the Seahawks proposal quote moved from wrong index 6 to a matching report at index 10.

These corrections did not make their complete cards publishable. Full canonical-name requirements still reject abbreviated names in other findings. Neal's prose still invents a Giants departure/release when the source says Raiders release. Thomas's indexed response drops the control's “finalized deals” and `here_we_go`, but still turns media proposals into actions or concrete interest by the teams.

Source review found three especially clear new regressions:

- **Todibo:** the control correctly describes a loan at Lens from West Ham; the indexed response says he is on loan at West Ham United.
- **Mitchell:** the indexed response changes a plan to retire with the Cavaliers unless traded into a plan to leave unless traded, and invents a conflict between compatible headlines.
- **Quinn:** the indexed findings label Stephen A. Smith's end-of-tenure commentary as a denial. The body also weakens the distinction between one commentator's opinion and reported departure news.

Important failures are unchanged in both versions. Herbert's trade-talk shutdown becomes reported concrete interest. Green's findings borrow a trade pronoun referring to Porzingis. Roma's coach from metadata receives Pellegrini's injury. Both Jalen Johnson outputs correctly withhold movement findings from an altercation, but dated identity wording triggers the source-number guard. Howe's stored coaching affiliation becomes current news; the indexed variant additionally invents a causal connection to conflict at Old Trafford.

`review.json` contains all twelve paired judgments and links to receipts. Review was performed by Codex, unblinded against the frozen inputs; it is not independent human adjudication. This small, purposive, single-generation comparison supports the narrow reference-matching observation, not a general accuracy rate or proof that input indices always improve or worsen prose.

## Cost

| Measure | Existing input | Explicit indices |
| --- | ---: | ---: |
| Input tokens | 27,311 | 27,618 |
| Output tokens | 1,138 | 1,117 |
| Summed request seconds | 109.854 | 107.474 |

Total: **24 generations, 54,929 input tokens, 2,255 output tokens, 224.319 elapsed seconds**. The index fields added **307 input tokens, about 1.12%**. All calls completed; there were no context holds, automatic retries or clipped inputs. Prefix-cache effects and output lengths prevent treating the small timing difference as a speed improvement. No monetary or electricity cost was measured.

## Decision and validation

The index idea works as a narrow reference aid. The paired card results do not support adopting this exact input change as an overall Insider improvement, especially given the reversed loan direction and retirement condition. The runtime input and eight quality fixtures were restored to their pretrial versions; the experiment, patch, test, model identities and raw receipts remain available. This is a response to specific material regressions, not a requirement that every trial card be perfect.

The indexed candidate passed **415 tests, with 74 ignored**, including the new order/source-preservation test. The restored runtime passed **414 tests, with 74 ignored**. Formatting and diff whitespace checks passed; restored release package/output examples build. `candidate.patch` applies cleanly to the restored local source. Full database publication/recovery tests, production throughput and independent human quality review were not run.

The remaining dominant failures concern which person a claim belongs to, whether a report describes a real move or a proposal, preservation of denials/conditions, and importing metadata into news. No broader parser relaxation or additional prompt pass is included in this trial.

## Reproduction

Run from the Rust workspace. The current runtime is the restored control. To regenerate both arms, build and copy its `character_package` and `character_outputs` binaries to the `/private/tmp/scoracle-insider-unindexed-*` paths used by the scripts. Apply `candidate.patch` in an isolated copy of this local candidate and build the indexed binaries. The patch is relative to the preceding local six-character implementation, not the checkpoint alone.

- `selection.json`, inventory/capture SQL and JSONL, and `case-provenance.json` retain selection and exclusions.
- `identity-capture/` and `replacement-capture/` retain read-only identity/provenance captures and no-material records.
- `instructions.txt` is the unchanged instruction text used by both versions.
- `build_cases.py` uses the two Rust assemblers and verifies that only index fields differ.
- `run_trial.py` verifies model identity, alternates order, runs each pair once and saves actual requests/responses. Its `trial/` directory must be new; use a separate artifact copy when reproducing, preserving these receipts.
- `analyze.py` regenerates `analysis.json` and has a small self-check for wrong indices versus invented quotes. It checks all 24 receipts and equal non-prompt request settings.
- `review.json` records semantic findings separately from mechanical results.
- `trial/run.json` pins executable hashes and settings; `validation.txt` records checks and restoration.
