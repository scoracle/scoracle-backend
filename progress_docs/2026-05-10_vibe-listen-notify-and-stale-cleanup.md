# Vibe — corpus-only cron + LISTEN/NOTIFY news-volume trigger

Date: 2026-05-10

## Goal

Stop serving stale vibe scores on entity profiles, and replace the
fixture-driven legacy paths with a clean two-layer pipeline:

1. **Corpus cron** (twice daily) — broad, predictable refresh.
2. **News-volume LISTEN/NOTIFY** (real-time) — when a news cycle
   actually breaks for an entity, score it within minutes instead of
   waiting for the next cron pass.

The trigger for this work was a Detroit Pistons profile showing a
sentiment of 90 generated 2026-05-02 — visibly stale on 2026-05-10, and
a multiple-of-10 from the v2 prompt that pre-dated the anti-rounding
fix in v3.

## Decisions

- **Cron schedule.** `0 0,12 * * * cron-vibe.sh -mode corpus` (midnight
  + noon ET). Replaces the old `0 3 * * * -mode batch` line. Corpus
  mode RSS-sweeps every team in NBA/NFL/FOOTBALL and runs local model only on
  entities whose corpus picked up something fresh — independent of
  fixtures, so offseason and eliminated teams don't go dark.
- **Drop legacy fixture-driven paths entirely.** `runBatch` and
  `loadStarterCandidates` in `cmd/vibe/main.go`, plus the
  `vibe_worker.go` milestone listener wired to `percentile_changed`,
  are all gone. Single + corpus modes remain. The `percentile_changed`
  channel itself stays — FCM push notifications still consume it.
- **News-volume trigger.** SQL trigger on `news_article_entities`
  AFTER INSERT counts distinct articles for the entity in the trailing
  60 min; when this insert is the 5th, fires `pg_notify('vibe_trigger',
  ...)`. Threshold-crossing semantics (only fires when `prior_count =
  4`) mean a 50-article storm sends one NOTIFY, not 46.
- **Threshold = 5 articles / 60 min.** Conservative starting point.
  Routine teams won't trip; real news cycles (trade, injury, scandal)
  will. Easy to tune.
- **Single corpus, sport segregated by column.** Kept the existing
  `news_articles` + `news_article_entities` schema rather than
  migrating to per-sport schemas. The `sport` column on the link table
  already gives per-sport queries via one indexed join. No migration
  needed; both cron sweeps and user-driven `/news/{entityType}/{id}`
  fetches write into the same corpus, so the trigger sees both signal
  sources without a separate user-search log.
- **API filters at the read layer.** Three filters now sit on the
  vibe-read SQL:
  - `sentiment IS NOT NULL` — pre-v2 blurb-only rows
  - `prompt_version <> 'v2'` — multiple-of-10 rows from the legacy
    prompt
  - `generated_at > NOW() - INTERVAL '72 hours'` (latest endpoint only;
    history endpoint keeps full timeline) — anything older than 3 days
    signals offseason / no-news rather than current vibe; 404 lets the
    frontend render the honest "Training" state.

## Accomplishments

### Active crontab swap
Old: `0 3 * * * cron-vibe.sh -mode batch -sport all -since-hours 24`
New: `0 0,12 * * * cron-vibe.sh -mode corpus`
Backup of previous crontab at `~/.cache/crontab/crontab.bak`.

### Manual corpus pass
Ran `cron-vibe.sh -mode corpus` immediately to refresh stale data
without waiting for noon. Final tally:
- RSS sweep: 158 teams, 0 failures, 3 min.
- local model queue: 334 candidates, 306 fresh v3 scores, 0 failures, 0
  no-corpus markers, 28 skipped (already-recent), 1h 58m.
- Detroit Pistons (NBA team_id=9): now 58, prompt v3, 2026-05-10
  11:01:54 — non-round, current.

### Code changes

**Removed:**
- `go/internal/listener/vibe_worker.go` (milestone real-time path).
- `runBatch`, `loadStarterCandidates`, batch flags in
  `go/cmd/vibe/main.go`.
- `vibe *VibeWorker` parameter on `listener.Start` /
  `listener.listenLoop`.
- Old `0 3 * * * -mode batch` cron line.

**Added:**
- `go/internal/listener/news_volume_worker.go` — `StartNewsVolume()`
  goroutine, dedicated pgx connection on `vibe_trigger`, 30-min
  per-entity debounce, calls `ml.Generator.Generate()` with
  `trigger_type='news_spike'`.
- `sql/migrations/011_vibe_trigger_news_volume.sql` —
  `notify_vibe_trigger()` plpgsql function and
  `trg_vibe_trigger_on_news_link` trigger; adds covering index
  `idx_news_entities_lookup_created (entity_type, entity_id, sport,
  created_at)` so the trigger's window count is index-only.

**Edited:**
- `go/internal/api/handler/vibe.go` — added `prompt_version <> 'v2'`
  to `GetLatestVibe`, `GetVibeHistory`, `GetHottestEntities`; added
  72h freshness cap to `GetLatestVibe` only.
- `go/internal/listener/listener.go` — package doc lists both
  channels; `Start` signature drops `*VibeWorker`.
- `go/cmd/api/main.go` — replaces `vibeWorker = NewVibeWorker(...)`
  with a bare `*ml.Generator` for the news-volume worker; spawns
  `listener.StartNewsVolume(...)` next to the existing
  `listener.Start(...)`.
- `go/cmd/vibe/main.go` — package doc, flag set, and main switch
  reflect single + corpus only.
- `scripts/hosting/cron-vibe.sh` and
  `scripts/hosting/crontab.example` — comments rewritten to describe
  corpus cron + news-volume LISTEN/NOTIFY as the two-layer pipeline.

## Quick reference

### How the new pipeline scores entities

| Trigger source                      | Cadence       | Code path                                            |
|-------------------------------------|---------------|------------------------------------------------------|
| Corpus cron (NBA/NFL/FOOTBALL teams)| 00:00 + 12:00 | `cmd/vibe/main.go runCorpus`                         |
| News-volume spike (5 articles/60m)  | Real-time     | SQL trigger → `news_volume_worker.go StartNewsVolume`|
| Manual one-off (debugging)          | Ad-hoc        | `cmd/vibe/main.go runSingle` (`-entity-type ...`)    |

### Validating the trigger
```
psql "$DATABASE_PRIVATE_URL" -c "
  SELECT tgname, tgenabled
  FROM pg_trigger
  WHERE tgname = 'trg_vibe_trigger_on_news_link';"
```
Direct NOTIFY for an end-to-end sanity check (will run local model against
the entity if it has fresh corpus + isn't inside the 30-min debounce):
```
psql "$DATABASE_PRIVATE_URL" -c "
  SELECT pg_notify('vibe_trigger', json_build_object(
    'entity_type','team','entity_id',9,'sport','NBA',
    'article_count',5,'ts',extract(epoch from now())::bigint
  )::text);"
```
API logs will show `News-volume spike` followed by either `vibe
generated` or `skipped (no corpus inside lookback)`.

### Tuning levers
- Threshold: edit `prior_count = 4` in
  `sql/migrations/011_vibe_trigger_news_volume.sql` (re-run
  `CREATE OR REPLACE FUNCTION` to update without touching the trigger).
- Window: edit the `INTERVAL '60 minutes'` in the same function.
- Debounce: edit `newsVolumeDebounce` in `news_volume_worker.go`.
- Cron cadence: edit the crontab line; `corpus-skip-recent-hours`
  default 10 expects ≤ half the cron cadence.

## Future work

- **Trend rollup endpoint.** `GET /{sport}/vibe/{entityType}/{id}/trend?days=30`
  returning daily-bucketed averages off the existing `vibe_scores`
  history. Cheaper than rendering 50 raw points and visually smoother
  for sparkline-style UI. Frontend can keep using
  `GetVibeHistory` for the full series.
- **Threshold tuning.** Once we have a few weeks of `news_spike`
  trigger data, look at firing rate per sport per day. If NBA fires
  20× per day during the playoffs and FOOTBALL fires 200× per day on
  transfer-window days, the threshold may need to be sport-aware.
- **Dead row cleanup.** `vibe_scores` has 35 v1 + 3058 v2 rows now
  hidden by API filters. They cost ~1 MB on disk and clutter `EXPLAIN`
  plans. Optional `DELETE WHERE prompt_version IN ('v1','v2')` once
  we're confident no consumer needs them. History endpoint already
  hides them, so no UI regression.

## Updated file layout (vibe pipeline)

```
go/
├── cmd/
│   ├── api/main.go                  # spawns Start + StartNewsVolume goroutines
│   └── vibe/main.go                 # single + corpus modes only
├── internal/
│   ├── api/handler/vibe.go          # filters: sentiment ✓, prompt_version, 72h cap
│   ├── listener/
│   │   ├── listener.go              # percentile_changed → FCM
│   │   └── news_volume_worker.go    # vibe_trigger → local model
│   └── ml/vibe.go                   # generator (unchanged)
sql/migrations/
├── 007_vibe_scores.sql              # base table
├── 009_vibe_sentiment.sql           # sentiment column
└── 011_vibe_trigger_news_volume.sql # NEW: notify_vibe_trigger() + trigger + index
scripts/hosting/
├── cron-vibe.sh                     # docs rewritten
└── crontab.example                  # 0 0,12 * * * corpus
```

## Addendum — 2026-05-23: stale-team rescue in corpus

Debugging "Chelsea (football team 18) has had no vibe for 6 days despite
35–64 articles/day" surfaced a starvation pattern in corpus mode that the
new LISTEN/NOTIFY worker compensates for in theory but not always in
practice. Two follow-ups:

1. **Deployed binary was stale.** The production scoracle-api binary was
   built 2026-05-10, before the v3 commit, so the news-volume listener
   never started — the SQL trigger was firing pg_notify into a void. The
   `scoracle-api.path` unit auto-restarts on bin/ changes, so a rebuild
   was the entire fix.

2. **`loadTouchedEntities` was starving popular teams.** Corpus mode
   only queues entities whose `news_article_entities` row was created
   during the current run (after `runStart`). For a team that gets
   continuous user-driven `/news/team/{id}` ingestion between cron
   passes, every Google News URL the noon RSS sweep tries to insert is
   already in `news_articles`, so the run-window filter returns zero
   fresh links and the team is dropped. Added a `stale_teams` CTE that
   UNIONs in teams with any in-lookback article AND no vibe in the last
   18h. Teams-only (small N, ~30-100/sport); headliner players ride
   along via cross-entity linking from the original `from_run` set, and
   real-time player coverage is the news-volume worker's job.

Why 18h: longer than the 12h cron cadence (no clock-drift double-fire
across consecutive runs) and shorter than the 72h read-layer cap (so
a stale team gets refreshed before the read filter would 404 it).
