# Optimization Ledger — serving-surface decommission (O12, O13 routes, O14, O17)

**Date:** 2026-06-19 · Backend (Go; route/handler removal + a rename; **service restarted**, prepared-statement validation passed). No migration (the destructive tweet-table drop is O15, a separate step).

## Goal
Cut the deprecated + decommissioned **serving surface** now that the platform owns all data and the live
web uses only the canonical product endpoints. Verified against every consumer first.

## Consumer safety (verified before removal)
- **Live web** (`scoracle-frontend`) builds `/sigil` + `/momentum` via `entityProductUrl` — the
  `/vibes` + `/trends` strings in `sigil.server.ts`/`momentum.server.ts` are stale labels (O20), not URLs.
- `newsUrl()` (legacy `/news/{type}/{id}`) is imported only by `co-mentions.ts` → the **disconnected**
  `CoMentionsCard` (not in `CARD_REGISTRY`). No live caller. `/news/status` + `/twitter/*`: no live web caller
  (the only `twitter` ref is the X share-intent link).
- **iOS** still calls `/vibes`/`/trends`/`/special` but is **source-only, not shipped**; its convergence
  rename (iB4) is already a launch blocker that repoints it.
- **Astro standby** 72h soak expired ~6 weeks ago (cutover 2026-05-03).

## What Was Done
- **O17 — rename** `GetEntitySigil`→`GetEntityRating` (handler) and the `entity_sigil`→`entity_rating`
  prepared statement. The `/rating` route + payload are unchanged; the Go names now match the product.
- **O14 — drop deprecated aliases** `/{sport}/{type}/{id}/vibes` and `/trends`. Renamed the league
  variant `/leagues/{id}/{type}/{id}/trends` → `/momentum` for full convergence.
- **O12 — remove live-RSS serving routes** `/news/status` + `/news/{entityType}/{entityID}`; deleted
  `handler/news.go` (both handlers + their `lookupPlayer`/`lookupTeam` helpers) and the 2 now-orphaned
  prepared statements `player_news_lookup` + `team_news_lookup`. Kept `team_name_lookup`
  (`notifications/store.go` uses it) and the `thirdparty.NewsService` RSS methods (the corpus pipeline uses them).
- **O13 (routes/handler)** — removed `/twitter/feed`, `/twitter/{type}/{id}`, `/twitter/status`; deleted
  `handler/twitter.go`. The `thirdparty.TwitterService` client, config env, and the tweet tables come out in **O15**.
- **`server_test.go`** — dropped the news/twitter route assertions; repointed the trends assertions to `/momentum`.

## Files Changed
- `go/internal/api/server.go`, `server_test.go`, `handler/data.go`, `internal/db/db.go`
- deleted: `go/internal/api/handler/news.go`, `go/internal/api/handler/twitter.go`

## Verification
- `go vet` clean; `go test ./internal/api/...` PASS (route ownership incl. new momentum assertions).
- Boot on :8001 clean (`entity_rating` + all statements validated, no degraded mode).
- Removed routes → **404** (`/vibes`, `/trends`, `/news/status`, `/news/{type}/{id}`, `/twitter/*`);
  kept routes → **200** (`/rating`, `/sigil`, `/momentum`, `/news`, `/leaderboard/sigil`). Prod health 200.

## Result
O17 ✅, O14 ✅, O12 ✅, O13 routes/handler ✅. Remaining for the X decommission: **O15** (migration —
redefine transfer-heat sans tweets, drop `tweets`/`tweet_entities`, remove `purgeTweets` + Twitter config/client).
ENDPOINTS.md + swagger reconcile (dead-route removal, trends→momentum, rating_specialist→rating_peak) is
folded into the **O25** pass. iOS `/vibes`/`/trends` calls now 404 — tracked by the existing iB4 blocker.
