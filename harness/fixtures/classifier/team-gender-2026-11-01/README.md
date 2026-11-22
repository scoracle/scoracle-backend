# Team-gender metadata and d3 — 1 November 2026

The model endpoint through the existing Archbox tunnel serves `vllm-sr/d3-2b-Q8_0`, fingerprint `47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707`. These are read-only model trials; no decisions were published.

## Database and candidate audit

The live database has 144 football, 30 NBA and 32 NFL teams. None of those 206 teams has `meta.gender` or `meta.team_gender`; there is no dedicated gender column. None of five leagues has either gender key. Sample retained decision receipts contain no team-gender metadata. Those historical receipts are from October 22 and identify d1; the current model identity was checked directly at the serving endpoint.

The existing candidate filter makes a separate decision before model inference: `Chelsea Women appoint Jo Vale as head coach` yields no candidate, but `Chelsea's women's team appoint Jo Vale as head coach` yields Chelsea. Metadata alone cannot repair this earlier filtering inconsistency. Audit queries and results are retained in `database-audit.sql` and `database-audit.jsonl`.

## Controlled results

All scores use the existing admission threshold of 55. Gender and club are synthetic fixture facts, not a backfill of real entities. The first four variants each include six matching-team cases, six opposite-gender cases and six cases with gender missing.

| Input/target wording | Matching teams admitted | Opposite gender rejected |
|---|---:|---:|
| Gender in identity prose | 6/6 | 0/6 |
| Explicit structured gender field | 6/6 | 1/6 |
| Structured gender plus an extra identity instruction | 6/6 | 0/6 |
| Gender in the question's target description | 6/6 | 6/6 |

Binding the target as **the men's football team named Cedar United** (or women's) worked in this controlled set. Supplying gender as background alone did not. No score thresholds were tuned.

The final run used the actual updated Rust request builder, with gender only in `subject_meta.team_gender` (no duplicate gender prose). It added possessive wording and unrelated-club cases: **18/18 labeled cases correct**, comprising 8 matching-subject admissions and 10 other-subject rejections. The six additional missing-gender cases were all admitted; they are deliberately not scored as known-gender decisions. Missing metadata is still an unresolved production gap.

See `production-builder-cases.jsonl`, `production-builder-requests.jsonl`, `production-builder-results.jsonl` and `review.json`. Experimental raw requests/responses are retained for the explicit-instruction and target-description comparisons. These small synthetic sets do not establish production-wide accuracy; mixed-category rendering is unit-tested but not model-qualified here.

## Prepared implementation

The local code reads `public.teams.meta.gender` into `subject_meta.team_gender` and binds a known value into all four decision questions. Supported canonical values are `men`, `women`, and `mixed`. Missing/null stays unknown and keeps the existing target description. Invalid values fail before inference. The existing request hash includes both metadata and question wording, preventing reuse of a differently described target's decision.

This uses the existing JSON metadata column; no migration or real-data update was applied. Acquisition/backfill must supply verified values, not infer men from an unqualified name. The earlier SQL name-suffix filter remains unchanged. No model threshold, character prompt or deployed service was changed by this decision-model test.

## Reproduce and limitations

```sh
cargo run --example classifier_replay -- \
  fixtures/classifier/team-gender-2026-11-01/production-builder-cases.jsonl --requests-only
CLASSIFIER_URL=http://127.0.0.1:18081 cargo run --example classifier_replay -- \
  fixtures/classifier/team-gender-2026-11-01/production-builder-cases.jsonl 1
```

385 active library tests passed; 74 environment-dependent tests were ignored. A new isolated database-to-request test covers men, women and unknown metadata through `subject_meta`. Its disposable local PostgreSQL instance could not initialize because the host's System V shared-memory segment limit was exhausted. Host settings and other processes were left untouched. The live production database audit completed read-only; the full synthetic database integration test remains unrun.

To run that integration test, provide an **empty isolated database named `classifier_test_gender`** in `TEST_DATABASE_URL`, then run `cargo test --lib canonical_team_gender_reaches_the_decision_target -- --ignored`. It creates only synthetic test tables in that explicitly named database.
