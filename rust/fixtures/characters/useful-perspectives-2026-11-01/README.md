# Six useful perspectives: local implementation and evaluation

The shared contract and bounded instruction pass are implemented locally. The final real-data replay produced useful individual cards, but neither nine-in-ten routine usability nor daily edition capacity is demonstrated. Serious recurring errors survive mechanical acceptance, and complete dense packages exceed the existing context budget. This is a reviewable candidate, not a deployment recommendation.

This work starts from `b812a94c`. It supersedes the perfection-focused next-experiment recommendations in the earlier Journalist investigations. Their 215 source-preparation calls remain diagnostic evidence; no preparation stage, model judge, larger model or routine rewriting was adopted. No database data, serving visibility, deployed binaries or model installation was changed. The unrelated production-recovery planning document was preserved.

## Implemented contract

- All six characters use `tools/context.rs`: stable top-level `meta`, `fresh`, `tone`, in that order. Each role has one authoritative `instructions.txt`, sent separately as system instructions. Production and replay use the same typed assemblers; Analyst and Oracle now retain replayable Parts rather than only frozen prompt strings.
- Meta carries canonical identity and populated database records, including role, position, affiliation, location, explicit team gender and available observation dates. Missing fields stay missing. Location is retained even when venue is missing. D3 retains identity/routing ownership; its selection is not factual verification.
- Fresh retains exact source wording, article IDs, URLs, publisher dates and evidence scope where available. Existing request-local report keys still map to original publication sources. Scout keeps availability separate from measured statistics, including inside validation, so an availability number cannot authorize an invented performance statistic.
- Influencer and Insider stop loading historical headline memories. Legacy fields remain readable for older captures but are excluded from writing inputs. Current finished peers and dated numerical comparisons do not enable historical generated-prose injection.
- Journalist permits up to three distinct natural summaries, topic-level supporting links for ordinary claims, and explicit attribution for distinctive claims, quotations and disagreements. The shorter first draft regressed; the final pass restores the checkpoint's fuller source-ownership guidance and worked examples while retaining these product changes.
- Influencer distinguishes coverage tone from actual expressed feelings and population sentiment. Insider preserves reported movement stages and denials; co-mentions do not establish a move. Scout explains supplied measurements, code-computed standings and limits. Each role has representative examples.
- Analyst now exposes up to two existing dated comparisons: rating notability and coverage-tone score change, with code-computed sign, units, sample counts, separate windows and computation date. Invalid/missing comparisons are omitted. The loader requires the current reporting date, season and candidate edition, preventing accidental use of older incompatible snapshots. Without comparisons, the task is current balance, not invented trajectory. Missing-peer generation is skipped unless a valid comparison is available.
- Oracle receives available finished perspectives with their original dates and narrative source article IDs. Missing cards remain unknown; repeated stories are not independent confirmation. The existing subject-name publication check now also runs through the replay request parser.
- Eligible changed packages get one generation per execution attempt. Routine output-correction calls are disabled for these six roles. Existing source validation, queue claims, atomic publication, unchanged-product reuse and durable queue retries remain. A queue retry can still generate again; this is not a lifetime one-call guarantee.

Full-input token admission, model identity and completion checks remain enabled. No source is silently clipped. No schema migration or edition-completion policy change is included. Existing conservative prose guards remain and can still block otherwise useful wording. The older numeric Analyst contract still represents an absent momentum snapshot as steady/zero; the new prose does not treat that sentinel as an observed trend.

## Evaluation design and provenance

The endpoint was the existing `http://127.0.0.1:8081`, alias `Qwen3.5-4B-Q4_K_M`. Saved server properties identify the Q4_K Medium model at `/Users/scotty/models/Qwen3.5-4B-Q4_K_M.gguf`, llama.cpp build `b11429-d81235049`, four slots and a 10,240-token server context. The application retained its stricter 7,900-token request envelope, including output reservation. Thinking was disabled. No simultaneous benchmark or compilation was deliberately started during inference; the shared server was not exclusively reserved.

The checkpoint baseline is an 18-case existing fixture screen across all six roles. These are curated/synthetic edge cases, not a representative real-data accuracy baseline. A first-pass real replay then establishes the local candidate's operational baseline on the same 42 packages used for the final workload. The two real runs are repeated development evaluations, not independent held-out qualification.

The 42-package workload covers 11 subjects across football, NBA and NFL, and team/player/person types: Liverpool, Boston Celtics, Green Bay Packers, Bruno Fernandes, LeBron James, Sam LaPorta, Jerry Jones, Roberto De Zerbi, Martín Demichelis, Bradley Beal and Drew Lock. It deliberately includes popular dense subjects and sparse reporting, disagreements, missing peers and incomplete measurement samples. It is purposive, not randomly sampled or weighted to the routine queue distribution. Statistical stages remain scoped to teams/players.

`character_capture` uses PostgreSQL connections with `default_transaction_read_only=on` and a statement timeout. Some live reporting deliveries were already settled, so historical reporting cases were reconstructed from frozen assignment spectrum IDs and their original classifier-source records. Original D3 candidate lists determine identity holds; missing identities are not filled from model knowledge. `case-provenance.json` records included article IDs, exact-copy exclusions and unresolved-target holds. Captured identity metadata is current-at-capture; sources retain their original dates. These are frozen historical assignment packages, including older catch-up reporting, not a claim of newly collected November news.

Scout and peer fixtures use the production loaders. Analyst and Oracle consume **actual saved finished cards**, not outputs generated earlier in this replay. The runner imposes upstream → Analyst → Oracle barriers, but is not an end-to-end regenerated edition. Some saved peers already overstate measurements; downstream source fidelity cannot correct all inherited errors.

No current matching dated Analyst snapshot existed for the sampled subjects. Older October 16/20 snapshots were not substituted. Two explicitly synthetic dated-comparison cases exercise distinct windows and missing peers. They test the newly exposed contract, not real trend accuracy. The final metadata-only addition preserves explicit team gender; the three sampled team records all have missing gender, verified read-only in `meta-gender.txt`, so their benchmark packets are unchanged.

The six difficult Journalist packets are retained as regressions. They use the same 29 reports actually served in the previous development control, in original corpus order, without experimental grouping or preparation notes. The original Parts also held a larger corpus; the initial failed trial mistakenly served that larger corpus. This is recorded rather than hidden.

## Output quality

`review.json` records every final screen and workload case against its saved source packet. This is Codex's unblinded source review, **not independent human adjudication**. “Usable” requires a supported central reading and useful character purpose, while allowing awkward wording. Small factual/editorial corrections are separate from materially misleading subjects, measurements, status and relationships. Parser status is recorded independently.

For the final real workload's **24 generated outputs**:

| Review judgment | Count |
| --- | ---: |
| Usable without an essential edit | 8 |
| Central reading supported, small edit needed | 5 |
| Materially misleading | 11 |

Six of the eight usable outputs also passed mechanical checks. Two useful outputs were rejected: Bruno's Scout summary used recorded-profile phrasing outside the narrow recorded-appearances guard, and Jones's Insider used the canonical full club name where headlines used “Cowboys.” This is not a recommendation to remove source guards wholesale. Conversely, multiple serious errors passed. Counting mechanical acceptance as truth would materially overstate quality.

| Role | Final real evidence |
| --- | --- |
| Journalist | Demichelis's two discussions are now faithful; the first pass invented a new signing and former-club context. Beal's central reading improves but a full-name expansion triggers rejection. Lock still gets another player's seven interceptions and an invented retain-or-trade decision. Eight dense packages never reach generation. |
| Influencer | Jones's negative tone is now supported; the first pass reversed the critic and invented a balanced emotional reading. De Zerbi and Demichelis preserve their central expressed enthusiasm but still add small unsupported interpretations needing edits. Three dense packages are held and three team packages have unresolved-identity holds. |
| Scout | Packers and LaPorta preserve supplied measurements and sample limits. Bruno's relative profile is useful but rejected by the narrow appearance-wording guard. Liverpool silently truncates percentiles. LeBron leaks instructions and uses a team framing. Boston changes rebounds/blocks/steals into “points” and attributes one team's values to its entire cohort; the parser accepts. |
| Insider | All six real outputs are mechanically rejected. Jones's buyer/seller reading is usable. Other cases still strengthen reported status, misidentify movement from co-mentions or attach findings to the wrong report index. Boston's response declares a completed transaction; Liverpool invents active negotiations and an Iraola movement finding. |
| Analyst | Four cases skip generation because a peer/comparison is absent. LaPorta's generated reading confuses missing temporal comparisons with inability to assess the current balance and strengthens relative ranks into measured success. Both synthetic dated cases incorrectly say distinct windows are the same; one also copies “detached and economical” from tone into reporting. |
| Oracle | LaPorta and Liverpool provide useful smaller syntheses. Bruno gains an invented potential/reality gap. LeBron strengthens bands into capability. Boston reverses elite rebounding/turnover standings into weaknesses and invents lack of cohesion, repeating a serious first-pass error. All five final Oracle outputs pass mechanical checks. |

The common 18-case checkpoint screen moves from 10 accepted / 7 rejected / 1 no-call to 14 accepted / 3 rejected / 1 no-call. That is a mechanical comparison only. Positive supporter coverage stops inventing community activity, Scout's small football example retains its four goals, and the denial response now emits `denied` with null stage, although sourced chronology still triggers rejection and an unstated pronoun needs editing. Insider's interest quote still omits the target's full name.

The full final screen adds the two synthetic trend cases and six regressions: 26 attempts, 24 generations, one no-call and one context hold. The AC Milan known packet is 5,827 input tokens plus 2,100 reserved output, just over 7,900. Among generated regression packets, Milan still merges two people, Bears reserved invents a Thanksgiving streak despite parser acceptance, and Clippers reserved asserts an unsupported same-game relationship. Clippers known is a useful small reading rejected for expanding Clippers to LA Clippers; Bears known needs its supporting opponent source key. These packets remain diagnostics, not the entire product definition.

The proposed nine-in-ten routine target is **not achieved or statistically calibrated** by these results. Even counting all five small-edit cases as usable would leave substantial recurring material errors. Further review should prioritize source ownership, measurement meaning and reported status rather than stylistic perfection.

## Processing cost and actual queue state

All costs below retain failed attempts and zero-call paths. “Input tokens” means full supplied prompt tokens for completed generations; prefix reuse still occurs. Summed request time is not elapsed time when four slots overlap. Per-case receipts retain actual requests, responses, completion reasons and timings. No electricity or monetary cost was measured.

| Run | Attempts | Generations | Input tokens | Output tokens | Summed request seconds |
| --- | ---: | ---: | ---: | ---: | ---: |
| Checkpoint fixture baseline | 18 | 17 | 15,764 | 1,349 | 75.221 |
| Initial candidate screen, including admission failures | 24 | 3 | 2,882 | 277 | 14.348 |
| Corrected admission / regression screen | 21 | 20 | 22,678 | 2,149 | 128.838 |
| First real workload | 42 | 24 | 40,640 | 3,106 | 625.625 |
| Final screen plus regressions | 26 | 24 | 41,400 | 1,773 | 152.345 |
| Final real workload | 42 | 24 | 46,852 | 2,528 | 672.775 |
| **Total for this investigation** | **173** | **112** | **170,216** | **11,182** | **1,669.152** |

The initial screen's failures include exact-input template verification rejecting trailing system whitespace. System strings are now trimmed at the authoritative boundary, preserving exact-input verification rather than weakening it. Oversized regression attempts in that run also made no generation call. The 112 calls exclude all earlier investigations' calls.

The common 18 final fixture cases used 19,623 input / 1,024 output tokens and 66.789 summed seconds versus the baseline's 75.221 seconds. Shorter outputs and cache effects matter; this is not a general speedup claim.

The first four-slot real workload took **163.922 elapsed seconds**. The final took **174.195 elapsed seconds**, with maximum local queue wait **150.512 seconds**. The final run evaluated 45,716 uncached input tokens; summed server prompt/decode durations were 214.757 / 421.757 seconds across slots. Extra source provenance and fuller instructions increase input cost. Twenty-four calls for this mixture do not imply six calls per entity or six-character cost amortization.

| Final workload role | Packages | Generations | Accepted | Rejected | Context holds | No-call |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Narratives | 11 | 3 | 1 | 2 | 8 | 0 |
| Vibe | 9 | 3 | 3 | 0 | 3 | 3 |
| Scout | 6 | 6 | 4 | 2 | 0 | 0 |
| Transfers | 6 | 6 | 0 | 6 | 0 | 0 |
| Analyst | 5 | 1 | 1 | 0 | 0 | 4 |
| Oracle | 5 | 5 | 5 | 0 | 0 | 0 |
| **Total** | **42** | **24** | **14** | **10** | **11** | **7** |

The read-only actual workload audit was captured November 1 at 21:27 America/Detroit. The latest edition remained October 23 in `characters`; earlier editions were partial. It had 1,589 pending Narratives assignments (median four source memberships, maximum 426), 156 failed Narratives assignments and 1,034 pending Sigil assignments. The separate work queue had 767 pending and 830 failed Narratives jobs, 1,034 pending and 26 failed Sigil jobs, 47 failed / four pending Rating jobs and eight failed Momentum jobs. Assignment counts and queue jobs are different populations and must not be added as a call forecast. Many other assignments were already no-new-evidence or abstentions.

This audit and bounded replay do **not** certify a complete daily drain. Full packages as large as the captured LeBron Narratives request cannot fit the retained context envelope. Six rejected real Insider calls and accepted-but-misleading syntheses also make generated-output throughput a poor measure of useful production capacity. The simple one-call architecture is economical in stages, but current useful output yield does not justify treating all generated cards as finished work.

## Validation and next bounded step

`cargo test --lib --examples --bin eval`: **414 passed, 74 ignored** (391 library, 14 eval binary, nine examples). The checkpoint's comparable library/examples total was 398; the new comparable total is 400. `cargo fmt --check` and `git diff --check` pass. Release capture/package/output examples build. Tests cover shared serialization, production/replay inputs, dated-comparison admission, no-rewrite behavior, Oracle subject validation and keeping availability numbers out of performance evidence.

The 74 ignored checks, full disposable-PostgreSQL publication/recovery suite, production queue drain and end-to-end regenerated edition were not run. No production writes were used to create missing comparisons or unblock the edition. Existing false-positive guards and all-or-nothing edition behavior remain a material operational limitation: this pass does not yet ensure minor wording issues cannot block an edition.

The next bounded step should address the **Insider source-reference contract**, not resume per-article preparation: expose explicit code-assigned report indices beside each report, then run one fixed comparison on twelve newly captured routine movement packages across the three sports, including denials and no-move co-mentions. Keep one generation per arm, the same model, complete evidence and the existing publication checks. Review prose and findings together; stop after that comparison. Wrong measurement meaning, Oracle reversals, Analyst window merging and dense-package admission remain separately documented risks rather than reasons for another unrestricted prompt loop.

## Reproduction and artifacts

- `baseline-cases/` and `baseline/`: checkpoint fixture inputs and output receipts. Reproduce using the checkpoint's `character_outputs` example in an isolated checkout.
- `real-capture/`, `sparse-capture/`, `frozen-reporting.sql`, `frozen-reporting.jsonl`, `case-provenance.json`: read-only capture inputs and provenance. Historical capture files retain their original prompt versions.
- `build_cases.py`: rebuild current replay fixtures through the Rust `character_package` assembler. It uses frozen files; it does not query or mutate the database.
- `run_workload.py NEW_DIRECTORY`: four-slot replay with stage barriers, model checks, per-case receipts and queue timing. It does not publish or access the database. The output directory must be new.
- `summarize.py`, `analysis.json`: reproducible cost aggregation, counting each root experiment ledger once. Assertions reject incomplete runs.
- `review.json`: source-based judgments for all final cases, with receipt paths and concrete rationales.
- `server-props.json`, `workload-audit.sql`, `workload-audit.jsonl`, `validation.txt`: model identity, actual frozen workload state and local checks.

Run commands from the Rust workspace. Re-running the model is nondeterministic for roles using nonzero production temperatures; stored receipts are the evidence for the numbers above. Do not overwrite a retained experiment directory.
