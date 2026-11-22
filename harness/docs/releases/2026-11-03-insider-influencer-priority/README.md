# Insider and Influencer priority

The user requested pausing Journalist so available Insider and Influencer work can drain. This is a new user-authorized operational change after the completed monitoring window; the monitoring automation remains deleted.

At 2026-11-03 02:59:36 UTC, changed only `coverage_schedule.required_stages` from `{narratives,vibe,transfers}` to `{vibe,transfers}` under `coverage_lock()`, after a rollback-only preview and checking no character claims were running. All Journalist assignments, source deliveries and queue entries remain preserved.

Restarted the existing Mac and Archbox writers gracefully with `COGNITION_STAGES=vibe,transfers`. Mac startup at 02:59:40 UTC and Archbox startup at 02:59:43 UTC both confirmed exactly these two plugins and the existing Qwen endpoints (Mac 8081, Archbox 8082). Binaries remain `73a9882f62f4`; no build or code deployment. The Archbox writer retains its one-claim limit, Mac retains the per-plugin limits, and the existing global claim limit remains unchanged. Observation mode, ten-article cap, prompts, parsers and model weights remain unchanged. The classifier/controller and collection services were not restarted.

At 03:00:11 UTC the schedule was draining the November 3 edition. There were 399 Influencer targets with 1,628 pending source deliveries and 188 Insider targets with 559 pending source deliveries. Queue rows were 399 Influencer and 347 Insider; queue counts are not identical to targets with usable evidence. Classifier had 13,231 pending and four running jobs.

This is a priority change, not an early-edition bypass. Existing policy releases writers only after the classifier drain settles and no earlier than midnight Detroit time. No new generation was claimed as verification; read-back confirmed writers remain closed while the classifier drains. Next eligible generation will be limited to Insider and Influencer. Journalist will not block the daily cycle while paused.

Private backups and transaction receipts:
- Mac: `/Users/scotty/scoracle-worker/releases/prioritize-insider-influencer-20261103/`
- Archbox: `/mnt/data/backup/scoracle/releases/prioritize-insider-influencer-20261103/`

Rollback: restore the saved Mac launcher and Archbox writer unit, restore the saved required-stage array under the coverage lock, reload the Archbox user service manager and gracefully restart the writers. Do not restore whole queue/delivery tables or erase work completed since this change.

No source changes, parser tests or model smoke generations were needed for this configuration-only change. Verification consisted of the transactional preview, applied-policy read-back, plugin registration and endpoint-health startup checks. The concurrently modified classifier work and unrelated planning files were not touched or committed.

The user subsequently authorized pausing classification and generating from already-classified evidence immediately. The early edition and reversible classifier pause are documented in [the next operational record](../2026-11-03-early-corpus/README.md). This supersedes the waiting-for-midnight behavior described above.
