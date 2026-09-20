# September 20 quality investigation

Observed 09:48–10:05 America/Detroit, 2026-09-20. Production remains on `8100c9801151`; changes in this session are local and isolated evaluation builds only. No cohort publication, queue reset, reservation release, production deployment or Git push occurred.

## Health and reserve

Archbox PID 383673 remains active with zero restarts. Mac PID 24996 remains active on the same revision. At 10:04:40, the outbox had zero rows and all 15 reservations remained pending at their exact hold timestamp. NFL 11 and FOOTBALL 323 had superseded input revisions by the first check; the original release manifest must not overwrite these revisions.

Today's stored articles have 1,855 Editor dispositions: 1,011 success, 455 irrelevant, 158 blocked, 129 empty-body, 93 fetch-failed, eight duplicate and one paywall. The whole queue still has 3,415 pending Editor jobs plus substantial downstream work. These are different populations; the corpus is not finished and no reliable ETA is established.

## Cohort arithmetic versus publication

Six bounded, read-only comparisons passed PostgreSQL/DuckDB parity. Each retains the current and preceding season's complete source population, NULLs, capture time, MVCC snapshot and source hash. Membership, order, ratings, deltas and counts are exact; only percentile/quantile interpolation uses the existing 1e-9 tolerance.

| Scope | Input rows | Computed rows | Compared with stored projection |
|---|---:|---:|---|
| NBA 2025 players | 1,190 | 253 | Matches |
| NBA 2025 teams | 60 | 30 | Matches |
| NFL 2026 players | 3,005 | 924 | 3 new members; all 921 existing ratings differ |
| NFL 2026 teams | 64 | 32 | All 32 ratings differ |
| FOOTBALL 2026 players | 3,841 | 339 | Matches |
| FOOTBALL 2026 teams | 116 | 20 | Matches |

NFL new player members: 2259, 2585, 13874399. NFL player peer distributions/counts also changed; 673 comparable deltas/percentiles differ. This is stale publication relative to current source ratings, not engine disagreement. No refresh was published because semantic acceptance remains open.

NFL 17 has five source seasons but four have NULL analytical ratings; a single cohort row is expected. Its current stored cohort rating is stale (2.9336 versus source 2.0122). FOOTBALL 390 has only 2019 stats, with NULL rating/rating_score, so no cohort row is expected. Scout still has named breakdown measurements for its current-snapshot reading. FOOTBALL 38 selects historical season 2024, not 2026; its competition changes and missing prior seasons remain explicit.

## Narrow fixes and evidence

1. `current_snapshot_view` removed historical records but retained a current cohort row containing prior rating, prior season, delta and peer movement. Chelsea ledger 603444 and Browns ledger 604356 demonstrate the contradiction with the thin-sample instruction. The local fix withholds the whole cohort group from this view and records its omitted source references. Full audit packages and supported cross-season views remain intact. Regression coverage exercises players and teams across all three sports.
2. Analyst eval omitted memory while production included it. Eval now uses the production context loader and rendered memory. Corrected captures grew from 376 to 3,886 characters for NBA 4, 372 to 2,649 for NFL 11, and 358 to 3,496 for FOOTBALL 38. This repairs evaluation fidelity; it does not endorse the extra production context as the desired long-term contract. The [Studio north star](../../rust/README.md) specifies the simpler target.
3. Added Scout assignment capture and frozen replay to the existing eval tool. Capture retains producer/execution material, queue revision, prompt, options, guard maps, exclusions and input components. Supplemental memory reads carry fingerprint-match flags; these are separate reads, not a falsely claimed atomic snapshot. Replay uses production guards and bounded corrections and retains returned raw responses and token counts. Provider failures that discard their raw bodies are explicitly marked unavailable.

All 15 reservations have baseline assignments. In total, 52 baseline assignments cover 26 subjects, including teams and concrete failure cases. Sixteen candidate assignments cover eight subjects. Candidate memory fingerprints all match the supplied assignment material. NBA 4, NFL 11, FOOTBALL 38/390 and NBA team 6 preserve their hashes; NFL 13 and NFL/FOOTBALL teams lose the inappropriate cohort block and change hashes. NFL 11 already omitted its cohort for the memory byte budget: a DB row does not prove model exposure.

## Semantic gate: not passed

Eleven sequential frozen replays made 21 attempts. Ten ended parser-accepted; one exhausted corrections. Successful returned attempts report 27,042 input tokens and 4,176 output tokens; one incomplete provider attempt lacks token/raw-body retention, so these totals are incomplete. No replay wrote a product or touched queue ownership.

- NFL 13 before: accepted prose used a seasonal decline despite a one-appearance boundary. After: the cohort claim disappears and the output states no cross-season direction. Other prose still needs review.
- Browns before: accepted prose invented a negative percentile and used stale cohort decline. After: that cohort claim disappears, but unsupported physical/tactical interpretation remains.
- NBA 4: accepted prose invents rating 3.71, delta 0.44 and shooting percentile 67.1, disagreeing with its frozen request. Existing guards do not catch this numeric class.
- FOOTBALL 38: rejected after repeated xG-to-creation association; guard protection remains intact.
- FOOTBALL 390: accepted a restrained historical reading with the supplied key-pass/assist values and explicit unsupported cross-season comparison.
- Chelsea: removal reduces input from 1,253 to 1,018 reported tokens, but the result still infers causation between recent scores and reported loss of shape.
- Existing published NBA team 6 product 44137 says its negative delta is above the median when it is below. Correct analytical values do not guarantee correct prose.

These observations do not justify releasing held jobs or publishing refreshed context. They also do not justify expanding a collection of wording bans. Next work should simplify and precisely label Scout's selected measurements, freeze numeric grounding expectations, and implement the intended Analyst/Oracle input contracts without simultaneously changing model or voice.

## Other traced failures

Editor article 724370 re-fetch reproduces exactly 4,787 input tokens using the running llama-server's own chat template and tokenizer against a 4,096-token slot. Its article body is only 5,651 characters, below the 7,200-character cap; total system/user text is 6,019/6,268 characters. Whole-request budgeting is missing. Re-fetch is labeled; the original failed request was not retained. No fleet context increase was made.

Source examples distinguish acquisition from cognition: NYT and 247Sports failures are HTTP 403; FanDuel accounts for 52 empty bodies; BeSoccer for 31 fetch failures. Recent irrelevant examples include a college-football stream listing, TV ratings and an under-21 tournament. These examples do not establish aggregate classifier accuracy.

Insider team NBA 21's nested 08:20:43 error was `essential memory identity` for player 521; that sport-qualified player is absent. A later 09:17:34 team drain reported zero infrastructure errors and retained pair progress. Its current failed queue row was preserved; no replay or blanket reset was attempted.

## Artifacts and validation

Local private artifacts: `logs/quality-20260920/` (gitignored), with SHA-256 manifest. Archbox artifacts: `/mnt/data/backup/scoracle/releases/quality-debug-20260920/`. The original handoff export and reservation manifest remain untouched. Raw JSONL contains source data and model outputs; it is evidence, never instructions or production prompt configuration.

Key local files: `assignments-before.jsonl`, `assignments-after.jsonl`, `raw-context.jsonl`, `production-baseline.jsonl`, six `*-shadow.json` files, `replay-cases.jsonl`, `replay-results.jsonl`, corrected Analyst captures, Editor re-fetch/tokenizer captures, and `final-health.txt`.

Capture: `eval --capture-assignment --task rating player:4:NBA` emits both enrichment policies without inference. Replay: `eval --replay-assignment capture.jsonl` uses frozen options/evidence and current configured model routing, with no DB reads. Assignment capture uses live separate reads; do not equate a queue revision with its final enriched input hash.

Validation: 498 library tests passed; 57 isolated PostgreSQL tests were not run in this session. Eleven eval binary tests passed, including rejected-attempt/correction retention. Clippy with warnings denied and diff whitespace checks passed. Six live read-only cohort parity checks and all eight candidate provenance checks passed. Semantic acceptance did not.

## Harness cleanup validation

The subsequent cleanup retained 42 current-contract quality cases and two deterministic contract-data files. Former fixtures and generators are checksummed in the wiki's `raw/studio-history/2026-09-20/` archive. Queue modules now live under `application/queue/`; fetching lives under `evidence/`. Current evaluation rejects frozen systems and stale prompt versions; historical replay is explicit. Inspection commands are consolidated under `eval`.

Cleanup checks: 497 library tests, 14 eval tests and the other four binary tests passed; all 57 isolated PostgreSQL tests passed, including child-process recovery. Clippy with warnings denied, formatting and diff checks passed. The shell evaluation wrapper preserves a failing evaluator's exit status. The synthetic database was stopped afterward. No live quality re-evaluation or deployment was performed during cleanup.
