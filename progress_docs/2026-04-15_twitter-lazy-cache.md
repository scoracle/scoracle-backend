# X (Twitter) Lazy Cache — Phase 1

Date: 2026-04-15

## Goal

Replace the in-memory, single-list journalist-feed cache with a per-sport,
Postgres-backed lazy cache as described in
`/home/sheneveld/Downloads/scoracle-x-api-architecture.md`. X credentials are
not yet available; this phase lands the plumbing so the integration is ready
to flip live when `TWITTER_BEARER_TOKEN` arrives.

## Decisions

- **TTL default:** 20 minutes (`TWITTER_CACHE_TTL_SECONDS=1200`). Configurable
  per list via `twitter_lists.ttl_seconds`.
- **Storage:** three tables — `twitter_lists` (one row per sport),
  `tweets` (short-lived cached rows), `tweet_entities` (entity link table).
- **Concurrency:** `golang.org/x/sync/singleflight` coalesces concurrent
  refreshes of the same sport into one upstream call.
- **Stale-while-revalidate:** on upstream error, the service logs and serves
  the previous cached rows so users never see a hard fail during an X outage.
- **Entity matching:** reuses the existing `search_aliases` matcher from the
  news pipeline. The matcher was extracted from `news.go` into
  `thirdparty/match.go` as `MatchesEntity` so the logic lives in one place.

## Endpoints

| Method | Path                                          | Notes |
|--------|-----------------------------------------------|-------|
| GET    | `/api/v1/{sport}/twitter/feed`                | Lazy-cached sport feed (refreshes when stale) |
| GET    | `/api/v1/{sport}/twitter/{entityType}/{id}`   | Tweets linked to a player/team via `search_aliases` |
| GET    | `/api/v1/twitter/status`                      | Per-sport cache state + config |

Clean break: the legacy `/api/v1/twitter/journalist-feed` route and
`TWITTER_JOURNALIST_LIST_ID` env var were removed — configure per-sport lists
directly.

## File Layout Additions

```
sql/migrations/002_add_twitter_cache.sql       # schema
go/internal/thirdparty/match.go                # shared entity matcher
go/internal/thirdparty/twitter.go              # rewritten lazy-cache service
go/internal/api/handler/twitter.go             # sport + entity handlers
```

Prepared statements added to `go/internal/db/db.go`: `twitter_list_get`,
`twitter_list_upsert`, `twitter_list_mark_fetched`, `twitter_list_mark_error`,
`twitter_list_status_all`, `twitter_tweet_upsert`, `twitter_feed_by_sport`,
`twitter_feed_by_entity`, `twitter_entity_link`, `twitter_entities_for_sport`,
`twitter_tweets_purge`.

## Quick Reference

- Apply migration: `psql $DATABASE_URL -f sql/migrations/002_add_twitter_cache.sql`
- Configure per-sport lists via `TWITTER_LIST_NBA`, `TWITTER_LIST_NFL`,
  `TWITTER_LIST_FOOTBALL`.
- `twitter_tweets_purge` is wired into `db.go` but not yet scheduled; hook into
  `internal/maintenance` when tuning retention.

## Remaining Work (Phase 2+)

- Schedule `twitter_tweets_purge` in maintenance ticker once retention policy
  is chosen.
- Populate `search_aliases` for NFL/NBA players to feed the entity linker.
- Flip live and validate `since_id` behavior + rate-limit cadence once X
  credentials arrive.
