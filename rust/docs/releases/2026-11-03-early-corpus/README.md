# Early November 2 corpus

The user authorized pausing classification and generating reviews immediately from already-classified evidence, with Journalist still paused. This is a separate operational run after the completed monitoring window. No monitoring automation was restarted.

Classification stopped gracefully on Archbox. All 12,921 pending classifier jobs and their unsettled drain obligations were preserved. The existing classifier model server remains available, but its queue consumer has no registered plugins. A reversible user-service override runs only the coverage controller. Mac and Archbox writers continue with `vibe,transfers`; the writers retain their normal outbox handling.

The November 3 edition froze at **2026-11-03 03:15:25.815770 UTC**, using actual capture time and retaining the midnight Detroit cutoff so products report **November 2**. The transaction committed around 03:16:08 UTC. Initial work was 410 Influencer and 193 Insider assignments. These are assignments, not promised published-card counts. Empty assignments have explicit no-evidence outcomes. Paused character assignments and unfinished classification remain recorded, so this is an intentionally partial edition.

The one-off transaction used a session-local copy of the live `coverage_tick` function with only the draining wait guard removed. The public scheduler function was unchanged. Production headline selection ran for Insider and Influencer, retaining source identity checks, score ranking, deduplication and cap ten. It recorded 434 selection dispositions. Assertions checked unchanged classifier rows and obligations, no running claims before freezing, the reporting cutoff, required stages, paused Journalist obligations, unchanged public scheduler definition, and the ten-article limit.

Two rollback previews timed out during headline selection, at 30 and 110 seconds. The query plan substantially underestimated pending deliveries. Refreshing planner statistics for assignments and deliveries allowed the third rollback preview and the applied transaction to complete; selection took roughly 43 seconds. The third preview's JSON summary decoder initially failed on multiline JSON after the successful rollback; its intact receipt was decoded afterward. All receipts remain private. No prompts, parsers, model weights, binaries, concurrency settings, source-check policies or retry counts changed.

At **03:20:22 UTC**, the new reporting-day corpus contained:

| Character | Saved rows | Qualified rows |
| --- | ---: | ---: |
| Insider | 31 | 23 |
| Influencer | 25 | 12 |
| Total | 56 | 35 |

Saved rows include assignments still processing and assignments held by source checks; qualification is not factual accuracy. Early held-source categories include unresolved target identity and incomplete reporting-day evidence. Insider content warnings include unsupported named claims, unsupported relationships, opening-name style and finding-contract diagnostics. Observation mode still permits content/style warnings. No structural model rejections were present in the captured host receipts; this does not mean every assignment published.

Public API verification matched both the saved body and hook for one newly qualified card of each character, reporting November 2. Both returned HTTP 200. Python's default user agent initially received an edge HTTP 403; the same read with a browser user agent succeeded. This was an access-layer response, not a model failure.

Both writer models and all checked Archbox services were healthy. Archbox's first 35 completed calls used 1,083–2,065 native input tokens. Its median request times were 4.505 seconds for Insider and 6.021 seconds for Influencer. The earlier Mac startup snapshot contains 11 calls, with median times 11.946 and 23.404 seconds respectively. These snapshots have different capture times and are not a controlled host benchmark.

Private operations and corpus archives:

- Archbox operations, rollback previews, applied SQL and service backup: `/mnt/data/backup/scoracle/releases/early-corpus-20261103/`.
- Archbox exact requests, saved outputs, delivery/assignment receipts and its diagnostic records: `/mnt/data/backup/scoracle/early-corpus-20261103/`.
- Mac diagnostic records and per-call telemetry: `/Users/scotty/scoracle-worker/monitoring/early-corpus-20261103/`.

The separate read-only collector copies have a November 4 noon UTC expiration and are manual tools, not scheduled jobs. The original monitoring collector and completed baseline remain untouched. Production continues retaining generation ledger entries, saved outputs and append-only diagnostics as the corpus grows. Only aggregate summaries are stored beside this document; raw requests, outputs, logs and API responses must stay private.

To capture tomorrow's completed corpus before tuning, run these manual collectors:

```sh
ssh archbox 'python3 /mnt/data/backup/scoracle/early-corpus-20261103/collector/capture.py archbox'
python3 /Users/scotty/scoracle-worker/monitoring/early-corpus-20261103/collector/capture.py mac
```

For review, group examples by character, target type, sport, evidence count, qualification and diagnostic category. Keep successful, held and abstaining cases. Review actor/source attribution, unsupported additions, story coverage, hook/body usefulness and score meaning separately. Compare future prompts on the same saved target and evidence, retaining versions, runtime settings and native tokens. This is development material, not a held-out accuracy result.

Classification stays paused until explicitly resumed. The pause file is `/home/sheneveld/.config/systemd/user/scoracle-cognition.service.d/90-corpus-pause.conf`. Tomorrow, remove only that override, reload the user service manager and restart `scoracle-cognition.service` after checking the live edition and intended classifier build. Do not restore queue tables or reset retries. Normal daily gating still determines when classifier claims are admitted. Journalist remains independently paused by the earlier writer configuration and required-stage change.

No application build or test suite was run for this operational change. Verification used rollback previews with assertions, applied-state read-back, actual generations, private corpus capture and public API matches. Concurrent classifier source edits and the unrelated recovery-planning document were preserved and not committed.
