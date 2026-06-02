# 2026-06-02 — Transfers/Trades Phase 3: real-time trigger + listener + cron

## Goal

Close the Transfers feature: automate generation so a breaking transfer cycle
refreshes a team's rumors within minutes (not at the next cron), and schedule a
comprehensive batch. Clones the Vibe dual-trigger model (cron batch + on-demand
news-spike LISTEN/NOTIFY).

## What Was Done

**`034_transfer_trigger.sql`** — extends the existing `notify_vibe_trigger()`
(migration 011) to ALSO `pg_notify('transfer_trigger', ...)` on the *same* 4→5
news crossing, **teams only** (the card's grain; a player spike is usually match
coverage, not a transfer). One function, one `COUNT(DISTINCT ...)`, two notifies —
a second trigger would double per-insert cost for the same signal.

**`internal/listener/transfer_worker.go`** (clones `news_volume_worker.go`) —
`StartTransfer` LISTENs on `transfer_trigger`, auto-reconnects, dispatches async.
GPU is the central risk (one Archbox GPU shared with vibe), so two governors:
- **global concurrency cap** — buffered-channel semaphore (`TRANSFER_MAX_CONCURRENT`,
  default 2); waiters block, never piling onto the GPU.
- **per-team in-flight guard** — `sync.Map` keyed by team; a same-team burst can't
  launch the team twice (the DB debounce only updates *after* a run finishes, so it
  can't dedupe mid-run).
Plus the DB-backed **60-min debounce** (`recentlyTransferred`, survives restarts,
re-checked after acquiring a slot in case a cron run beat us) and heat-only rows
that always render, so saturation degrades to numbers-without-fresh-summaries.

**`config.go`** — `TRANSFER_ENABLED` (default true), `TRANSFER_DEBOUNCE_MINUTES`
(60), `TRANSFER_MIN_ARTICLES` (2), `TRANSFER_MAX_CONCURRENT` (2).

**`cmd/api/main.go`** — wires `StartTransfer` behind `TransferEnabled`, reusing the
**already-pinged** `ollamaCli` (no second ping; `newsVolumeGen != nil` is exactly
"Ollama reachable"). nil gen → listener still runs, logging spikes only.

**`scripts/hosting/cron-transfer.sh`** + `go/bin/transfer` — cron wrapper mirroring
`cron-vibe.sh`. Recommended crontab (staggered 30 min after the vibe corpus run so
vibe's RSS sweep has refreshed the corpus and they don't contend head-on):
```
30 0,12 * * * /home/sheneveld/scoracle-backend/scripts/hosting/cron-transfer.sh -mode corpus >> .../logs/transfer-corpus.log 2>&1
```
(Not installed — left for the user to add to crontab.)

## Verification

- `go build` / `go vet` / `go test ./internal/{config,thirdparty,ml}` all green.
- **Trigger fires exactly once**: LISTEN + 5 separate team-link inserts (mirrors
  `persistArticles`' per-statement writes) → one `transfer_trigger` NOTIFY on the
  5th, correct payload `{team, id, sport, count:5}`. The 4 prior inserts: silent.
- **End-to-end**: fired a real West Ham (team 1) spike → API logged
  `Transfer news spike` → dispatched → `transfer: rumors refreshed
  team="West Ham United" candidates=8 rumors=4 cleared=4 duration=1m4s`; rows
  written with `trigger_type='news_spike'`.
- **Debounce holds**: a second `transfer_trigger` for West Ham (fresh row present)
  was *received* but skipped — `rumors refreshed` count unchanged (no re-gen).
- Startup log: `Transfer rumor worker enabled` + `Transfer listener connected
  channel=transfer_trigger max_concurrent=2`. API rebuilt + redeployed.

## Found (out of scope) — pre-existing vibe bug

The West Ham crossing also fired `vibe_trigger`; the vibe worker then **failed**:
`new row for relation "vibe_scores" violates check constraint
"vibe_scores_trigger_type_check"` — the `vibe_scores` CHECK omits `'news_spike'`
(the `007:16` gap the plan called out; `transfer_rumors` got it right from day one).
**Vibe news-spike scoring has silently never persisted.** One-line migration to add
`'news_spike'` to that CHECK; flagged for a follow-up since it's the vibe pipeline,
not transfers.

## Result — Transfers feature complete

Phases 0–3 in: schema + deterministic heat + endpoint/card (P1), Gemma vetting +
deterministic roster-direction + former-player gate (P2 + refinement), co-mention
proximity precision, and now real-time automation (P3). Follow-ups: the vibe
`news_spike` CHECK fix; a two-team-flood load test of the concurrency cap (logic
verified, full simulation deferred); optional News-tab adoption of `title_pos`.
