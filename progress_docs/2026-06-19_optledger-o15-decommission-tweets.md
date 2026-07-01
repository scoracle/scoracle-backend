# Optimization Ledger O15 — drop the tweet arm (X permanently decommissioned)

**Date:** 2026-06-19 · Backend (migration `098` + Go cleanup; **two-step deploy** — Go binary first, then migration). Completes the X decommission begun in the O13 route/handler removal.

## Goal
X/Twitter is permanently decommissioned (user decision; parked since 2026-06-13). Remove the last of the
tweet infrastructure: the transfer-heat tweet arm, the empty tweet tables, the orphaned prepared
statements, the purge ticker, the thirdparty client, and the config.

## Two-step ordering (zero-downtime, no degraded mode)
The running binary prepares every statement at boot, and `compute_transfer_heat` referenced the tweet
tables — so the Go change had to land **before** the table drop:
1. **Deploy the Go binary** that no longer registers any `twitter_*` prepared statement and no longer
   references the tweet tables. (Tables still present; the old `compute_transfer_heat` keeps working.)
2. **Apply migration 098**: redefine `compute_transfer_heat` to drop its tweets UNION arm, *then* drop
   the (empty) `tweets` / `tweet_entities` / `twitter_lists` tables.

## What Was Done
- **Migration 098** — `compute_transfer_heat` redefined news-only (tweets UNION arm removed; `tweet_ids`
  OUT retained, now always `'{}'`). Dropped `tweet_entities`, `tweets`, `twitter_lists` (all empty since the park).
- **db.go** — removed the entire `twitter_*` prepared-statement block (11 statements: list get/upsert/
  mark/status, tweet upsert/feed/entity-link, tweets purge).
- **maintenance.go** — removed `purgeTweets` + the `TweetTTLInterval`/`TweetTTL` config fields, defaults,
  log keys, and the tweet-TTL ticker.
- **handler.go** — removed the `twitter *thirdparty.TwitterService` field + its constructor.
- **main.go** — removed the `twitter_lists` startup sync (+ the now-unused `thirdparty` import).
- **config.go** — removed `TwitterEnabled`/`TwitterBearerToken`/`TwitterLists`/`TwitterCacheTTL` + their
  env parsing + `loadTwitterLists()`.
- **deleted** `thirdparty/twitter.go` (the whole `TwitterService` + tweet types).
- **.env** template — removed the inert `TWITTER_*` block.

The transfer worker (`ml/transfer.go`) keeps its (now-inert) tweet plumbing: `compute_transfer_heat`
returns an empty `tweet_ids`, and `loadPairTweets` already short-circuits on an empty id list, so the
dropped tables are never queried. Trivial future cleanup: drop the vestigial `tweet_ids` OUT param +
`transfer_rumors.input_tweet_ids` and the inert worker plumbing.

## Files Changed
- `sql/migrations/098_decommission_tweets.sql` (new)
- `go/internal/db/db.go`, `internal/maintenance/maintenance.go`, `internal/api/handler/handler.go`,
  `cmd/api/main.go`, `internal/config/config.go`, `.env`
- deleted: `go/internal/thirdparty/twitter.go`

## Verification
- `go build`, `go vet`, `go test ./internal/config/... ./internal/api/...` all pass.
- Boot on :8001 clean (no `twitter_*` statements, no degraded mode); `/transfers` + `/news` 200.
- Migration applied: `tweets`/`tweet_entities`/`twitter_lists` → gone; `compute_transfer_heat` no longer
  references tweets and still returns heat (real FOOTBALL pair: heat=4, `tweet_ids={}`, 3 news_ids).
- Post-deploy **clean restart** with the tables gone: active, health 200, no degraded / missing-table errors.

## Result
O13 + O15 ✅ — X/Twitter fully decommissioned end-to-end. The transfer-heat pipeline now derives purely
from the Google-RSS → local model corpus, matching the platform's owned-data model.
