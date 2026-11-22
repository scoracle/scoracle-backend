# Baseline comparison — 2026-11-15

**No confirmed fixture-level regressions. Every failure from the plugin-skills branch also occurs on the old baseline. No production code or prompt changes were needed.** Existing quality issues still prevent a release qualification pass.

Compared baseline `9be32e645648ec814a9cba4981a6031d8d597c2c` against branch `codex/character-skills` at `1c4573c9026dde37203be7f4d574b9f7209645ff`.

## Results

All 79 Classifier requests completed without transport errors. Matching receipts by fixture ID, requests, model responses, scores, routing decisions and identity gates were identical; only timing and completion order differed. The baseline reproduces all 13 disagreements across 154 supplied criterion labels at the replay's 50% threshold. The 15 structured cases remain unlabelled and are not accuracy-scored.

All 54 Character fixtures had the same per-case status and reported failures: 52 model calls and two no-call dispositions, with no transport errors.

| Skill | Baseline checks | Branch checks | Outcome |
| --- | ---: | ---: | --- |
| Journalist | 47 / 83 | 47 / 83 | Same failures and printed outputs |
| Influencer | 6 / 6 | 6 / 6 | Same mechanical pass and printed outputs |
| Scout | 27 / 29 | 27 / 29 | Same failures and printed outputs |
| Insider | 52 / 73 | 52 / 73 | Same failures; two rejected responses vary |
| Analyst | 15 / 17 | 15 / 17 | Same failures; one passing response varies |
| Oracle | 16 / 16 | 16 / 16 | Same mechanical pass; one response varies |

Check totals are mechanical assertions, not editorial accuracy. In particular, Influencer coverage remains effectively parser-only.

## Differences investigated

- **Insider:** `fully-named-advanced-talks` produced 160 baseline versus 288 branch output tokens; `reliable-source-heating-steam` produced 139 versus 147. Both were rejected with the same reported validation outcome on each revision. The runner retains only excerpts of rejected responses, so this comparison does not claim full raw-response equality.
- **Analyst:** `sparse-samples-stay-steady` passed on both. The branch adds “or explicit performance metrics” to the statement that no directional comparison can be assessed.
- **Oracle:** `crossroads-conflict` passed on both. Wording differs while retaining the contrast between strong performance and a deteriorating surrounding mood. Mechanical checks do not independently establish factual quality.

All 54 prepared Character requests match byte for byte across revisions, including system prompts, user prompts, generation options and no-call decisions. All six parsers and the production evaluation logic match after normalizing moved module paths and formatting. This supports model variability, rather than a refactor-induced request or validation change, as the explanation for these differences. No new passing-to-failing transition occurred.

The original report's numeric-guard rejections, metadata leakage, thin-sample overstatement, Insider extraction failures and Classifier attribution/temporal disagreements are therefore pre-existing issues in these fixtures. They were not tuned as part of this regression check.

## Method and evidence

Used a fresh `git archive` of the baseline in an isolated temporary directory, without changing its source or either working checkout. Built the existing `eval` and `classifier_replay` runners offline. Reused the original fixture-mode commands, model endpoints, model identities, context setting and concurrency. All 58 input file hashes matched: 54 Character fixtures and four Classifier JSONL sets containing 79 cases.

Server properties matched exactly. Model listings differed only in their dynamic `created` timestamps. No server was restarted or reconfigured. Timing differed, so this is not a controlled performance comparison.

- [Comparison data](baseline-comparison.json): per-suite parity and the four changed cases.
- [Baseline summary](baseline/summary.json): every fixture outcome and classifier mismatch.
- [Baseline manifest](baseline/run-manifest.json): commands, exit codes, environment, concurrency and fixture hashes.
- [Prepared request parity](baseline/request-parity.json): 54 matching requests and their hashes.
- [Validation source parity](baseline/validation-source-parity.json): evaluation logic and parser comparisons.
- `baseline/*.stdout`, `baseline/*.stderr` and server metadata preserve the baseline evidence alongside the original branch logs.

Caretaker stayed inactive. No database access, worker startup, deployment, prompt tuning or source-code fix occurred. Evidence is retained with this branch.

This is one live run per revision, using the existing fixture runners. It does not requalify database publication, queue execution, production load or full editorial quality. The prior structural/integration checks remain separate evidence; no broad test rerun was needed because no production code changed.
