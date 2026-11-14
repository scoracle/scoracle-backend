# Three-model corpus rollout

User authorized committing the approved prompt work before the experimental governing rule, then deploying Classifier, Journalist and Insider and processing the backlog. Checkpoint: d2ee094c. The experimental governing rule remains outside production. Classifier runtime omits the rejected other-entity enrichment, matching the selected no-context baseline.

Models use task → target_context → target → evidence_context → evidence. Classifier v17 / brief-v8 keeps existing admission thresholds. Journalist n145 and Insider v43 use their task-first prompts. Quality checks are advisory for 48 hours after activation through the existing observation mode. Invalid structured outputs and source/publication-integrity failures remain protected and archived; observation does not guarantee every generation can be published.

Migrations 331 and 332 preserve old receipts while accepting the new request envelope and retaining headline provenance guards. No historic spectra are rescored. Reuse existing services: Classifier on Archbox, Journalist on Mac, Insider on Archbox writer. Preserve other services. Save previous configurations and binaries privately before activation. Inspect current-edition failed Journalist work before retries; preserve coverage scheduling and prior results.

Validation: Rust library suite: 413 passed, 75 ignored. Live activation and migration verification will be recorded below.

## Activation

Activated November 14, 2026 at 5:11 p.m. EST from commit `2d052ea1b720`. Observation expires automatically **November 16, 2026 at 5:10:53 p.m. EST** (epoch 1794867053). Workers remain enabled after expiry; subsequent validation decisions return to strict mode.

- Mac Journalist: `/Users/scotty/scoracle-worker/releases/three-model-20261114-2d052ea1/scoracle-cognition`, SHA-256 `b8b696710f60e09ae9755f031d0092f15414ac58f892943c3ca01af438975620`.
- Archbox Classifier and Insider: `/mnt/data/backup/scoracle/releases/three-model-20261114-2d052ea1/scoracle-cognition`, SHA-256 `423c72772290ffac62d20151c3fb6cf67b22a3b99b917cd451754dbad379a86c`. Built from the exact committed archive; embedded commit reads `unknown` because the archive has no Git metadata. This manifest and binary hash identify the release.
- Classifier retains its d3 endpoint on 8081. Insider uses the existing Qwen endpoint on 8082. Journalist uses its existing Mac endpoint on 8081. Stage selections are classifier, transfers, and narratives respectively.

Migrations 331/332 applied successfully. A temporary-table transaction verified the new brief-v8 receipt is accepted and a mismatched headline is rejected, then rolled back. All three workers started successfully; API, collector and model services remain active.

At 5:12:34 p.m. EST, 11 new v43 Insider outputs and 11 associated prompt/request ledger records were persisted. The first nine strict-quality warnings were all `observed`, including non-exact quotations and unsupported named/numerical claims. None of those warnings blocked publication. This verifies collection behavior, not factual accuracy.

The current November 4 edition remains in its character phase: Insider has 657 pending jobs and one running. Classifier has no currently claimable work; Journalist has 52 exhausted queue rows. Of the 189 failed Journalist assignments, 185 already have a daily publication and four have unresolved target identities. No blind resets were performed. New Classifier and Journalist production outputs are not yet verified; their services await the scheduler after the current character backlog resolves. Large source sets can require multiple claims, so output count is not the same as completed-assignment count.

## Corpus and rollback

Successful character products remain in `insider_scores` / `news_summaries`, with prompts and requests in `cognition_ledger`; Classifier receipts and failures remain in its spectrum/measurement tables. Full observation/rejection records are private JSONL files in the release directories: `transfers-decisions.jsonl`, `classifier-decisions.jsonl`, and `journalist-decisions.jsonl`. No retention policy was shortened.

Mac rollback: restore the release directory's `rollback-run-worker.sh` to `/Users/scotty/scoracle-worker/run-worker.sh`, then signal its LaunchAgent with SIGINT for a graceful restart. Archbox rollback: stop the writer, remove only the `three-model-corpus.conf` drop-ins from the cognition and writer units, reload systemd user units, and restart cognition. The old binaries and unit snapshots are retained in the release directory; the original writer was inactive. Leave compatible migrations and all collected results intact. No Git push was performed.

## Continuous corpus follow-up (activation pending approval)

The user requested immediate live outputs and Archbox Classifier with Mac Journalist/Influencer. Mac Influencer now uses the same compatible `2d052ea1` release and common observation deadline; its prior launcher is saved as `rollback-influencer-launcher.sh`. Insider continues on Archbox. New Mac Influencer startup was verified.

Migration 333 prepares continuous-mode API views for Journalist, Influencer and Insider (including transfer rumors). When the existing coverage scheduler is enabled, its normal publication filters remain authoritative. When explicitly disabled, those views expose all retained rows and `published_edition()` returns NULL so default readers choose the latest reporting date. Prior definitions are saved privately in `rollback-live-views.sql`.

Automatic approval review rejected the combined scheduler-disable and failed-job-reset action because it may cause duplicate/invalid reprocessing and required more specific authorization. It did not execute. The scheduler remains enabled in the November 4 character phase. An explicit approval question is pending for continuous mode and the 90 failed jobs (52 Journalist, 38 Influencer). No retry or schedule mutation was made. Raw queue state was retained privately before the proposed action. At the latest check, 49 new Insider outputs were stored.
