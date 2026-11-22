# Journalist three-context pilot — 1 November 2026

Status: **not ready for rollout**. Scope correction: report eligibility belongs to the decision model. The character is evaluated on fidelity to the selected material, not on repeating entity-routing decisions. Journalist now summarizes what supplied reports say and discuss; it is not asked to discover significant narratives. The existing `narratives` JSON key and storage contract remain compatible.

The request has `meta`, `fresh`, and `tone`, with evidence rules and the task in the system message. Meta uses the existing database identity reader. Fresh preserves source wording, publisher, publication time and explicit evidence scope. Classifier routing assertions stay out of the writing context. Journalist neither loads nor injects headline history; legacy history in stored parts is ignored.

## Evaluation

The local Qwen3.5-4B-Q4_K_M ran eight frozen cases twice in each of two rounds, without database access or publication. `before` omits the optional database record; `after` retains it. These labels do **not** mean old versus new prompts. Cases with no record provide identical-input repeats. Both rounds retain fixtures, raw outputs and provider receipts. The first round used the initial three-context summary prompt; the second removed classifier assertions and clarified metadata boundaries.

- Initial round: 7/16 responses passed mechanical checks.
- Publisher-only fresh context: 6/16 passed mechanical checks.
- These counts are not factual accuracy rates, and the second round also adds explicit checks for metadata leakage.
- The assistant reviewed all outputs against the supplied reports; no human factual approval is recorded.

The earlier rubric incorrectly counted faithful summaries of an upstream routing mismatch as character failures. Those eligibility judgments belong to the decision model; the original counts are retained as historical results, not current quality scores. Character failures include rewriting reported subjects, mixing database attributes into publisher reporting, inventing details, or omitting required attribution. The existing numeric guard also rejects a supported result-and-scorer summary. The preference-discussion case confirms the model can summarize a topic without needing a new event, but metadata still contaminates its attributed version.

See [review.json](review.json) for findings by case and [the latest results](qwen-source-only/results.jsonl) for mechanical checks. Each case directory contains `fixture.json`, `output.json` and `receipt.json`; the receipt preserves the exact instructions actually sent.

## Corrected character boundary

The `n113-character-summary` prompt removes report-eligibility judgments from Journalist's task. The decision model selects evidence; the character summarizes it while preserving its named subjects. Metadata remains background for understanding and must not supply publisher claims. Profile-topic summaries are permitted when limited to what the headline actually says.

The [corrected trial](qwen-character-boundary/results.jsonl) ran eight cases twice (16 responses); 7/16 passed mechanical checks. This changed rubric is not comparable to the earlier eligibility-based scores. Both reported-subject cases preserved the source subject in both variants. The remaining issues are metadata mixed into reporting, omitted attribution, and a false headline saying the club confirmed a fee that the evidence leaves unconfirmed. Existing numeric-guard false rejections remain separate.

See [the corrected evidence review](character-boundary-review.json). Some mechanically accepted responses fail factual review. These are the character-quality failures to address; the upstream routing mismatch is not itself a character failure.

## Reproduce

From the Rust workspace, use a new output directory:

```sh
cargo run --example headline_policy_trial -- \
  fixtures/journalist/three-contexts-2026-11-01/character-boundary-cases.jsonl \
  /tmp/journalist-summary-retest \
  http://127.0.0.1:8081 Qwen3.5-4B-Q4_K_M
```

The runner uses the current assembler, one attempt per variant, no publication, temperature zero and thinking disabled. Review complete titles and bodies, including rejected replies. Do not use parser acceptance as a substitute for evidence review.

## Validation limits

All targets compiled; formatting and diff checks passed. 384 library tests and 14 evaluation CLI tests passed; 73 environment-dependent library tests were ignored. Live database metadata loading was not exercised because no database connection was configured. Other character prompts, parser rules, model routes and deployed services were not changed.
