# Headline emotion comparison — October 19

Keep d1 for the next pipeline qualification. ModernBERT's one extra correct
Influencer routing decision was on a fictional control; d1 and ModernBERT tied on
the 21 real headlines. Laya missed too much relevant material in this configuration.
These are provisional diagnostics, not a qualified model selection.

| Configured candidate | Emotion F1 at 50% | Influencer routing F1 | Real-headline Influencer F1 |
|---|---:|---:|---:|
| d1-3B Q4_K_M | 0.750 | 0.810 | 0.545 |
| Laya Q8_0 | 0.500 | 0.167 | 0.250 |
| ModernBERT-large zero-shot NLI, float32 | 0.529 | 0.829 | 0.545 |

At the existing routing cutoffs, d1 had 17 true positives, four false positives
and four misses. ModernBERT had 17 true positives, three false positives and four
misses. Laya had two true positives, one false positive and 19 misses. This small
sample does not establish a meaningful advantage for ModernBERT. d1 also had
better Journalist and Insider routing F1 than the alternatives in these cases.

The shared real-headline errors matter: both d1 and ModernBERT attributed another
club's emotional reporting to the target in two cases, and both missed a social
media dispute. They differed on frustration wording and a named coach's “great
feeling.” Better target attribution and reviewed emotion examples should drive
further work. A confident score alone does not solve that problem.

## Candidates and method

[Laya's author documents](https://huggingface.co/convaiinnovations/laya) the English
checkpoint as ModernBERT-large with a trained decision head. The raw
[ModernBERT-large backbone](https://huggingface.co/answerdotai/ModernBERT-large)
is a masked-language model. The third candidate therefore uses
[MoritzLaurer's trained ModernBERT-large NLI head](https://huggingface.co/MoritzLaurer/ModernBERT-large-zeroshot-v2.0),
pinned at `a51e07b524299e309dd2b88d48b0cfa2bd9ec598`.
It is a general zero-shot classifier, with no sports-emotion tuning in this run.

All three saw the same 45 headline/publisher/subject states: 24 fictional controls
and 21 deliberately selected retained headlines. Controls include mixed feelings,
negation, sarcasm, historical emotion, neutral results, clickbait and another
club's feelings. The 21 real cases were reviewed against headline wording before
new predictions; earlier full-article labels were not treated as headline gold.
Labels remain assistant-reviewed and provisional. Six real cases are emotion
positives, so the real-only estimates are particularly unstable.

The two native models received byte-identical production requests exported from
`classifier_replay --requests-only`. The NLI model used the same state and fixed,
recorded declarative hypotheses for the five criteria. No thresholds, labels or
hypotheses were tuned after seeing the predictions. The operational cutoffs are
subject 55, emotion 15 and movement 80; these cutoffs are not calibrated equally
for every candidate. Raw-label metrics at 50% and per-cohort results are retained.

Each run excluded one warmup, then measured 45 sequential requests. d1 used its
existing Archbox GTX 1070 Ti endpoint. Laya and ModernBERT used four local Mac CPU
threads. Their observed median request times were 0.497 s, 2.080 s and 0.488 s,
respectively. Different hardware, quantization and request adapters prevent a
fair architectural speed ranking. ModernBERT kept complete token IDs and disabled
truncation; native receipts keep aggregate input-token usage, which does not prove
per-question tokenizer coverage.

[Machine-readable results](../fixtures/classifier/headline-emotion-benchmark-2026-10-19.json)
include fingerprints, revisions, metrics, receipt hashes and the private local
receipt directory. Public [fictional controls](../fixtures/classifier/headline-emotion-controls.jsonl)
and the [runner](../examples/headline_model_bench.py) are reusable. No production
database rows, model assignments or services were changed. The temporary Laya
server is separate from the production services.

This measures the emotion material selected for Influencer. Her ability to turn
those headlines and memories into a faithful emotional narrative needs its own
character-output review.
