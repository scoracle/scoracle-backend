# d3 classifier qualification and rollout — October 27, 2026 (Detroit)

The user authorized deploying d3 and tuning classification prompts while production fetching and character generation remain paused. This candidate replaces only the decision model and its four questions. It does not restart character generation, replay the archive, alter routing thresholds or publish generated articles.

## Qualification

The native `/v1/systemone` request contains the headline, publisher, canonical metadata and supplied `noul` choices. The existing Rust exporter matched all 82 original evaluation requests exactly. Version `classification-criteria-v11-targeted` strengthens entity/sport identity, explicit emotional attribution, present/proposed movement versus history, and current availability including denials. Routing stays `headline-routing-v2` (subject 55, emotion 15, movement 80, availability 85).

The 82 development cases include 42 captured real headlines, 36 controls and four saved decorated spam cases. Two candidate prompt banks were compared with v10; v11a was selected on development routing mistakes, with Brier score as tie-break, before running the frozen fresh set. No thresholds or prompts changed after its held-out results. The 40 fresh cases contain 24 captured real inputs across NBA/NFL/football and 16 controls. Labels are assistant-authored provisional expectations, not independent human gold. Spam is rejected by the existing deterministic guard and is not credited to the model.

| Native GTX 1070 Ti held-out result | d1 / v10 | d3 Q8_0 / v10 | d3 Q8_0 / selected v11 |
|---|---:|---:|---:|
| Raw binary-label judgments | 138/153 | 147/153 | 149/153 |
| Routing judgments | 143/150 | 131/150 | 144/150 |
| Real-input raw judgments | 83/89 | 87/89 | 88/89 |
| Real-input routing judgments | 83/86 | 79/86 | 84/86 |
| Brier score | 0.06671 | 0.03506 | 0.02360 |
| Median request seconds | 0.339 | 0.435 | 0.481 |

Development routing improves from d3 v10's 280/293 to v11's 283/293. All completed qualification outputs were valid. Sixteen repeated calls at concurrency four matched sequential scores exactly. These small samples support the user-authorized monitored rollout, not a statistically established routing win. d3 is slower on this production GPU, despite being faster in the earlier Mac MXFP8 trial.

Remaining v11 failures include inferred emotion in a neutral Bronny spotlight headline, missing the 85 availability cutoff for Ravens injury reporting, a denial of transfer talks below the movement cutoff, and false relevance for similar college/town names. Human adjudication of fresh false positives/negatives is the next useful tuning input; do not repeatedly tune against this held-out set.

## Pinned artifacts

- Official Ollama `d3:2b-q8_0` manifest: `e1b3991aa4806936d30a6d624b234aca3f99ba89d61d6d3a17ee81739f94dafa`.
- Text GGUF SHA-256: `47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707`; native alias `vllm-sr/d3-2b-Q8_0@sha256:47d85eed4b76e2613d2b1496f6a106841fc605638f96acf46ef54ad1a9f45707`.
- Official Ollama 0.40.12 Linux amd64 archive SHA-256: `f8adcb7e08d272283a4bd3abc18cf581afe37e64b2aee04fda4f407f215485ba`. Its bundled native runtime supports the same decision interface without an adapter. CUDA 12 is selected explicitly for Pascal. Global Ollama installations remain unchanged.
- d3 uses Apache 2.0; the official registry manifest identifies license blob `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30`. This text-only path does not load a vision projector.
- Retained d1 alias: `LiquidAI/d1-3B-GGUF@sha256:16aff27ea2eefdc32b9897f43854a5d3170c1dc8dccb9c756905af30a4e22402`.
- Launcher uses context 16384, four slots, batch/microbatch 4096, all GPU layers. A 512 microbatch failed the native non-causal-attention assertion during isolated smoke testing; 4096 passed sequential and concurrent qualification.

Full private evaluation data, exact requests/responses, frozen labels, timing and check logs are retained at `/Users/scotty/Documents/Codex/2026-10-27/task/d3-rollout/artifacts`. The source-free summary is `rust/fixtures/classifier/d3-qualification-2026-10-27.json`. Remote runtime/model staging is `/mnt/data/backup/scoracle/d3-rollout-20261027`.

## Compatibility, activation and rollback

Migration 325 extends the existing headline provenance trigger to v11 without rewriting stored rows. Delivery and memory queries explicitly retain v9/v10. Tests cover historical retrieval, valid v11, missing admission, mismatched headline and wrong evidence scope, including upgrade from schema 324.

Build from the committed candidate. Use `release.sh --build-only` with a scratch `RELEASE_BIN_DIR` to retain the standard seven-binary build. This narrow rollout installs only the changed cognition binary on Archbox and the paused Mac worker: Go/API code is unchanged and need not restart. The classifier service receives a drop-in pointing to the pinned d3 launcher, preserving the d1 unit configuration. Stop the previously active classifier worker for the model/binary cutover; restart only that worker. Keep coverage/fetching and the Mac character worker stopped. Confirm native model identity, actual Rust decision output and worker build identity before declaring activation. Do not force work into queues just to prove traffic.

Before activation, save the current binaries, Mac launcher, service configuration and old database function in the remote/local rollout rollback directories. To roll back, stop the classifier worker, remove only the d3 drop-in, restore its saved cognition binary, restart the retained d1 model service, then restart the previously active classifier worker. Restore the Mac launcher while leaving it stopped. Migration 325 is additive and compatible with the old binary, so it can remain. Never remove the retained d1 model/runtime or resume paused character work as part of rollback.

This source record describes the tested candidate and activation procedure. The external `DEPLOYMENT.md` receipt in the local rollout directory records actual activation, binary hashes, service state and remaining limitations after cutover.
