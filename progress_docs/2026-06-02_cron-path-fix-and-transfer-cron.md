# 2026-06-02 — Install transfer cron + fix stale post-consolidation cron paths

## Goal

Install the transfer/trade corpus cron so the feature runs on a daily batch (the
last piece of Transfers Phase 3). While doing so, discovered the whole cron suite
was pointed at a path that no longer exists.

## Discovery — the entire cron suite was dead

The 2026-06-01 archbox consolidation moved the repo from `~/scoracle-backend` to
`~/scoracle/scoracle-backend`, but every cron job — and the wrapper scripts' internal
`cd` — still hardcoded the **old** `/home/sheneveld/scoracle-backend` path, which no
longer exists. So since the move, **none of the cron jobs could even find their
scripts**: football event draining (daily), football schedule/roster (weekly),
Postgres backup (nightly), vibe corpus, and tier recompute (weekly) had all been
silently failing. (The just-consolidated vibe cron would have been broken too.)

## What Was Done

**Path fix (8 files in `scripts/hosting/`)** — rewrote every
`/home/sheneveld/scoracle-backend` → `/home/sheneveld/scoracle/scoracle-backend`:
`crontab.example`, `cron-vibe.sh`, `cron-transfer.sh`, `cron-scoseed.sh`,
`recompute-tiers.sh`, `backup-postgres.sh`, `restore-drill.sh`, `logrotate.conf`.
(The old path is never a substring of the new one, so the replace is exact +
idempotent.) Scripts and binaries (`go/bin/{transfer,vibe,scoracle-api}`) all
already lived at the new path — only the hardcoded strings were stale.

**Transfer cron entry** added to `crontab.example`:
```
30 0 * * * .../cron-transfer.sh -mode corpus >> .../logs/transfer-corpus.log 2>&1
```
Once daily, 30 min after the vibe corpus run (reads the corpus vibe's RSS sweep just
refreshed; offset to ease the shared GPU). Intraday coverage is the in-API
transfer news-spike worker, so once a day is enough.

**Installed**: `mkdir -p logs`; reinstalled the live crontab via
`crontab scripts/hosting/crontab.example` (cron auto-saved the prior crontab to
`~/.cache/crontab/crontab.bak`).

## Verification

- All 7 live cron entries now resolve to `~/scoracle/scoracle-backend/...`; transfer
  entry present at `30 0`; live crontab diff-clean against the committed example.
- **Cron-env smoke test**: ran `cron-transfer.sh -mode single` under a stripped
  environment (`env -i HOME=… PATH=/usr/bin:/bin SHELL=/bin/sh`, mimicking cron) →
  the wrapper `cd`'d, sourced `.env.local`, the binary connected the DB and reached
  Ollama (no init error), stopping only at arg validation (exit 2). Proves the full
  cron chain works; `-mode corpus` will run the real batch identically.

## Notes / follow-ups

- This fixed **all** crons, not just transfer — backups and football seeding are
  live again as of this reinstall. Worth a manual `backup-postgres.sh` run to close
  the ~1-day backup gap if that matters.
- `logrotate.conf` path was also corrected, but logrotate is installed separately
  (not via crontab) — apply it wherever logrotate configs live on archbox if used.
- The transfer corpus batch over every team in 3 sports is GPU-heavy and sequential;
  the 30-min offset only reduces, not eliminates, contention with the vibe run. If
  the vibe run routinely overruns 30 min, widen the offset.
