# Go Lean Sweep + Ingestion Consolidation

**Date:** 2026-08-11 (follows `2026-08-11_docker-seeder-purge.md`)

## Doctrine

Go does what Go does best: speed and simplicity. Google handles relevancy and
data fetching (the nightly RSS sweep is the ONLY ingestion layer); the Rust LLM
junctions curate everything that lands in SQL and gets served to the frontend.
Anything in Go that judged, curated, or re-derived content belonged to the old
regex-enrichment era and was pruned.

## Go code pruned

- **Stage constants 10 → 3** (`internal/work`): kept only what Go enqueues —
  `editor` (ingest), `peak` (percentile listener), `sigil` (vibesynth). Deleted
  `scrub` (gone in PLAN-one-rail 8.8), `article_read` (Editor holds its seat),
  `transfers`/`vibe`/`narratives`/`momentum` (Rust-internal), `fixture_boxscore`
  (enqueued via SQL function, never via Go).
- **`thirdparty/news.go`**: dropped the dead `Handler.news` field,
  `NewsService.Status()`, the always-empty `team`/`firstName`/`lastName`
  params + `buildSearchName` player-name heuristics, the ignored result-map
  return, never-set `Article.Author`/`ImageURL`, and the orphaned
  `broadTeamPrimaryConfidence = 0.95`.
- **Window ladder flattened**: `timeWindows = []int{24}` → `rssLookbackHours = 24`;
  removed the dead breaks, the backwards 100ms sleep gate, `newsMinArticles`,
  `sleepContext`.
- **`corpus.Sweep` returns `(ok, fail)`** — the `affected` map handoff had no
  consumer since the Editor enqueue moved into the persist transaction.
- **Boxscore backfill deleted**: the 6-hour `enqueueRecentFixtureBoxscores`
  ticker + `BoxscoreBackfill*` config keys enqueued `fixture_boxscore` work
  nothing drains (stage not in prod `COGNITION_STAGES`); box scores now flow
  Editor → Investigator. Deleted the 130 orphaned pending queue rows.

## Cron layer consolidated (nightly-only)

- Deleted `cron-live-fixtures.sh` + `cron-scoseed.sh` (purged-seeder wrappers);
  removed the 7 dead live-crontab lines (failing every 30 min with
  `ModuleNotFoundError` since the purge). Backup: `~/crontab.backup-2026-08-11`.
- Collapsed the expired statcommentary rollout gate (Jul 20 epoch) into one
  plain nightly line; aligned narrative-links to nightly 02:45.
- Rebuilt `crontab.example` around the nightly window; removed `.venv` (86M)
  and `seed/` pycache from disk.
- Note: narrative-links cadence is its heating/cooling baseline — classifications
  now measure 24h deltas instead of 6h.

## Model topology documented (RUNBOOK §1.1)

Two machines, two models, model-agnostic by design (routes resolve from
`COGNITION_ROUTE_<ROLE>[_BASE_URL]`; fixture gates prove a candidate voice
before it ships):

- **archbox** (1070 Ti, Ollama): `ministral-3:3b` — low-thought busy work:
  Editor, Investigator, Graph (+ `sql`, `multilang` utility roles)
- **Mac mini** (`192.168.1.77:8000`): `ministral-3:8b` — character work that
  surfaces: Journalist, Insider, Influencer, Analyst, Scout, Oracle

## Verified

Go: `gofmt` clean, `go build ./...`, `go vet ./...`, full `go test ./...` green.
Live queue purged of orphaned rows; crontab installed from backup-protected edit.

## Follow-ups

- SQL function `enqueue_recent_fixture_boxscores` is now uncalled — candidate
  for a future SQL prune (needs a migration).
- `cron-stat-matchups.sh` is in crontab.example but not the live crontab.
- `cron-bucketlabel.sh` + `bucketlabel` bin look like a finished one-shot (F2).
