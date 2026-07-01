# 2026-06-02 — Vibe news_spike CHECK fix + vibe cron 2/day → 1/day

## Goal

Two related cleanups surfaced while finishing Transfers Phase 3:
1. Fix the `vibe_scores.trigger_type` CHECK so real-time (news-spike) vibe scores
   actually persist.
2. Now that the in-API news-spike worker provides intraday coverage, drop the vibe
   corpus cron from twice daily to once daily — the noon pass is redundant.

## What Was Done

**`035_vibe_news_spike_trigger_type.sql`** — `vibe_scores.trigger_type` CHECK
(migration 007) only allowed `'milestone' | 'manual' | 'periodic'`, but the
news-volume LISTEN/NOTIFY worker has always written `'news_spike'`. So **every
spike-driven vibe generation failed at persist** (`violates check constraint
vibe_scores_trigger_type_check`) — real-time vibe scoring silently never landed a
row; only the twice-daily `periodic` cron + `milestone`/`manual` paths did.
Widened the CHECK to include `'news_spike'`, matching the full set the code emits
(and matching `transfer_rumors`, which had it right from migration 031).

**Cron consolidation** (`crontab.example` + reinstalled live crontab, plus
`cron-vibe.sh` / `cron-transfer.sh` header comments) — vibe corpus
`0 0,12 * * *` → `0 0 * * *` (midnight only). The former noon pass existed to keep
every team within ~12h of fresh news; the news-spike worker (5 articles in 60 min
→ immediate local model rescoring) now covers breaking cycles in real time, so the second
batch is redundant. `cron-transfer.sh`'s recommended stagger updated to `30 0` to
follow the single daily vibe run.

## Files Changed

```
sql/migrations/035_vibe_news_spike_trigger_type.sql   (NEW)
scripts/hosting/crontab.example                        (vibe 0 0,12 → 0 0)
scripts/hosting/cron-vibe.sh                            (header comment)
scripts/hosting/cron-transfer.sh                        (stagger comment 30 0,12 → 30 0)
```

## Verification

- Migration applied; live constraint now
  `CHECK (trigger_type IN ('milestone','manual','periodic','news_spike'))`.
- A full-shape `news_spike` vibe row INSERTs successfully (rolled back). Pure DB
  change — the already-running API's vibe worker is fixed without a rebuild; the
  next news spike will persist a row instead of erroring.
- `crontab scripts/hosting/crontab.example` reinstalled; `crontab -l` shows
  `0 0 * * * … cron-vibe.sh -mode corpus`; no `0,12` entries remain; live crontab
  diff-clean against the committed example. (cron saved a backup of the prior
  crontab to ~/.cache/crontab/crontab.bak.)

## Notes

- Before this fix there are **0** `news_spike` rows in `vibe_scores` (periodic
  15936 / milestone 1292 / manual 16) — direct evidence the spike path never
  persisted. New spike rows will accrue from here.
- The transfer cron itself is still not installed (documented in
  `cron-transfer.sh`); add `30 0 * * * …cron-transfer.sh -mode corpus` when ready.
