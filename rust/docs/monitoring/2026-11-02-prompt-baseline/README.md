# Fixed-prompt monitoring window

The user requested a few more hours of monitoring before trying new prompt concepts, with enough retained material for larger comparisons. Six thread follow-ups are scheduled 30 minutes apart, ending around 10 PM Detroit time on November 2 (03:00 UTC November 3). Automation: `monitor-scoracle-prompt-baseline`. The older `monitor-backlog-plumbing` automation belongs to another thread and remains paused.

Keep this run observational. Do not change prompts, parsers, model weights, article cap, concurrency, queue state or production configuration; do not retry rejected outputs or invoke extra generations. The baseline uses Journalist `n135-ten-articles`, Insider `insider-source-v36-paragraphs`, Influencer `vibe-frame-v48-paragraphs`, Qwen3.5-4B-Q4_K_M, temperature zero, thinking disabled, ten articles. Mac and Archbox share at most four character claims; Archbox has one Journalist slot. Existing guard failures are data, not permission to relax validation.

## User-directed policy change

The user subsequently requested that model-content failures reach production for review. The fixed-validation baseline ends at the deployment boundary documented in `../../releases/2026-11-02-observe-content/README.md`. After that boundary, `CHARACTER_CONTENT_POLICY=observe` allows structurally usable Journalist, Insider and Influencer candidates past content/style failures. Prompts, model weights, native schemas, cap ten, concurrency and source/claim checks remain unchanged. This explicitly supersedes the earlier instruction to keep blocking content validation throughout the window.

The collector now separates `observed_validation_attempts` from `rejected_generation_attempts`; old receipts without a disposition remain rejections. An observed attempt is allowed past validation, not proof of publication or factual correctness. Compare strict and observation periods separately; higher publication rates after this switch are not a quality improvement. Do not requeue old holds, relax structural checks or make further production changes while monitoring. Resume the existing schedule after rollout, still ending by 03:00 UTC; delete the automation after the final report.

## Each scheduled check

Run from `/Users/scotty/scoracle/scoracle-backend/rust`. Use a fresh timestamp in output filenames so prior snapshots remain intact:

```sh
python3 docs/monitoring/2026-11-02-prompt-baseline/capture.py mac > /private/tmp/scoracle-mac-TIMESTAMP-summary.json
ssh archbox 'python3 /mnt/data/backup/scoracle/prompt-baseline-20261102/collector/capture.py archbox' > /private/tmp/scoracle-archbox-TIMESTAMP-summary.json
```

Both collectors are read-only against production. The Archbox collector uses a read-only PostgreSQL session with a statement timeout. It saves full generation ledger records (including exact requests), generated product rows and qualification state, assignment outcomes and delivery metadata to its private archive. Each host separately preserves its complete, dated rejected-generation records. Only aggregate summaries return to the workspace. Private files are created with owner-only access. No source payloads, outputs or production logs belong in GitHub.

Canonical snapshots remain at:

- Mac: `/Users/scotty/scoracle-worker/monitoring/prompt-baseline-20261102/TIMESTAMP/`
- Archbox: `/mnt/data/backup/scoracle/prompt-baseline-20261102/TIMESTAMP/`

Each has `summary.json`, `rejections.jsonl` and `model-calls.log` (per-call hashes, target keys, native tokens and durations for later host attribution); Archbox also has `evidence.jsonl`. These are cumulative snapshots since 23:34:50 UTC, the current release's first writer activation. Compare the newest summary to the immediately preceding successful snapshot on each host. Full source archives remain untouched. Older rejection records with no timestamp are excluded from this dated window and counted separately; they are not deleted. Incomplete/invalid JSON is counted and should be investigated if nonzero or increasing. The collector stops accepting new captures after 03:10 UTC, allowing the last scheduled check to finish.

Check service/model health, GPU memory, running claim counts, queue counts and attempts, current reporting edition, required stages, article cap and publication progress. An idle queue or collector waiting for the character phase is not itself a stall. Investigate no model progress across an interval while eligible work is pending, repeated transport failures, unavailable services, or unreadable archives. Report incidents; do not silently change the experiment to fix them.

For each host and character, report interval model calls/tokens/wall time by subtracting cumulative totals. Lifetime p50/p95 cannot be subtracted; label them cumulative. Model-call duration includes time in the request path and is not an isolated hardware benchmark. Check prompt/model versions in product groups for drift. If the user explicitly changes an experiment, label the boundary and preserve this baseline first.

## Counts and interpretation

Keep these units separate:

- **Completed model calls:** native call telemetry, including calls whose output later fails validation.
- **Rejected generation attempts:** one append-only rejection record per rejected generation attempt, not one per source article. Group by character and normalized validation category. Full diagnostics stay in the private records.
- **Job events:** queue executions; a successful execution can abstain or settle a held result and is not necessarily a published card.
- **Saved product rows:** persisted outputs, including those still hidden by assignment qualification. Counts can include multiple rows for a target.
- **Distinct targets:** counted within each character/sport/entity-type/prompt/date group. Do not sum across dates as if globally unique.
- **Qualified rows:** independently visible candidates at snapshot time. Qualification can later change, so deltas are not an immutable count of successful generations.
- **Delivery states:** source obligations, not model calls or independent accuracy samples.

Do not derive a factual accuracy rate from any of these counts. Archive/log timestamps are close but not atomic across hosts; label interval estimates. Repeated retries of the same evidence are not independent examples. Accepted ledger capture is best effort; successful raw provider replies are not universally retained, so record the exact request and saved normalized output and disclose that limit. Rejected raw replies are retained in full.

## Final follow-up and later comparisons

On the scheduled run at or after **02:45 UTC**, capture the final snapshot, give the three-hour report and pause this automation through the automation tool. Do not schedule further checks or continue beyond 03:00 UTC. The six-run schedule is also finite. Keep the user-facing report concise and link to this record; call out substantial failures and any gap in monitoring.

Build the next comparison from these saved requests and outputs, with successes, abstentions and failures included. Stratify by character, sport, entity type, evidence count and failure category. Keep the same target and evidence in each paired prompt comparison, preserving dates and source wording. Record prompt/model/runtime/host and input/output token counts; differences between hosts or changing news packets are not prompt effects. Split development cases from later untouched cases. Compare attribution, actor relationships, uncertainty, factual support, coverage of distinct stories, hook/body usefulness and Influencer score meaning separately from parser acceptance.

This monitor gathers the corpus. Future prompt changes and comparative generations remain a separate user-directed step; no extra model passes are introduced here.
