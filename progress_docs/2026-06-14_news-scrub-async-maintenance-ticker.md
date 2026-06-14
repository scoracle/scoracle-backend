# 2026-06-14 — News scrub wired async (maintenance ticker)

Task 3 of the News→Gemma pipeline (companion: `~/scoracleWiki/wiki/Plan - News pipeline
integration.md`, Phase 2 wiring). Built + SQL-validated, committed — **NOT deployed** (needs a
binary rebuild + restart, which is gated). Follows task 1 (foundation deployed) + task 2
(vetted-flag scrub).

## Goal
Run the Gemma scrub automatically + off the request path: a deliberate-cadence sweep that vets
newly-linked `news_article_entities` rows, so consumers can later read the clean (vetted) set.

## Decision — fold into the maintenance ticker (not the reactive listeners)
The vibe + transfer workers are latency-optimized LISTEN/NOTIFY consumers; the transfer worker is
already GPU-bound (`max_concurrent=2`). The scrub is bulk/backlog work, so it belongs on a
deliberate cadence with no cross-contention — the established `maintenance.go` ticker pattern.

## What was done
- **config.go**: `NewsScrubEnabled` (default true), `NewsScrubInterval` (`NEWS_SCRUB_INTERVAL_MINUTES`,
  default 30m), `NewsScrubBatch` (`NEWS_SCRUB_BATCH`, default 15) — `envBool/envInt`, mirroring the
  `TransferEnabled` pattern.
- **maintenance.go**: `Start()` now takes a `*ml.NewsScrubber`; new `news_scrub` ticker (skipped if
  Ollama unreachable → scrubber nil, or interval 0). `scrubNewsLinks()` runs two bounded,
  non-destructive phases per tick:
  1. **Auto-vet primaries** (cheap SQL, no Gemma): `match_confidence >= 1.0` links are the entity the
     article was fetched for — deterministically relevant. Bounded UPDATE (`newsScrubPrimaryBatch =
     20000`/tick) sets `vetted=true, scrubbed_at=NOW()`.
  2. **Gemma pass**: newest `batch` candidate-rich articles (those with an unscrubbed SECONDARY link,
     conf < 1.0 — the disambiguation cases), scrubbed serially via `ScrubArticle(..., persist=true)`.
     Serial = good GPU citizen (Ollama serializes anyway). Per-article errors logged + skipped.
- **main.go**: construct `NewsScrubber` off the existing `ollamaCli` (nil when Ollama unreachable);
  build `maintenance.Config` from `DefaultConfig()` overridden by the cfg scrub knobs; pass the
  scrubber into `maintenance.Start`.

## Why newest-first
Ordering candidates by `published_at DESC` keeps the recent window (what news/vibe/transfers
consumers actually read) scrubbed first; the ancient tail stays `scrubbed_at IS NULL` and is
harmless (consumers filter by recency, and the transition query shows unscrubbed links anyway).

## Verification
- `go build ./...` + `go vet` + `gofmt -l` clean.
- Phase-2 candidate SELECT (read-only on prod): returns the newest 15 candidate-rich articles
  (FOOTBALL + NBA), newest-first.
- Phase-1 auto-vet UPDATE validated in a ROLLED-BACK tx: `UPDATE 20000`, then `ROLLBACK` — SQL valid,
  zero net change (distribution unchanged: 106,279 NULL + the 16 task-2 rows).
- Backlog sizing: 33,906 candidate-rich articles, 61,516 unscrubbed primaries. The ticker drains the
  recent window quickly; the historical tail doesn't need scrubbing.

## State / next
- Committed, NOT deployed. **Deploy = rebuild `bin/scoracle-api` + manual `systemctl --user restart`
  scoracle-api.service** (path-watcher is inert). After deploy: watch the `news_scrub` ticker logs
  ("auto-vetted primary links", "News scrub: swept") + spot-check `vetted`/`scrubbed_at`.
- Then task 4 (flip news/vibe/transfers onto `vetted IS TRUE OR scrubbed_at IS NULL`), task 5 (open
  fuzzy gates + retire 033 together), task 6 (generators into cron/vibe_trigger), task 7 (frontend).
